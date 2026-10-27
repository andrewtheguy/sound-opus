//! The encoder over made sounds and the recordings, at a grid of rates and
//! settings, a JSON object a line (JSONL) for `tools/tables.py` to make the
//! tables of `docs/benchmarks.md` from:
//!
//!     cargo run --release --example bench > bench.jsonl
//!     python3 -I tools/tables.py bench.jsonl
//!
//! Every sound is the stream both users send, 48 kHz stereo of 16-bit
//! samples, coded in 20 ms blocks. The made sounds are fifty blocks each of a
//! tone, music, speech, noise and silence, as sound-flac's bench makes them;
//! a cell's time is the least of five passes' mean block, each block timed on
//! its own, after one pass to warm up. Then every WAV under `tmp/samples` (or
//! `SAMPLES`), the recordings of sound-flac's `tools/samples.txt`, is brought
//! to that stream by ffmpeg (resampled where it is not at 48 kHz, a mono one
//! sent as both channels, as a desktop plays it) and coded whole, three passes
//! after one to warm up, each pass from an encoder that has seen no block, so
//! that what an encoder carries from block to block falls on the same blocks
//! in every pass: a recording's time is the least pass's mean block, and with
//! it the least of the passes' 99th-percentile and worst blocks, the latency
//! a stream sees, and `worst_at`, the block that was slowest at its least
//! over the passes, which is the encoder's doing where a pass's worst may be
//! the machine's. Every pass's packets are the warm-up's, which is checked.
//!
//! A cell's bytes are the packets' mean, with their 99th percentile and
//! largest, and `peak_kbit`, the most any second of them (fifty packets
//! running) came to: what a link at the rate meets. `fb`, `celt` and `stereo`
//! are the shares of packets whose table of contents says fullband, CELT
//! alone and stereo.
//!
//! Quality is measured through an independent decoder, FFmpeg's own Opus
//! decoder as the tests use it, against the sound that went in, the encoder's
//! lookahead taken off the front and each frame's and block's mean off both,
//! as libopus takes an offset out before it codes. Both figures are proxies,
//! not listening:
//! `lsd` is the log-spectral distance in dB over the 25 critical bands up to
//! 20 kHz, of each channel, over 2048-sample Hann frames half overlapped,
//! averaged over the frames of the source above −60 dBFS; `lsd_side` the same
//! of the side, (left − right) / 2, over the frames whose side is above
//! −60 dBFS and within 30 dB of the channels, so a stereo image folded
//! toward mono shows and a mono recording's noise does not (`null` where the
//! source has none); `segsnr` the segmental signal-to-noise ratio in dB over
//! 20 ms blocks above −60 dBFS, each held to −10..60 dB, the blocks of each
//! coding mode at that mode's best alignment (FFmpeg's decoder puts SILK's
//! and hybrid's sound a few samples early). Lower `lsd` and higher `segsnr`
//! are nearer the source.
//!
//! `RATES` names the rates in kbit/s (`RATES=64,128`; by default 32 to 320,
//! the floor of the walk to zdesk's ceiling). `SETTINGS` names the encoders:
//! `crate`, this crate's own `Encoder`, the default; or `c<N>-<application>-<vbr>`,
//! libopus set to complexity N, `audio` or `lowdelay`
//! (`OPUS_APPLICATION_RESTRICTED_LOWDELAY`), and `cvbr` (constrained VBR),
//! `vbr` or `cbr`, the crate's other settings unchanged. Before anything is
//! measured the harness checks that `c10-audio-cvbr` makes the crate's own
//! packets, so a setting that differs from it in one word differs from the
//! crate in that alone. `KINDS` narrows the made sounds (`KINDS=music,tone`,
//! `KINDS=` for none), `FILES` the recordings by substring of their path.
//! `KEEP=dir` writes every cell's packets there as an Ogg Opus file,
//! `<setting>-<kbit>/<sound>.opus`, to listen to.
use std::f64::consts::TAU;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use opus::{Application, Bitrate, Channels};
use sound_opus::{Encoder, MAX_PACKET, Stream};

const STEREO: Stream = Stream { rate: 48_000, channels: 2 };

