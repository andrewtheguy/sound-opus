//! The walk on a modelled link: a recording played as the zdesk gateway plays
//! a desktop's sound to a browser, through the encoder at the rates the walk
//! arrives at, over a link of a given rate and round trip. One JSON object a
//! line (JSONL) a run, for `tools/walk.py` to make the tables of
//! `docs/walk.md` from.
//!
//! ```text
//! walk-sim RECORDING [--source rdp|wlshare] [--link KBPS[,KBPS@SECS...]]
//!          [--rtt MS] [--jitter MS] [--delay MS[,MS@SECS...]] [--queue MS]
//!          [--sndbuf KB] [--hop MS] [--hop-link MBPS] [--burst KB@SECS[,...]]
//!          [--passed-lag] [--batch-below KBPS,MS] [--ceiling KBPS] [--held] [--secs S]
//!          [--pause S,EVERY] [--case NAME] [--timeline FILE]
//! ```
//!
//! `--source` is where the sound comes from: `rdp`, an RDP host's wave
//! buffers of 8192 frames at 44.1 kHz, about 186 ms, coded by the gateway; or
//! `wlshare`, 20 ms units wlshare codes itself at the rate the gateway names,
//! `--hop` away (the round trip, 1 ms) over a connection of `--hop-link`
//! Mbit/s (1000) that carries the pictures too, `--burst` writing a picture
//! of so many KB on it at a time. `--batch-below` holds a passed stream's
//! units at the gateway while the walk's rate is KBPS or less, until they
//! come to MS of sound, so a message's framing is not paid on every 20 ms of
//! a stream coded small: 64 and 60 by default, as zdesk batches them, and
//! `0,0` sends what is queued at each send. `--passed-lag` hands a passed stream's
//! dropped units to the walk, as an RDP stream's dropped buffers are.
//!
//! `--link` is the browser's link in kbit/s, what the sound's connection gets
//! of it, 1000 by default, and the rate it changes to from each `@` second on;
//! `--rtt` its round trip, 40 ms, and `--jitter` up to how much more each
//! message and pong takes, at random, keeping their order; `--delay` how
//! much longer than that the path takes from each `@` second on, a queue
//! another stream keeps at the bottleneck, 0 by default; `--queue` the
//! buffer at its bottleneck, in milliseconds at its rate, 100. The gateway's
//! socket to it holds `--sndbuf` KB before a write blocks, 85, what Linux's
//! autotuning gives a connection this slow (measured, `docs/walk.md`), and
//! wakes a blocked writer when it is down to two thirds of that, as Linux
//! does.
//!
//! The walk hears what zdesk would tell it: a ping written ahead of a batch
//! when none is in flight, its pong, the ping overdue whenever the walk asks
//! to be told ([`BitrateWalk::next_tick`]), and every batch sent, its packets
//! and bytes and how long its send blocked; and while the walk is skipping,
//! the pump drops what comes and the writer pings again as each pong is back.
//!
//! `--ceiling` is the rate the walk starts at and never passes, 192 kbit/s as
//! zdesk sets it; `--held` holds it there. `--secs` plays that much of the
//! recording, all of it by default, and `--pause` makes the last S seconds of
//! every EVERY digital silence, a player paused. `--case` names the run in
//! its line. `--timeline` writes every batch as CSV.
//!
//! Time is the simulation's own, deterministic: coding takes none, a byte
//! crosses the bottleneck at its rate after every byte before it, and the
//! congestion window is what the bottleneck and its buffer hold, so the
//! buffer fills before the socket does. The gateway is zdesk's: a broadcast
//! queue sixteen deep that drops its oldest, the encoder or the passed units
//! behind it, sound skipped while the walk asks for it and silence shed
//! while it says the link is behind, a channel
//! two deep to the socket's writer, one batch a WebSocket message. The
//! browser plays each packet back to back from the moment it has it,
//! trimming any lead past 300 ms. What a run's line says is the rate the
//! sound was coded at, how late it was heard, what was not heard and why,
//! the stalls in sound that should have run on, how long the link was held
//! behind, and how the rate moved.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sound_opus::walk::BitrateWalk;
use sound_opus::{Encoder, Stream};