/// The rates, in kbit/s, of a run that names none: the walk's floor up to
/// zdesk's ceiling.
const RATES: [u32; 9] = [32, 48, 64, 96, 128, 160, 192, 256, 320];

const KINDS: [&str; 5] = ["music", "speech", "tone", "noise", "silence"];

/// The frame of the spectra, and how far one starts after the last.
const N: usize = 2048;
const HOP: usize = N / 2;

/// The edges of the critical bands, Zwicker's, in Hz, to 20 kHz, where
/// fullband Opus stops.
const EDGES: [f64; 26] = [
    0.0, 100.0, 200.0, 300.0, 400.0, 510.0, 630.0, 770.0, 920.0, 1080.0, 1270.0, 1480.0, 1720.0, 2000.0, 2320.0, 2700.0, 3150.0, 3700.0, 4400.0, 5300.0, 6400.0, 7700.0,
    9500.0, 12000.0, 15500.0, 20000.0,
];
const BANDS: usize = EDGES.len() - 1;

/// What a bin of a band counts as at the least, samples at full scale being
/// 1: about 12 dB over a 16-bit stream's own rounding noise in a Hann frame,
/// so that a band the source does not have is not counted as one lost.
const FLOOR_BIN: f64 = 1e-6;

/// −30 dB, as a ratio of powers: how far under the channels a frame's side
/// may be and still count.
const SIDE: f64 = 1e-3;

/// −60 dBFS as a mean square, samples at full scale being 1: what a frame or
/// block of the source has to be over to count.
const LOUD: f64 = 1e-6;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    }
}

/// `blocks` blocks of one kind of sound, interleaved stereo: sound-flac's
/// bench's made sounds.
fn signal(blocks: usize, kind: &str) -> Vec<i16> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let n = blocks * STEREO.block();
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f64 / f64::from(STEREO.rate);
        let (l, r) = match kind {
            "silence" => (0.0, 0.0),
            // A chord with harmonics and slow vibrato, a bass note, a little
            // room noise, and a right channel that is the left mixed with
            // another note: music.
            "music" => {
                let vib = 1.0 + 0.003 * (t * 5.5 * TAU).sin();
                let mut l = 0.0;
                for (k, f) in [220.0, 277.18, 329.63].iter().enumerate() {
                    for h in 1..=5 {
                        l += (0.3 / (h as f64 * (k + 1) as f64)) * (t * f * vib * h as f64 * TAU).sin();
                    }
                }
                let bass = 0.2 * (t * 55.0 * TAU).sin();
                let r = 0.8 * l + 0.15 * (t * 440.0 * TAU).sin() + bass;
                (l + bass + 0.002 * rng.next(), r + 0.002 * rng.next())
            }
            // One voice-like tone, its pitch and loudness moving, with breath
            // noise, the same on both channels.
            "speech" => {
                let f0 = 120.0 + 30.0 * (t * 2.0 * TAU).sin();
                let mut v = 0.0;
                for h in 1..=12 {
                    v += (0.25 / h as f64) * (t * f0 * h as f64 * TAU + 0.3 * h as f64).sin();
                }
                let env = 0.5 + 0.5 * (t * 3.0 * TAU).sin();
                let v = v * env + 0.01 * rng.next();
                (v, v)
            }
            // A pure tone at -6 dBFS, a notification's beep.
            "tone" => {
                let v = 0.5 * (t * 1000.0 * TAU).sin();
                (v, 0.7 * v)
            }
            // Uniform noise peaking at -26 dBFS: nothing to predict.
            "noise" => (0.1 * rng.next(), 0.1 * rng.next()),
            _ => unreachable!(),
        };
        for v in [l, r] {
            out.push((v.clamp(-1.0, 1.0) * 32_767.0).round() as i16);
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Vbr {
    Constrained,
    Free,
    Constant,
}

/// libopus as the harness sets it, for the settings the crate does not have.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Setting {
    complexity: i32,
    lowdelay: bool,
    vbr: Vbr,
}

impl Setting {
    /// What the crate sets, which the harness checks before it measures.
    const CRATE: Setting = Setting { complexity: 10, lowdelay: false, vbr: Vbr::Constrained };

    fn parse(text: &str) -> Setting {
        let bad = || panic!("{text}: a setting is `crate` or c<0-10>-<audio|lowdelay>-<cvbr|vbr|cbr>");
        let mut words = text.split('-');
        let (Some(c), Some(application), Some(vbr), None) = (words.next(), words.next(), words.next(), words.next()) else { bad() };
        let complexity = c.strip_prefix('c').and_then(|n| n.parse().ok()).filter(|n| (0..=10).contains(n)).unwrap_or_else(|| bad());
        let lowdelay = match application {
            "audio" => false,
            "lowdelay" => true,
            _ => bad(),
        };
        let vbr = match vbr {
            "cvbr" => Vbr::Constrained,
            "vbr" => Vbr::Free,
            "cbr" => Vbr::Constant,
            _ => bad(),
        };
        Setting { complexity, lowdelay, vbr }
    }

    fn name(&self) -> String {
        let application = if self.lowdelay { "lowdelay" } else { "audio" };
        let vbr = match self.vbr {
            Vbr::Constrained => "cvbr",
            Vbr::Free => "vbr",
            Vbr::Constant => "cbr",
        };
        format!("c{}-{application}-{vbr}", self.complexity)
    }
}

/// A cell's encoder: the crate's, or libopus set as a `Setting` says.
enum Coder {
    Crate(Encoder),
    Raw(opus::Encoder, Box<[u8; MAX_PACKET]>),
}

impl Coder {
    fn new(setting: Option<Setting>, kbit: u32) -> Coder {
        let bitrate = kbit * 1000;
        let Some(setting) = setting else { return Coder::Crate(Encoder::new(STEREO, bitrate).unwrap()) };
        let application = if setting.lowdelay { Application::LowDelay } else { Application::Audio };
        let mut encoder = opus::Encoder::new(STEREO.rate, Channels::Stereo, application).unwrap();
        encoder.set_vbr(setting.vbr != Vbr::Constant).unwrap();
        encoder.set_vbr_constraint(setting.vbr == Vbr::Constrained).unwrap();
        encoder.set_bitrate(Bitrate::Bits(bitrate as i32)).unwrap();
        encoder.set_complexity(setting.complexity).unwrap();
        Coder::Raw(encoder, Box::new([0; MAX_PACKET]))
    }

    /// Append one block's packet to `out`, as the crate does.
    fn encode(&mut self, block: &[i16], out: &mut Vec<u8>) {
        match self {
            Coder::Crate(encoder) => encoder.encode(block, out).unwrap(),
            Coder::Raw(encoder, packet) => {
                let length = encoder.encode(block, &mut packet[..]).unwrap();
                out.extend_from_slice(&packet[..length]);
            }
        }
    }

    /// The samples a decoder's output is behind the sound that went in.
    fn lookahead(&mut self) -> usize {
        match self {
            Coder::Crate(encoder) => usize::from(encoder.pre_skip().unwrap()),
            Coder::Raw(encoder, _) => encoder.get_lookahead().unwrap() as usize,
        }
    }
}

/// The harness's `c10-audio-cvbr` makes the crate's own packets, or nothing
/// it measures of a setting is of the crate with that one thing changed.
fn check_crate() {
    let pcm = signal(50, "music");
    for kbit in [32, 96, 320] {
        let (mut ours, mut theirs) = (Coder::new(Some(Setting::CRATE), kbit), Coder::new(None, kbit));
        for block in pcm.chunks_exact(STEREO.samples()) {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            ours.encode(block, &mut a);
            theirs.encode(block, &mut b);
            assert_eq!(a, b, "the harness's {} is not the crate's encoder at {kbit} kbit/s", Setting::CRATE.name());
        }
    }
}

struct Measured {
    us: f64,
    p99: f64,
    worst: f64,
    /// The block that was slowest at its least over the passes.
    worst_at: usize,
    packets: Vec<Vec<u8>>,
    lookahead: usize,
}