type Error = Box<dyn std::error::Error>;

const STEREO: Stream = Stream { rate: 48_000, channels: 2 };
/// A packet's length.
const PACKET: Duration = Duration::from_millis(20);
/// An RDP wave buffer, in frames at its own 44.1 kHz.
const WAVE_FRAMES: u64 = 8192;
/// zdesk's broadcast queue, and the channel to the socket's writer.
const QUEUE_DEPTH: usize = 16;
const CHANNEL_DEPTH: usize = 2;
/// The browser's most lead.
const MAX_LEAD: Duration = Duration::from_millis(300);
/// A TCP segment's payload.
const MSS: u64 = 1448;
/// What a WebSocket message costs past its payload: its header, and the TLS
/// record around it.
const MESSAGE_OVERHEAD: u64 = 4 + 22;
/// A ping with an eight-byte payload, in its record.
const PING_BYTES: u64 = 2 + 8 + 22;

/// A link of a rate that changes at given times, in bytes a second, every
/// byte after the ones before it.
struct Link {
    rates: Vec<(Duration, f64)>,
    /// When the link is next idle.
    free: Duration,
}

impl Link {
    fn rate_at(&self, t: Duration) -> (f64, Option<Duration>) {
        let i = self.rates.iter().rposition(|(from, _)| *from <= t).unwrap_or(0);
        (self.rates[i].1, self.rates.get(i + 1).map(|(from, _)| *from))
    }

    /// When `bytes` handed over at `at` have all crossed.
    fn pass(&mut self, at: Duration, bytes: u64) -> Duration {
        let mut t = at.max(self.free);
        let mut left = bytes as f64;
        loop {
            let (rate, until) = self.rate_at(t);
            let need = Duration::from_secs_f64(left / rate);
            match until {
                Some(until) if t + need > until => {
                    left -= rate * (until - t).as_secs_f64();
                    t = until;
                }
                _ => {
                    t += need;
                    break;
                }
            }
        }
        self.free = t;
        t
    }
}

/// A segment the socket took: the stream's byte it ends at, and when it was
/// acknowledged.
struct Segment {
    end: u64,
    acked: Duration,
}

/// The gateway's TCP connection to the browser, the bottleneck a hop away.
struct Socket {
    link: Link,
    rtt: Duration,
    /// The bottleneck's buffer, in time at its rate.
    queue: Duration,
    /// What the socket holds, unsent and unacknowledged, before a write
    /// blocks.
    sndbuf: u64,
    segments: Vec<Segment>,
    /// Every byte the socket has taken.
    taken: u64,
    /// How much longer than its round trip the path takes from when on.
    delays: Vec<(Duration, Duration)>,
}

impl Socket {
    /// How much longer than its round trip the path takes at `t`.
    fn delay_at(&self, t: Duration) -> Duration {
        self.delays.iter().rev().find(|(from, _)| *from <= t).map_or(Duration::ZERO, |(_, delay)| *delay)
    }

    /// Bytes acknowledged by `t`.
    fn acked_by(&self, t: Duration) -> u64 {
        match self.segments.partition_point(|s| s.acked <= t) {
            0 => 0,
            n => self.segments[n - 1].end,
        }
    }

    /// When the stream up to `byte` has been acknowledged.
    fn acked_at(&self, byte: u64) -> Duration {
        if byte == 0 {
            return Duration::ZERO;
        }
        self.segments[self.segments.partition_point(|s| s.end < byte)].acked
    }

    /// What the path holds in flight: the bottleneck's round trip and its
    /// buffer, which a congestion window fills.
    fn window(&self, t: Duration) -> u64 {
        let rate = self.link.rate_at(t).0;
        ((rate * (self.rtt + self.queue).as_secs_f64()) as u64).max(4 * MSS)
    }