/// One encoder over the blocks of `pcm`: the least of `passes` passes' mean
/// block in microseconds, and the least of their 99th-percentile and worst
/// blocks, each block timed on its own after one pass to warm up, whose
/// packets every pass is checked against. A pass runs for `least_ms` at the
/// least, so a short sound is timed long enough.
fn measure(setting: Option<Setting>, kbit: u32, pcm: &[i16], passes: usize, least_ms: u128) -> Measured {
    let samples = STEREO.samples();
    let blocks = pcm.len() / samples;
    let mut coder = Coder::new(setting, kbit);
    let lookahead = coder.lookahead();
    let mut out = Vec::with_capacity(MAX_PACKET);
    let packets: Vec<Vec<u8>> = pcm
        .chunks_exact(samples)
        .map(|block| {
            out.clear();
            coder.encode(block, &mut out);
            out.clone()
        })
        .collect();
    let mut times = vec![0f64; blocks];
    let mut least = vec![f64::INFINITY; blocks];
    let (mut mean, mut p99, mut worst) = (f64::INFINITY, f64::INFINITY, f64::INFINITY);
    for _ in 0..passes {
        let start = Instant::now();
        let (mut sum, mut done, mut p, mut w) = (0.0, 0, f64::INFINITY, f64::INFINITY);
        while done == 0 || start.elapsed().as_millis() < least_ms {
            let mut coder = Coder::new(setting, kbit);
            for ((block, time), packet) in pcm.chunks_exact(samples).zip(times.iter_mut()).zip(&packets) {
                // Each block into an empty buffer, as the warm-up coded it.
                out.clear();
                let start = Instant::now();
                coder.encode(block, &mut out);
                *time = start.elapsed().as_secs_f64() * 1e6;
                assert_eq!(&out, packet, "a pass made other packets than the warm-up");
            }
            sum += times.iter().sum::<f64>();
            for (l, &t) in least.iter_mut().zip(&times) {
                *l = l.min(t);
            }
            let mut sorted = times.clone();
            sorted.sort_by(f64::total_cmp);
            p = p.min(sorted[(blocks * 99 / 100).min(blocks - 1)]);
            w = w.min(sorted[blocks - 1]);
            done += 1;
        }
        mean = mean.min(sum / (done * blocks) as f64);
        p99 = p99.min(p);
        worst = worst.min(w);
    }
    let worst_at = (0..blocks).max_by(|&a, &b| least[a].total_cmp(&least[b])).unwrap_or(0);
    Measured { us: mean, p99, worst, worst_at, packets, lookahead }
}

/// The packets' sizes and what their tables of contents say, as a line's
/// JSON members.
fn sizes(packets: &[Vec<u8>]) -> String {
    let mut sizes: Vec<usize> = packets.iter().map(Vec::len).collect();
    let mean = sizes.iter().sum::<usize>() as f64 / sizes.len() as f64;
    // Fifty packets are a second, so a window's bytes are its kbit/s at
    // eight bits to the byte over a thousand.
    let second = sizes.len().min(50);
    let mut window: usize = sizes[..second].iter().sum();
    let mut peak = window;
    for i in second..sizes.len() {
        window = window + sizes[i] - sizes[i - second];
        peak = peak.max(window);
    }
    let peak_kbit = peak as f64 * 8.0 / 1000.0 * 50.0 / second as f64;
    sizes.sort_unstable();
    let p99 = sizes[(sizes.len() * 99 / 100).min(sizes.len() - 1)];
    let max = sizes[sizes.len() - 1];
    // RFC 6716's table of contents: the configuration in the top five bits,
    // 0-11 SILK, 12-15 hybrid, 16-31 CELT, each fullband at the top of its
    // run; the stereo flag under it.
    let share = |test: &dyn Fn(u8) -> bool| packets.iter().filter(|p| test(p[0])).count() as f64 / packets.len() as f64;
    let fb = share(&|toc| matches!(toc >> 3, 14 | 15 | 28..=31));
    let celt = share(&|toc| toc >> 3 >= 16);
    let stereo = share(&|toc| toc & 4 != 0);
    format!("\"bytes\":{mean:.1},\"p99_bytes\":{p99},\"max_bytes\":{max},\"peak_kbit\":{peak_kbit:.1},\"fb\":{fb:.3},\"celt\":{celt:.3},\"stereo\":{stereo:.3}")
}

/// A radix-2 FFT of `N` points, with the Hann window the spectra are taken
/// through.
struct Fft {
    reversed: Vec<usize>,
    cos: Vec<f64>,
    sin: Vec<f64>,
    window: Vec<f64>,
    /// The critical band of each bin up to half the rate, if any.
    band: Vec<Option<usize>>,
    /// What each band counts as at the least.
    floor: [f64; BANDS],
}

impl Fft {
    fn new() -> Fft {
        let bits = N.trailing_zeros();
        let reversed = (0..N).map(|i| i.reverse_bits() >> (usize::BITS - bits)).collect();
        let (cos, sin) = (0..N / 2).map(|k| (-TAU * k as f64 / N as f64).sin_cos()).map(|(s, c)| (c, s)).unzip();
        let window = (0..N).map(|i| 0.5 - 0.5 * (TAU * i as f64 / N as f64).cos()).collect();
        let hz = f64::from(STEREO.rate) / N as f64;
        // The bin at 0 Hz is no band's: an offset is not sound.
        let band: Vec<Option<usize>> = (0..=N / 2).map(|k| if k == 0 { None } else { EDGES.windows(2).position(|e| (e[0]..e[1]).contains(&(k as f64 * hz))) }).collect();
        let mut floor = [0.0; BANDS];
        for b in band.iter().flatten() {
            floor[*b] += FLOOR_BIN;
        }
        Fft { reversed, cos, sin, window, band, floor }
    }

    fn transform(&self, re: &mut [f64], im: &mut [f64]) {
        for (i, &j) in self.reversed.iter().enumerate() {
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= N {
            let (half, step) = (len / 2, N / len);
            for start in (0..N).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let (a, b) = (start + k, start + k + half);
                    let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                    re[b] = re[a] - xr;
                    im[b] = im[a] - xi;
                    re[a] += xr;
                    im[a] += xi;
                }
            }
            len *= 2;
        }
    }

    /// The band powers of left, right and side of every frame of `pcm`, and
    /// whether the frame and its side are loud enough to count.
    fn spectra(&self, pcm: &[i16]) -> Vec<Frame> {
        let frames = pcm.len() / 2;
        if frames < N {
            return Vec::new();
        }
        let (mut re, mut im) = (vec![0.0; N], vec![0.0; N]);
        (0..(frames - N) / HOP + 1)
            .map(|f| {
                let start = f * HOP;
                let (mut all, mut side) = (0.0, 0.0);
                let [left, right] = means(&pcm[2 * start..2 * (start + N)]);
                // Left and right as one complex signal, two real transforms
                // for one.
                for (i, w) in self.window.iter().enumerate() {
                    let l = f64::from(pcm[2 * (start + i)]) / 32_768.0 - left;
                    let r = f64::from(pcm[2 * (start + i) + 1]) / 32_768.0 - right;
                    all += l * l + r * r;
                    side += (l - r) * (l - r) / 4.0;
                    re[i] = l * w;
                    im[i] = r * w;
                }
                self.transform(&mut re, &mut im);
                let mut power = [[0.0; BANDS]; 3];
                for (k, band) in self.band.iter().enumerate() {
                    let Some(b) = *band else { continue };
                    let (zr, zi) = (re[k], im[k]);
                    let (cr, ci) = (re[(N - k) % N], -im[(N - k) % N]);
                    // Left is (Z[k] + conj Z[N-k]) / 2, right the difference
                    // over 2i, the side half the difference of the two.
                    let (lr, li) = ((zr + cr) / 2.0, (zi + ci) / 2.0);
                    let (rr, ri) = ((zi - ci) / 2.0, -(zr - cr) / 2.0);
                    let (sr, si) = ((lr - rr) / 2.0, (li - ri) / 2.0);
                    power[0][b] += lr * lr + li * li;
                    power[1][b] += rr * rr + ri * ri;
                    power[2][b] += sr * sr + si * si;
                }
                // A side 30 dB under the channels is a mono recording's noise,
                // not a stereo image.
                Frame { power, loud: all / (2 * N) as f64 > LOUD, side_loud: side / N as f64 > LOUD && side > SIDE * all / 2.0 }
            })
            .collect()
    }

    /// The log-spectral distance in dB between two frames' bands.
    fn lsd(&self, a: &[f64; BANDS], b: &[f64; BANDS]) -> f64 {
        let sum: f64 = a.iter().zip(b).zip(&self.floor).map(|((a, b), floor)| (10.0 * ((a + floor) / (b + floor)).log10()).powi(2)).sum();
        (sum / BANDS as f64).sqrt()
    }
}