    /// `bytes` written from `at` by a writer that waits for room: when the
    /// socket has taken the last of them, and when that crossed the
    /// bottleneck.
    fn write(&mut self, at: Duration, bytes: u64) -> (Duration, Duration) {
        let mut now = at;
        let mut left = bytes;
        let mut exit = at;
        while left > 0 {
            let segment = left.min(MSS);
            if self.taken - self.acked_by(now) + segment > self.sndbuf {
                // No room: the writer sleeps until the socket is down to two
                // thirds, as Linux wakes it.
                now = now.max(self.acked_at(self.taken.saturating_sub(self.sndbuf * 2 / 3)));
                continue;
            }
            let end = self.taken + segment;
            let sent = now.max(self.acked_at(end.saturating_sub(self.window(now))));
            exit = self.link.pass(sent, segment);
            // Acknowledgements are cumulative: a segment is never acked before
            // the one ahead of it, even where the path's delay fell between.
            let acked = (exit + self.rtt + self.delay_at(exit)).max(self.segments.last().map_or(Duration::ZERO, |s| s.acked));
            self.segments.push(Segment { end, acked });
            self.taken = end;
            left -= segment;
        }
        (now, exit)
    }
}

/// One packet on its way: its bytes, the rate it was coded at, when its
/// sound began at the source, and whether that sound follows the last
/// packet's with nothing dropped or shed between.
struct Packet {
    bytes: usize,
    rate: u32,
    at: Duration,
    joined: bool,
}

/// What reaches the gateway's broadcast queue: an RDP host's wave buffer, its
/// samples at 48 kHz, or a unit wlshare coded.
enum Input {
    Wave { from: usize, to: usize },
    Unit(Packet),
}

impl Input {
    fn length(&self) -> Duration {
        match self {
            Input::Wave { from, to } => Duration::from_secs_f64((to - from) as f64 / 48_000.0),
            Input::Unit(_) => PACKET,
        }
    }
}

/// The walk as zdesk runs it, with what it publishes: the rate it asks for,
/// and for wlshare when the ask reaches it; whether the link is behind, which
/// the shedding reads; and whether the broadcast queue lagged since the walk
/// was last told anything.
struct Walking {
    walk: BitrateWalk,
    base: Instant,
    hop: Duration,
    desired: u32,
    asks: VecDeque<(Duration, u32)>,
    behind: bool,
    lagged: bool,
    moves: u32,
    downs: u32,
    first_down: Option<Duration>,
    /// Every rate the walk moved to, and when.
    path: Vec<(Duration, u32)>,
}

impl Walking {
    /// The walk, told first of a queue that lagged, as zdesk tells it before
    /// anything else.
    fn walk(&mut self) -> &mut BitrateWalk {
        if std::mem::take(&mut self.lagged) {
            self.walk.dropped();
        }
        &mut self.walk
    }

    /// What the walk said at `now`: the rate it moved to, if it moved.
    fn heard(&mut self, moved: Option<u32>, now: Duration) {
        if let Some(rate) = moved {
            self.moves += 1;
            if rate < self.desired {
                self.downs += 1;
                self.first_down.get_or_insert(now);
            }
            self.desired = rate;
            self.path.push((now, rate));
            self.asks.push_back((now + self.hop / 2, rate));
        }
        self.behind = self.walk.behind();
    }

    /// zdesk's `note_lag`: behind from now, and the walk told when it is
    /// next told anything.
    fn lag(&mut self) {
        self.behind = true;
        self.lagged = true;
    }
}

/// zdesk's broadcast queue: sixteen deep, dropping its oldest. That a
/// listener lost some is told the walk on an RDP stream, and on a passed one
/// only with `tell`.
struct Broadcast {
    queue: VecDeque<Input>,
    tell: bool,
    dropped: Duration,
    gaps: u32,
    /// Whether the next packet out follows the last with nothing lost.
    joined: bool,
}

impl Broadcast {
    fn push(&mut self, input: Input, walking: &mut Walking) {
        if self.queue.len() == QUEUE_DEPTH {
            let lost = self.queue.pop_front().unwrap();
            if self.joined {
                self.gaps += 1;
            }
            self.dropped += lost.length();
            self.joined = false;
            if self.tell {
                walking.lag();
            }
        }
        self.queue.push_back(input);
    }
}