struct Frame {
    power: [[f64; BANDS]; 3],
    loud: bool,
    side_loud: bool,
}

/// The mean of each channel of interleaved stereo, full scale being 1: an
/// offset libopus takes out before it codes (its DC reject, in
/// `OPUS_APPLICATION_AUDIO`), so that neither proxy counts it, taken off
/// each frame and block of both signals. One recording has an offset of a
/// quarter of full scale.
fn means(pcm: &[i16]) -> [f64; 2] {
    let mut sum = [0.0; 2];
    for (i, &s) in pcm.iter().enumerate() {
        sum[i % 2] += f64::from(s);
    }
    sum.map(|s| s / 32_768.0 / (pcm.len() / 2) as f64)
}

/// The segmental SNR of one block of the source, `block` of `pcm`, against
/// `decoded` that many samples a channel later, held to −10..60 dB, or
/// nothing where the block is not loud enough to count.
fn block_snr(pcm: &[i16], decoded: &[i16], block: usize, offset: usize) -> Option<f64> {
    let samples = STEREO.samples();
    let source = &pcm[block * samples..(block + 1) * samples];
    let decoded = &decoded[block * samples + offset * 2..][..samples];
    let (ours, theirs) = (means(source), means(decoded));
    let (mut signal, mut noise) = (0.0, 0.0);
    for (i, (&s, &d)) in source.iter().zip(decoded).enumerate() {
        let (s, d) = (f64::from(s) / 32_768.0 - ours[i % 2], f64::from(d) / 32_768.0 - theirs[i % 2]);
        signal += s * s;
        noise += (s - d) * (s - d);
    }
    (signal / samples as f64 > LOUD).then(|| if noise == 0.0 { 60.0 } else { (10.0 * (signal / noise).log10()).clamp(-10.0, 60.0) })
}

/// `lsd`, `lsd_side` and `segsnr` of `decoded`, every sample the decoder
/// made of `packets`, against `pcm`, the sound that went in, as a line's JSON
/// members.
fn quality(fft: &Fft, source: &[Frame], pcm: &[i16], packets: &[Vec<u8>], decoded: &[i16], lookahead: usize) -> String {
    let mean = |values: Vec<f64>| if values.is_empty() { "null".to_owned() } else { format!("{:.3}", values.iter().sum::<f64>() / values.len() as f64) };
    // The spectra's frames are long enough that the lookahead aligns them.
    let theirs = fft.spectra(&decoded[lookahead * 2..]);
    let pairs = || source.iter().zip(&theirs);
    let lsd = pairs().filter(|(s, _)| s.loud).map(|(s, d)| (fft.lsd(&s.power[0], &d.power[0]) + fft.lsd(&s.power[1], &d.power[1])) / 2.0).collect();
    let lsd_side = pairs().filter(|(s, _)| s.side_loud).map(|(s, d)| fft.lsd(&s.power[2], &d.power[2])).collect();
    // A waveform's are not: FFmpeg's decoder puts the sound of a SILK or
    // hybrid packet three or four samples before where it puts a CELT
    // packet's, which libopus's own decoder does not. So the blocks of each
    // mode are compared at the offset, near the lookahead, at which that
    // mode's blocks match best over the whole sound, searched on every
    // eighth. A block is its packet's mode; the last leaves room to search.
    let blocks = packets.len() - 1;
    let celt: Vec<bool> = packets.iter().map(|p| p[0] >> 3 >= 16).collect();
    let near = lookahead.saturating_sub(16)..=lookahead + 4;
    let best = |mode: bool| {
        let score = |offset: usize| (0..blocks).step_by(8).filter(|&b| celt[b] == mode).filter_map(|b| block_snr(pcm, decoded, b, offset)).sum::<f64>();
        near.clone().max_by(|&a, &b| score(a).total_cmp(&score(b)).then(b.abs_diff(lookahead).cmp(&a.abs_diff(lookahead)))).unwrap()
    };
    let offsets = [best(false), best(true)];
    let segsnr = (0..blocks).filter_map(|b| block_snr(pcm, decoded, b, offsets[usize::from(celt[b])])).collect();
    format!("\"lsd\":{},\"lsd_side\":{},\"segsnr\":{}", mean(lsd), mean(lsd_side), mean(segsnr))
}

/// One Ogg page holding `packet` whole, as RFC 3533 lays a page out, as the
/// crate's tests make one.
fn page(kind: u8, granule: u64, sequence: u32, packet: &[u8]) -> Vec<u8> {
    let mut page = Vec::from(*b"OggS");
    page.extend_from_slice(&[0, kind]);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&1u32.to_le_bytes());
    page.extend_from_slice(&sequence.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    let full = packet.len() / 255;
    page.push(full as u8 + 1);
    page.extend(std::iter::repeat_n(255, full));
    page.push((packet.len() % 255) as u8);
    page.extend_from_slice(packet);
    let mut crc = 0u32;
    for &byte in &page {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04c1_1db7 } else { crc << 1 };
        }
    }
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

/// The packets as an Ogg Opus file, a page to a packet, its head saying to
/// discard `pre_skip` samples.
fn ogg(packets: &[Vec<u8>], pre_skip: u16) -> Vec<u8> {
    let mut file = page(2, 0, 0, &STEREO.head(pre_skip, STEREO.rate).unwrap());
    file.extend(page(0, 0, 1, b"OpusTags\x05\0\0\0bench\0\0\0\0"));
    for (n, packet) in packets.iter().enumerate() {
        let last = if n + 1 == packets.len() { 4 } else { 0 };
        file.extend(page(last, (n as u64 + 1) * 960, n as u32 + 2, packet));
    }
    file
}

/// What ffmpeg writes to its output of `input` as 16-bit samples, `args`
/// before and after the input.
fn ffmpeg(before: &[&str], input: &str, after: &[&str], stdin: Option<Vec<u8>>) -> Vec<i16> {
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error"])
        .args(before)
        .args(["-i", input])
        .args(after)
        .args(["-f", "s16le", "pipe:1"])
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .spawn()
        .expect("the bench decodes with ffmpeg, which must be installed");
    let writer = stdin.map(|bytes| {
        let mut pipe = child.stdin.take().unwrap();
        std::thread::spawn(move || pipe.write_all(&bytes))
    });
    let output = child.wait_with_output().unwrap();
    if let Some(writer) = writer {
        writer.join().unwrap().unwrap();
    }
    assert!(output.status.success(), "ffmpeg failed on {input}");
    output.stdout.as_chunks::<2>().0.iter().map(|&pair| i16::from_le_bytes(pair)).collect()
}

/// Decode with FFmpeg's own Opus decoder, which shares nothing with libopus,
/// keeping every sample.
fn decode(packets: &[Vec<u8>]) -> Vec<i16> {
    // `-c:a opus` before the input names the decoder: FFmpeg's, not its
    // wrapper around libopus.
    let decoded = ffmpeg(&["-c:a", "opus"], "pipe:0", &["-ar", "48000", "-ac", "2"], Some(ogg(packets, 0)));
    assert_eq!(decoded.len(), packets.len() * STEREO.samples(), "every packet is twenty milliseconds");
    decoded
}