/// The browser: plays each packet back to back from the moment it has it,
/// and trims what would put it more than [`MAX_LEAD`] ahead.
#[derive(Default)]
struct Browser {
    next_at: Option<Duration>,
    /// When the last message arrived, which the next cannot overtake.
    arrived: Duration,
    /// How late each packet's sound was heard, in milliseconds.
    late: Vec<f64>,
    stalls: u32,
    stalled: Duration,
    trimmed: Duration,
}

impl Browser {
    fn play(&mut self, arrived: Duration, packet: &Packet) {
        let start = self.next_at.map_or(arrived, |next| next.max(arrived));
        if let Some(next) = self.next_at
            && next < arrived
            && packet.joined
        {
            self.stalls += 1;
            self.stalled += arrived - next;
        }
        let ceiling = arrived + MAX_LEAD;
        let (start, trim) = if start > ceiling { (ceiling, (start - ceiling).min(PACKET)) } else { (start, Duration::ZERO) };
        self.trimmed += trim;
        self.next_at = Some(start + PACKET - trim);
        if trim < PACKET {
            self.late.push(start.saturating_sub(packet.at + trim).as_secs_f64() * 1e3);
        }
    }
}

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// A flag's number, which is zero or more.
fn number(flag: &str, value: &str) -> Result<f64, Error> {
    let n: f64 = value.parse()?;
    if !(n.is_finite() && n >= 0.0) {
        return Err(format!("{flag} {value}: a number of zero or more").into());
    }
    Ok(n)
}

fn ms(value: f64) -> Duration {
    Duration::from_secs_f64(value / 1e3)
}

/// A recording as the stream the users send, as the bench reads it: 48 kHz
/// stereo, a mono one on both channels.
fn recording(path: &str) -> Result<Vec<i16>, Error> {
    let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels", "-of", "csv=p=0"]).arg(path).output()?;
    let mix = match String::from_utf8_lossy(&probe.stdout).trim() {
        "1" => "pan=stereo|c0=c0|c1=c0",
        "2" => "anull",
        n => return Err(format!("{path}: {n} channels").into()),
    };
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i", path, "-af", mix, "-ar", "48000", "-ac", "2", "-f", "s16le", "pipe:1"])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(format!("ffmpeg failed on {path}").into());
    }
    Ok(output.stdout.as_chunks::<2>().0.iter().map(|&pair| i16::from_le_bytes(pair)).collect())
}

/// zdesk's test of a wave buffer for silence: every sample within one dither
/// step of zero.
fn silent(pcm: &[i16]) -> bool {
    pcm.iter().all(|s| s.unsigned_abs() <= 2)
}