/// A recording as the stream the users send, in whole blocks: 48 kHz, and a
/// mono one on both channels at its own level, as a desktop plays it.
fn recording(path: &Path) -> Vec<i16> {
    let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels", "-of", "csv=p=0"]).arg(path).output().unwrap();
    let channels: u32 = String::from_utf8_lossy(&probe.stdout).trim().parse().unwrap_or_else(|_| panic!("{}: no channels", path.display()));
    let mix = match channels {
        1 => "pan=stereo|c0=c0|c1=c0",
        2 => "anull",
        n => panic!("{}: {n} channels", path.display()),
    };
    let mut pcm = ffmpeg(&[], &path.to_string_lossy(), &["-af", mix, "-ar", "48000", "-ac", "2"], None);
    pcm.truncate(pcm.len() / STEREO.samples() * STEREO.samples());
    pcm
}

fn wavs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            wavs(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
            out.push(path);
        }
    }
}

/// A string as a JSON string, quoted and escaped.
fn json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What a run measures every sound at, and where it keeps the packets.
struct Grid {
    fft: Fft,
    settings: Vec<Option<Setting>>,
    rates: Vec<u32>,
    keep: Option<PathBuf>,
}

impl Grid {
    /// One sound at every setting and rate, a line each: `what` is the line's
    /// member naming the sound.
    fn cells(&self, pcm: &[i16], what: &str, name: &str, passes: usize, least_ms: u128) {
        let fft = &self.fft;
        let source = fft.spectra(pcm);
        for &setting in &self.settings {
            let setting_name = setting.map_or_else(|| "crate".to_owned(), |s| s.name());
            for &kbit in &self.rates {
                let start = Instant::now();
                let measured = measure(setting, kbit, pcm, passes, least_ms);
                let decoded = decode(&measured.packets);
                let quality = quality(fft, &source, pcm, &measured.packets, &decoded, measured.lookahead);
                if let Some(dir) = &self.keep {
                    let dir = dir.join(format!("{setting_name}-{kbit}"));
                    std::fs::create_dir_all(&dir).unwrap();
                    let file: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
                    std::fs::write(dir.join(format!("{file}.opus")), ogg(&measured.packets, measured.lookahead as u16)).unwrap();
                }
                println!(
                    "{{\"setting\":{},\"kbit\":{kbit},{what}:{},\"seconds\":{:.1},\"lookahead\":{},\"us\":{:.2},\"p99\":{:.2},\"worst\":{:.2},\"worst_at\":{},{},{quality}}}",
                    json(&setting_name),
                    json(name),
                    pcm.len() as f64 / STEREO.samples() as f64 / 50.0,
                    measured.lookahead,
                    measured.us,
                    measured.p99,
                    measured.worst,
                    measured.worst_at,
                    sizes(&measured.packets),
                );
                std::io::stdout().flush().unwrap();
                eprintln!("{name} {setting_name} {kbit}: {:.1?}", start.elapsed());
            }
        }
    }
}

fn main() {
    let list = |var: &str| std::env::var(var).ok().map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect::<Vec<_>>());
    let rates: Vec<u32> = list("RATES").map_or_else(|| RATES.to_vec(), |r| r.iter().map(|k| k.parse().expect("RATES are kbit/s")).collect());
    let settings: Vec<Option<Setting>> = list("SETTINGS").unwrap_or_else(|| vec!["crate".to_owned()]).iter().map(|s| (s != "crate").then(|| Setting::parse(s))).collect();
    check_crate();
    let grid = Grid { fft: Fft::new(), settings, rates, keep: std::env::var_os("KEEP").map(PathBuf::from) };
    for kind in list("KINDS").unwrap_or_else(|| KINDS.iter().map(|k| k.to_string()).collect()) {
        grid.cells(&signal(50, &kind), "\"kind\"", &kind, 5, 60);
    }
    let dir = std::env::var("SAMPLES").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("tmp/samples"));
    let mut files = Vec::new();
    wavs(&dir, &mut files);
    files.sort();
    let wanted = list("FILES").unwrap_or_default();
    for path in files {
        let file = path.strip_prefix(&dir).unwrap_or(&path).to_string_lossy().replace(' ', "_");
        if !wanted.is_empty() && !wanted.iter().any(|w| file.contains(w.as_str())) {
            continue;
        }
        grid.cells(&recording(&path), "\"file\"", &file, 3, 0);
    }
}