/// The rate and when it changes, as a flag gives it in kbit/s.
fn rates(value: &str) -> Result<Vec<(Duration, f64)>, Error> {
    let rates = value
        .split(',')
        .map(|step| {
            let (rate, from) = step.split_once('@').unwrap_or((step, "0"));
            let (rate, from) = (rate.parse::<f64>()?, from.parse::<f64>()?);
            if !(rate.is_finite() && rate > 0.0 && from.is_finite() && from >= 0.0) {
                return Err(format!("--link step {step}: a rate above zero and a time of zero or more").into());
            }
            Ok((Duration::from_secs_f64(from), rate * 1e3 / 8.0))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if rates.windows(2).any(|pair| pair[1].0 < pair[0].0) {
        return Err("--link steps go in order of their times".into());
    }
    Ok(rates)
}

/// A string as a JSON string, quoted and escaped.
fn json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() -> Result<(), Error> {
    let mut path = None;
    let mut link_arg = "1000".to_string();
    let (mut rtt, mut jitter, mut queue, mut sndbuf, mut hop, mut hop_link) = (40.0, 0.0, 100.0, 85.0, 1.0, 1000.0);
    let (mut rdp, mut passed_lag, mut held) = (true, false, false);
    let (mut ceiling, mut secs, mut pause) = (192.0, None, None);
    let mut batch_below = (64_000u32, Duration::from_millis(60));
    let mut delay_arg = "0".to_string();
    let mut bursts: Vec<(Duration, u64)> = Vec::new();
    let (mut case, mut timeline) = (String::new(), None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} takes a value"));
        match arg.as_str() {
            "--source" => {
                rdp = match value()?.as_str() {
                    "rdp" => true,
                    "wlshare" => false,
                    other => return Err(format!("--source {other}: rdp or wlshare").into()),
                }
            }
            "--link" => link_arg = value()?,
            "--rtt" => rtt = number(&arg, &value()?)?,
            "--jitter" => jitter = number(&arg, &value()?)?,
            "--queue" => queue = number(&arg, &value()?)?,
            "--sndbuf" => sndbuf = number(&arg, &value()?)?,
            "--delay" => delay_arg = value()?,
            "--hop" => hop = number(&arg, &value()?)?,
            "--hop-link" => hop_link = number(&arg, &value()?)?,
            "--burst" => {
                for step in value()?.split(',') {
                    let (kb, at) = step.split_once('@').ok_or("--burst KB@SECS")?;
                    bursts.push((Duration::from_secs_f64(number(&arg, at)?), (number(&arg, kb)? * 1e3) as u64));
                }
                bursts.sort();
            }
            "--passed-lag" => passed_lag = true,
            "--ceiling" => ceiling = number(&arg, &value()?)?,
            "--batch-below" => {
                let v = value()?;
                let (kbps, hold) = v.split_once(',').ok_or("--batch-below KBPS,MS")?;
                batch_below = ((number(&arg, kbps)? * 1e3) as u32, ms(number(&arg, hold)?));
            }
            "--held" => held = true,
            "--secs" => secs = Some(number(&arg, &value()?)?),
            "--pause" => {
                let v = value()?;
                let (s, every) = v.split_once(',').ok_or("--pause S,EVERY")?;
                pause = Some((number(&arg, s)?, number(&arg, every)?));
            }
            "--case" => case = value()?,
            "--timeline" => timeline = Some(std::io::BufWriter::new(File::create(value()?)?)),
            _ if arg.starts_with("--") => return Err(format!("unknown flag {arg}").into()),
            _ if path.is_none() => path = Some(arg),
            _ => return Err("walk-sim plays one recording".into()),
        }
    }
    let path = path.ok_or("walk-sim takes a recording to play")?;
    let mut pcm = recording(&path)?;
    if let Some(secs) = secs {
        pcm.truncate((secs * 48_000.0) as usize * 2);
    }
    pcm.truncate(pcm.len() / STEREO.samples() * STEREO.samples());
    if let Some((s, every)) = pause {
        let (s, every) = ((s * 48_000.0) as usize * 2, (every * 48_000.0) as usize * 2);
        if every > 0 {
            for start in (every.saturating_sub(s)..pcm.len()).step_by(every) {
                let end = (start + s).min(pcm.len());
                pcm[start..end].fill(0);
            }
        }
    }
    let frames = pcm.len() / 2;

    let name = path.rsplit('/').next().unwrap().trim_end_matches(".wav").to_string();
    let (rtt, hop) = (ms(rtt), ms(hop));
    let delays = delay_arg
        .split(',')
        .map(|step| {
            let (delay, from) = step.split_once('@').unwrap_or((step, "0"));
            Ok((Duration::from_secs_f64(number("--delay", from)?), ms(number("--delay", delay)?)))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if delays.windows(2).any(|pair| pair[1].0 < pair[0].0) {
        return Err("--delay steps go in order of their times".into());
    }
    let mut socket = Socket { link: Link { rates: rates(&link_arg)?, free: Duration::ZERO }, rtt, queue: ms(queue), sndbuf: (sndbuf * 1e3) as u64, segments: Vec::new(), taken: 0, delays };
    let mut hop_wire = Link { rates: vec![(Duration::ZERO, hop_link * 1e6 / 8.0)], free: Duration::ZERO };
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    // Up to `--jitter` more on the way to the browser, at random.
    let mut wobble = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        Duration::from_secs_f64((seed >> 11) as f64 / (1u64 << 53) as f64 * jitter / 1e3)
    };

    let ceiling = (ceiling * 1e3) as u32;
    let mut walking = Walking {
        walk: BitrateWalk::new(ceiling, !held),
        base: Instant::now(),
        hop,
        desired: ceiling,
        asks: VecDeque::new(),
        behind: false,
        lagged: false,
        moves: 0,
        downs: 0,
        first_down: None,
        path: Vec::new(),
    };
    let base = walking.base;
    // The one encoder: the gateway's for RDP, wlshare's for its units.
    let mut encoder = Encoder::new(STEREO, ceiling)?;
    let mut coded_at = ceiling;
    let mut broadcast = Broadcast { queue: VecDeque::new(), tell: rdp || passed_lag, dropped: Duration::ZERO, gaps: 0, joined: true };
    // Units on their way over the hop to the gateway.
    let mut on_hop: VecDeque<(Duration, Packet)> = VecDeque::new();
    let mut channel: VecDeque<Vec<Packet>> = VecDeque::new();
    // The pump, blocked on the channel since when, with its batch.
    let mut pump: Option<(Duration, Vec<Packet>)> = None;
    // Passed units the pump holds for a batch, since when.
    let mut holding: Option<(Duration, Vec<Packet>)> = None;
    // The writer, busy until when.
    let mut writing: Option<Duration> = None;
    // When the pong of the ping in flight is back.
    let mut pong: Option<Duration> = None;
    let mut browser = Browser::default();
    let mut out: Vec<u8> = Vec::new();
    let (mut shed, mut skipped) = (Duration::ZERO, Duration::ZERO);
    let mut pings = 0u32;
    let mut rates_coded: Vec<f64> = Vec::new();
    let (mut behind_for, mut behind_since) = (Duration::ZERO, None::<Duration>);
    if let Some(t) = &mut timeline {
        writeln!(t, "t_ms,packets,bytes,kbit,behind,taken_ms,arrived_ms")?;
    }

    // When the source next yields: an RDP buffer when its last frame is in,
    // a wlshare unit when its block is.
    let wave_end = |k: u64| ((k + 1) * WAVE_FRAMES * 48_000 / 44_100) as usize;
    let source_at = |k: u64| -> Option<Duration> {
        let end = if rdp { wave_end(k) } else { (k as usize + 1) * 960 };
        (end <= frames).then(|| Duration::from_secs_f64(end as f64 / 48_000.0))
    };
    let mut produced = 0u64;

    let mut now = Duration::ZERO;
    loop {
        // The walk hears a pong, or a ping overdue.
        if let Some(back) = pong.filter(|back| *back <= now) {
            pong = None;
            let moved = walking.walk().pong(base + back);
            walking.heard(moved, back);
            // Skipping, no batch comes to write a ping ahead of: the writer
            // writes one at once, after anything it is writing.
            let at = writing.map_or(now, |until| until.max(now));
            if walking.walk.skipping() && walking.walk().ping(base + at) {
                pings += 1;
                let (taken, exit) = socket.write(at, PING_BYTES);
                browser.arrived = browser.arrived.max(exit + rtt / 2 + socket.delay_at(exit) + wobble());
                pong = Some(browser.arrived + rtt / 2);
                writing = Some(taken);
            }
        }
        if walking.walk.next_tick().is_some_and(|due| due <= base + now) {
            let moved = walking.walk().tick(base + now);
            walking.heard(moved, now);
        }
        // The source.
        while source_at(produced).is_some_and(|at| at <= now) {
            if rdp {
                let from = if produced == 0 { 0 } else { wave_end(produced - 1) };
                broadcast.push(Input::Wave { from, to: wave_end(produced) }, &mut walking);
            } else {
                let at = PACKET * (produced as u32 + 1);
                while walking.asks.front().is_some_and(|(t, _)| *t <= at) {
                    let (_, rate) = walking.asks.pop_front().unwrap();
                    if rate != coded_at {
                        encoder.set_bitrate(rate)?;
                        coded_at = rate;
                    }
                }
                let k = produced as usize;
                out.clear();
                encoder.encode(&pcm[k * 1920..(k + 1) * 1920], &mut out)?;
                rates_coded.push(f64::from(coded_at) / 1e3);
                // Pictures written ahead of this unit go over the hop first.
                while bursts.first().is_some_and(|(t, _)| *t <= at) {
                    let (t, bytes) = bursts.remove(0);
                    hop_wire.pass(t, bytes);
                }
                let arrived = hop_wire.pass(at, out.len() as u64 + 8) + hop / 2;
                on_hop.push_back((arrived, Packet { bytes: out.len(), rate: coded_at, at: at - PACKET, joined: true }));
            }
            produced += 1;
        }
        while on_hop.front().is_some_and(|(t, _)| *t <= now) {
            let (_, unit) = on_hop.pop_front().unwrap();
            broadcast.push(Input::Unit(unit), &mut walking);
        }
        loop {
            let mut progress = false;
            if writing.is_some_and(|until| until <= now) {
                writing = None;
            }
            // The writer takes the next batch, and the pump's goes in its place.
            if writing.is_none()
                && let Some(packets) = channel.pop_front()
            {
                progress = true;
                let mut at = now;
                if walking.walk().ping(base + at) {
                    pings += 1;
                    let (taken, exit) = socket.write(at, PING_BYTES);
                    browser.arrived = browser.arrived.max(exit + rtt / 2 + socket.delay_at(exit) + wobble());
                    pong = Some(browser.arrived + rtt / 2);
                    at = taken;
                }
                let bytes = 4 + packets.iter().map(|p| 2 + p.bytes as u64).sum::<u64>() + MESSAGE_OVERHEAD;
                let (taken, exit) = socket.write(at, bytes);
                writing = Some(taken);
                browser.arrived = browser.arrived.max(exit + rtt / 2 + socket.delay_at(exit) + wobble());
                for packet in &packets {
                    browser.play(browser.arrived, packet);
                }
                if let Some(t) = &mut timeline {
                    writeln!(
                        t,
                        "{:.1},{},{bytes},{},{},{:.1},{:.1}",
                        now.as_secs_f64() * 1e3,
                        packets.len(),
                        packets[0].rate / 1000,
                        u8::from(walking.behind),
                        taken.as_secs_f64() * 1e3,
                        browser.arrived.as_secs_f64() * 1e3,
                    )?;
                }
                if let Some((since, batch)) = pump.take() {
                    walking.walk().sent(now - since, batch.len() as u32, batch.iter().map(|p| p.bytes).sum());
                    channel.push_back(batch);
                }
            }
            // The pump reads the next buffer or units, and sends the packets.
            let mut batch = Vec::new();
            if pump.is_none()
                && let Some(input) = broadcast.queue.pop_front()
            {
                progress = true;
                if walking.walk.skipping() {
                    skipped += input.length();
                    broadcast.joined = false;
                } else {
                    match input {
                        Input::Wave { from, to } => {
                            if walking.desired != coded_at {
                                encoder.set_bitrate(walking.desired)?;
                                coded_at = walking.desired;
                            }
                            if walking.behind && silent(&pcm[from * 2..to * 2]) {
                                shed += Duration::from_secs_f64((to - from) as f64 / 48_000.0);
                                broadcast.joined = false;
                            } else {
                                // The packets this buffer completes.
                                for k in from / 960..to / 960 {
                                    out.clear();
                                    encoder.encode(&pcm[k * 1920..(k + 1) * 1920], &mut out)?;
                                    rates_coded.push(f64::from(coded_at) / 1e3);
                                    batch.push(Packet { bytes: out.len(), rate: coded_at, at: PACKET * k as u32, joined: broadcast.joined });
                                    broadcast.joined = true;
                                }
                            }
                        }
                        Input::Unit(mut unit) => {
                            unit.joined &= broadcast.joined;
                            broadcast.joined = true;
                            let (_, held) = holding.get_or_insert_with(|| (now, Vec::new()));
                            held.push(unit);
                            while let Some(Input::Unit(_)) = broadcast.queue.front() {
                                let Some(Input::Unit(unit)) = broadcast.queue.pop_front() else { unreachable!() };
                                held.push(unit);
                            }
                        }
                    }
                }
            }
            // Units held go once the rate is over the batching one, or they
            // come to enough sound, or the first has waited that long.
            if pump.is_none()
                && holding.as_ref().is_some_and(|(since, held)| {
                    walking.desired > batch_below.0 || PACKET * held.len() as u32 >= batch_below.1 || now >= *since + batch_below.1
                })
            {
                progress = true;
                batch = holding.take().unwrap().1;
            }
            if !batch.is_empty() {
                if channel.len() < CHANNEL_DEPTH {
                    walking.walk().sent(Duration::ZERO, batch.len() as u32, batch.iter().map(|p| p.bytes).sum());
                    channel.push_back(batch);
                } else {
                    pump = Some((now, batch));
                }
            }
            if !progress {
                break;
            }
        }
        match (behind_since, walking.behind) {
            (None, true) => behind_since = Some(now),
            (Some(since), false) => {
                behind_for += now - since;
                behind_since = None;
            }
            _ => {}
        }
        // The next thing to happen.
        let due = walking.walk.next_tick().map(|due| due.saturating_duration_since(base));
        let flush = holding.as_ref().map(|(since, _)| *since + batch_below.1);
        let next = [source_at(produced), on_hop.front().map(|(t, _)| *t), writing, pong, due, flush].into_iter().flatten().filter(|t| *t > now).min();
        match next {
            Some(t) => now = t,
            None if broadcast.queue.is_empty() && channel.is_empty() && pump.is_none() && holding.is_none() => break,
            None => return Err("the simulation stalled with sound still queued".into()),
        }
    }
    if let Some(since) = behind_since {
        behind_for += now - since;
    }
    let path = walking.path.iter().map(|(t, rate)| format!("{}@{:.1}", rate / 1000, t.as_secs_f64())).collect::<Vec<_>>().join(" ");
    println!(
        "{{\"case\":{},\"recording\":{},\"source\":\"{}\",\"link\":\"{link_arg}\",\"rtt\":{},\"jitter\":{},\"queue\":{},\"delay\":\"{delay_arg}\",\"sndbuf\":{sndbuf},\"passed_lag\":{passed_lag},\"batch_below\":[{},{}],\"held\":{held},\"ceiling\":{},\"pause\":{},\"secs\":{:.1},\
\"rate_mean\":{:.1},\"rate_p10\":{:.0},\"late_p50\":{:.0},\"late_p95\":{:.0},\"late_max\":{:.0},\"heard\":{:.2},\"dropped\":{:.2},\"gaps\":{},\"skipped\":{:.2},\"trimmed\":{:.2},\"shed\":{:.2},\"stalls\":{},\"stalled\":{:.2},\"behind\":{:.2},\"pings\":{pings},\"moves\":{},\"downs\":{},\"first_down\":{},\"path\":{}}}",
        json(&case),
        json(&name),
        if rdp { "rdp" } else { "wlshare" },
        rtt.as_millis(),
        jitter,
        queue,
        batch_below.0 / 1000,
        batch_below.1.as_millis(),
        ceiling / 1000,
        pause.map_or("null".into(), |(s, every)| format!("[{s},{every}]")),
        frames as f64 / 48_000.0,
        rates_coded.iter().sum::<f64>() / rates_coded.len().max(1) as f64,
        pct(&rates_coded, 0.1),
        pct(&browser.late, 0.5),
        pct(&browser.late, 0.95),
        pct(&browser.late, 1.0),
        browser.late.len() as f64 * PACKET.as_secs_f64(),
        broadcast.dropped.as_secs_f64(),
        broadcast.gaps,
        skipped.as_secs_f64(),
        browser.trimmed.as_secs_f64(),
        shed.as_secs_f64(),
        browser.stalls,
        browser.stalled.as_secs_f64(),
        behind_for.as_secs_f64(),
        walking.moves,
        walking.downs,
        walking.first_down.map_or("null".into(), |t| format!("{:.2}", t.as_secs_f64())),
        json(&path),
    );
    Ok(())
}

