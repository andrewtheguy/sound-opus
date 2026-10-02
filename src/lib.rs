//! Opus for a desktop's sound, as wlshare and the remotex gateway both code it.
//!
//! wlshare codes what its desktop plays as Opus and the gateway hands each
//! packet to the browser as it came; the sound of a desktop that sends the
//! gateway samples instead is coded there. Both encoders are this crate's, so
//! the two streams are the same by construction:
//!
//! - **[`Stream`]** is what a stream is before any sound: the rate and the
//!   channels, and with them the block, the frames of samples in every packet.
//! - **[`Encoder`]** makes one Opus packet of every block, at a rate its owner
//!   may move while it runs.
//! - **[`Stream::head`]** is the `OpusHead` a decoder is configured from,
//!   which nothing sends: everything in it follows from the stream.
//! - **[`walk::BitrateWalk`]** is the rate a link will bear, walked under the
//!   configured one: down while sending blocks, back up while it keeps up, to
//!   a floor fixed here, and whether the link is behind, for a sender with
//!   silence to shed. Its one setting is whether it walks at all; the rate it
//!   arrives at is moved on an encoder here or named to a remote that codes
//!   its own.
//!
//! ## What a stream is
//!
//! Every packet is twenty milliseconds, Opus's default frame: shorter ones
//! spend more of the rate on packet overhead and longer ones add delay. The
//! encoder is libopus's for music and effects as much as speech
//! (`OPUS_APPLICATION_AUDIO`), at full effort, and its rate is an average:
//! constrained VBR lets silence cost a few bytes and a loud passage a little
//! more than the number, and keeps the running rate close enough to it that
//! the number is what a link sees. No forward error correction and no
//! discontinuous transmission: the streams this is for ride TCP, which loses
//! nothing.
//!
//! A packet decodes from the ones before it, as any Opus stream does, and
//! carries its own coding parameters, so a decoder is told nothing when the
//! rate moves and conceals a packet that never came.
//!
//! libopus is linked statically, from a prebuilt archive, so nothing of Opus is
//! compiled to build this crate and nothing is installed to run it.
//!
//! Nothing about a wire is here: how a packet is framed in a message, how the
//! stream's shape is agreed, what a sample's bytes are and where a send's
//! blocking is measured belong to the user.

use opus::{Application, Bitrate, Channels};
use thiserror::Error;

pub mod walk;

/// The rates Opus codes at, in Hz: the only ones a [`Stream`] may have.
pub const RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

/// The packets in a second: every packet is twenty milliseconds.
pub const PACKETS_PER_SECOND: u32 = 50;

/// The rate, in bits per second, of a stream whose owner names none.
pub const BITRATE_DEFAULT: u32 = 96_000;
/// The lowest rate an encoder takes, in bits per second: libopus's.
pub const BITRATE_MIN: u32 = 6_000;
/// The highest rate an encoder takes, in bits per second: libopus's.
pub const BITRATE_MAX: u32 = 510_000;

/// The most bytes one packet is, which libopus documents as the largest worth
/// allowing for; at the default rate a packet is nearer 240.
pub const MAX_PACKET: usize = 4000;

/// The samples, at 48 kHz, a decoder is to discard from the start of a stream:
/// the encoder's lookahead, 6.5 ms at every rate. What an `OpusHead` states as
/// its pre-skip where nothing but the encoder delays the sound.
pub const PRE_SKIP: u16 = 312;

/// The bytes of an `OpusHead` for one or two channels.
pub const HEAD_LEN: usize = 19;

/// libopus's encoder effort, 0–10. The top: one stereo stream costs a few
/// percent of a core either way, and the quality the effort buys matters most
/// at a low rate, where every bit has to count.
const COMPLEXITY: i32 = 10;

/// What a stream is before any sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream {
    /// Samples per second per channel, one of [`RATES`].
    pub rate: u32,
    /// 1 or 2.
    pub channels: u8,
}

impl Stream {
    /// Whether this is a stream Opus carries here.
    pub fn check(&self) -> Result<(), Error> {
        if RATES.contains(&self.rate) && (1..=2).contains(&self.channels) { Ok(()) } else { Err(Error::Unsupported(*self)) }
    }

    /// The frames of samples in every packet: twenty milliseconds, 960 at
    /// 48 kHz.
    pub fn block(&self) -> usize {
        (self.rate / PACKETS_PER_SECOND) as usize
    }

    /// The samples in a block: a frame of them for each channel.
    pub fn samples(&self) -> usize {
        self.block() * usize::from(self.channels)
    }

    /// The `OpusHead` a decoder of this stream is configured from, as RFC 7845
    /// lays out the identification header: `pre_skip` samples at 48 kHz to
    /// discard from the start — [`PRE_SKIP`], and whatever else delayed the
    /// sound on its way to the encoder — and `input_rate`, the rate the sound
    /// had before it was coded, which is metadata: Opus decodes at 48 kHz.
    pub fn head(&self, pre_skip: u16, input_rate: u32) -> Result<[u8; HEAD_LEN], Error> {
        self.check()?;
        let mut head = [0u8; HEAD_LEN];
        head[0..8].copy_from_slice(b"OpusHead");
        head[8] = 1; // version
        head[9] = self.channels;
        head[10..12].copy_from_slice(&pre_skip.to_le_bytes());
        head[12..16].copy_from_slice(&input_rate.to_le_bytes());
        // Output gain unchanged, and channel mapping family 0: one or two
        // channels, with no mapping table.
        Ok(head)
    }
}

/// Why a packet could not be made.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0:?} is not a stream Opus carries: one or two channels at 8, 12, 16, 24 or 48 kHz")]
    Unsupported(Stream),
    #[error("{0} bit/s is outside the {BITRATE_MIN} to {BITRATE_MAX} Opus is coded at")]
    Bitrate(u32),
    #[error("a block of {got} samples, where the stream's is {want}")]
    Block { got: usize, want: usize },
    /// libopus would not start the stream or take a setting, carried as its
    /// word for why.
    #[error("libopus refused the stream: {0}")]
    Refused(String),
    /// libopus failed on a block, carried as its word for why.
    #[error("encoding an Opus packet: {0}")]
    Encode(String),
}

/// Whether `bitrate`, in bits per second, is one an encoder takes.
pub fn check_bitrate(bitrate: u32) -> Result<(), Error> {
    if (BITRATE_MIN..=BITRATE_MAX).contains(&bitrate) { Ok(()) } else { Err(Error::Bitrate(bitrate)) }
}

fn refused(e: opus::Error) -> Error {
    Error::Refused(e.to_string())
}

/// One stream's encoder: a block in, an Opus packet out.
pub struct Encoder {
    stream: Stream,
    encoder: opus::Encoder,
    /// Where libopus writes a packet, before it is appended to the caller's.
    packet: Box<[u8; MAX_PACKET]>,
}

impl Encoder {
    /// An encoder for `stream` at `bitrate` bits per second, if both are ones
    /// Opus carries here.
    pub fn new(stream: Stream, bitrate: u32) -> Result<Self, Error> {
        stream.check()?;
        check_bitrate(bitrate)?;
        let channels = if stream.channels == 1 { Channels::Mono } else { Channels::Stereo };
        let mut encoder = opus::Encoder::new(stream.rate, channels, Application::Audio).map_err(refused)?;
        // Written out rather than left to libopus's defaults, which happen to
        // be the same: that the rate is an average, held near, is what a user's
        // own arithmetic and wording about it depend on.
        encoder.set_vbr(true).map_err(refused)?;
        encoder.set_vbr_constraint(true).map_err(refused)?;
        encoder.set_bitrate(Bitrate::Bits(bitrate as i32)).map_err(refused)?;
        encoder.set_complexity(COMPLEXITY).map_err(refused)?;
        Ok(Self { stream, encoder, packet: Box::new([0; MAX_PACKET]) })
    }

    /// The stream this encoder was made for.
    pub fn stream(&self) -> Stream {
        self.stream
    }

    /// Move the rate to `bitrate` bits per second, from the next packet on.
    /// Nothing else changes and a decoder needs no telling.
    pub fn set_bitrate(&mut self, bitrate: u32) -> Result<(), Error> {
        check_bitrate(bitrate)?;
        self.encoder.set_bitrate(Bitrate::Bits(bitrate as i32)).map_err(refused)
    }

    /// The samples, at 48 kHz, this encoder holds the sound back by:
    /// [`PRE_SKIP`], read from libopus.
    pub fn pre_skip(&mut self) -> Result<u16, Error> {
        let lookahead = self.encoder.get_lookahead().map_err(refused)?.max(0) as u32;
        // libopus counts in the stream's own rate, `OpusHead` at 48 kHz.
        Ok((lookahead * (48_000 / self.stream.rate)) as u16)
    }

    fn sized(&self, got: usize) -> Result<(), Error> {
        let want = self.stream.samples();
        if got == want { Ok(()) } else { Err(Error::Block { got, want }) }
    }

    /// Append to `packet` the Opus packet of one block: [`Stream::samples`]
    /// signed 16-bit samples, interleaved.
    pub fn encode(&mut self, block: &[i16], packet: &mut Vec<u8>) -> Result<(), Error> {
        self.sized(block.len())?;
        let length = self.encoder.encode(block, &mut self.packet[..]).map_err(|e| Error::Encode(e.to_string()))?;
        packet.extend_from_slice(&self.packet[..length]);
        Ok(())
    }

    /// The same, of samples as floats between -1 and 1.
    pub fn encode_float(&mut self, block: &[f32], packet: &mut Vec<u8>) -> Result<(), Error> {
        self.sized(block.len())?;
        let length = self.encoder.encode_float(block, &mut self.packet[..]).map_err(|e| Error::Encode(e.to_string()))?;
        packet.extend_from_slice(&self.packet[..length]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEREO: Stream = Stream { rate: 48_000, channels: 2 };

    /// A tone on each channel of `stream`, an octave apart, `blocks` long.
    fn tone(stream: Stream, blocks: usize) -> Vec<i16> {
        let mut pcm = Vec::with_capacity(blocks * stream.samples());
        for n in 0..blocks * stream.block() {
            for channel in 0..stream.channels {
                let t = n as f64 / f64::from(stream.rate);
                let hz = 440.0 * f64::from(channel + 1);
                pcm.push(((t * hz * std::f64::consts::TAU).sin() * 12_000.0) as i16);
            }
        }
        pcm
    }

    fn packets(encoder: &mut Encoder, pcm: &[i16]) -> Vec<Vec<u8>> {
        pcm.chunks(encoder.stream().samples())
            .map(|block| {
                let mut packet = Vec::new();
                encoder.encode(block, &mut packet).unwrap();
                packet
            })
            .collect()
    }

    /// One Ogg page holding `packet` whole, as RFC 3533 lays a page out, its
    /// checksum the format's own CRC-32: unreflected, from zero.
    fn page(kind: u8, granule: u64, sequence: u32, packet: &[u8]) -> Vec<u8> {
        let mut page = Vec::from(*b"OggS");
        page.extend_from_slice(&[0, kind]);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&1u32.to_le_bytes()); // the stream's serial number
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&[0; 4]); // the checksum, once the page is whole
        // A packet is a run of 255-byte segments and one shorter, which ends it.
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

    /// The packets as an Ogg Opus stream, as RFC 7845 lays one out: the head,
    /// with no pre-skip so that every sample decoded is kept, the tags, and a
    /// page to a packet.
    fn ogg(stream: Stream, packets: &[Vec<u8>]) -> Vec<u8> {
        let mut file = page(2, 0, 0, &stream.head(0, stream.rate).unwrap());
        file.extend(page(0, 0, 1, b"OpusTags\x04\0\0\0test\0\0\0\0"));
        for (n, packet) in packets.iter().enumerate() {
            let last = if n + 1 == packets.len() { 4 } else { 0 };
            // The position is in 48 kHz samples whatever the stream's rate.
            file.extend(page(last, (n as u64 + 1) * 960, n as u32 + 2, packet));
        }
        file
    }

    /// Decode with FFmpeg's own Opus decoder, which shares nothing with
    /// libopus, to 48 kHz in the stream's channels. `ffmpeg` must be on the
    /// path.
    fn decode(stream: Stream, packets: &[Vec<u8>]) -> Vec<i16> {
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let channels = stream.channels.to_string();
        let mut ffmpeg = Command::new("ffmpeg")
            // `-c:a opus` before the input names the decoder: FFmpeg's, not
            // its wrapper around libopus.
            .args(["-v", "error", "-c:a", "opus", "-i", "pipe:0", "-f", "s16le", "-ar", "48000", "-ac", &channels, "pipe:1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the Opus tests decode with ffmpeg, which must be installed");
        let mut stdin = ffmpeg.stdin.take().unwrap();
        let file = ogg(stream, packets);
        let writer = std::thread::spawn(move || stdin.write_all(&file));
        let output = ffmpeg.wait_with_output().unwrap();
        writer.join().unwrap().unwrap();
        assert!(output.status.success(), "ffmpeg refused the stream");
        let decoded: Vec<i16> = output.stdout.as_chunks::<2>().0.iter().map(|&pair| i16::from_le_bytes(pair)).collect();
        assert_eq!(decoded.len(), packets.len() * 960 * usize::from(stream.channels), "every packet is twenty milliseconds");
        decoded
    }

    fn energy(samples: impl Iterator<Item = i16>) -> f64 {
        let (sum, count) = samples.fold((0.0, 0usize), |(sum, count), s| (sum + f64::from(s) * f64::from(s), count + 1));
        sum / count as f64
    }

    #[test]
    fn a_stream_is_one_or_two_channels_at_an_opus_rate() {
        for rate in RATES {
            for channels in [1, 2] {
                let stream = Stream { rate, channels };
                assert!(stream.check().is_ok());
                assert_eq!(stream.block() as u32 * 50, rate, "twenty milliseconds");
                assert_eq!(stream.samples(), stream.block() * usize::from(channels));
            }
        }
        assert_eq!(STEREO.block(), 960);
        for stream in [Stream { rate: 44_100, channels: 2 }, Stream { rate: 48_000, channels: 0 }, Stream { rate: 48_000, channels: 3 }] {
            assert!(matches!(stream.check(), Err(Error::Unsupported(s)) if s == stream));
            assert!(matches!(Encoder::new(stream, BITRATE_DEFAULT), Err(Error::Unsupported(_))));
            assert!(stream.head(PRE_SKIP, 48_000).is_err());
        }
    }

    #[test]
    fn a_rate_outside_libopus_is_refused() {
        for bitrate in [0, BITRATE_MIN - 1, BITRATE_MAX + 1, u32::MAX] {
            assert!(matches!(Encoder::new(STEREO, bitrate), Err(Error::Bitrate(b)) if b == bitrate));
            assert!(matches!(Encoder::new(STEREO, BITRATE_DEFAULT).unwrap().set_bitrate(bitrate), Err(Error::Bitrate(_))));
        }
        for bitrate in [BITRATE_MIN, BITRATE_DEFAULT, BITRATE_MAX] {
            assert!(Encoder::new(STEREO, bitrate).is_ok());
        }
    }

    /// The rate control the module's text promises, read back from libopus
    /// rather than assumed from its defaults.
    #[test]
    fn the_encoder_is_constrained_vbr_at_full_complexity() {
        let mut encoder = Encoder::new(STEREO, 96_000).unwrap();
        let opus = &mut encoder.encoder;
        assert!(opus.get_vbr().unwrap(), "variable-rate, not CBR");
        assert!(opus.get_vbr_constraint().unwrap(), "held near the rate");
        assert_eq!(opus.get_complexity().unwrap(), COMPLEXITY);
        assert_eq!(opus.get_bitrate().unwrap(), Bitrate::Bits(96_000));
        assert!(!opus.get_inband_fec().unwrap(), "TCP loses nothing");
        assert!(!opus.get_dtx().unwrap());
    }

    /// The header's fields where RFC 7845 puts them, and the pre-skip libopus
    /// reports at every rate.
    #[test]
    fn the_head_has_the_specified_layout_and_the_encoders_lookahead() {
        let head = STEREO.head(PRE_SKIP, 44_100).unwrap();
        assert_eq!(&head[0..8], b"OpusHead");
        assert_eq!(head[8], 1, "version");
        assert_eq!(head[9], 2, "stereo");
        assert_eq!(u16::from_le_bytes([head[10], head[11]]), 312);
        assert_eq!(u32::from_le_bytes(head[12..16].try_into().unwrap()), 44_100);
        assert_eq!(&head[16..19], &[0, 0, 0], "no gain, mapping family 0");
        assert_eq!(Stream { rate: 16_000, channels: 1 }.head(0, 16_000).unwrap()[9], 1);
        for rate in RATES {
            let mut encoder = Encoder::new(Stream { rate, channels: 2 }, BITRATE_DEFAULT).unwrap();
            assert_eq!(encoder.pre_skip().unwrap(), PRE_SKIP, "{rate} Hz");
        }
    }

    #[test]
    fn a_block_of_the_wrong_size_is_refused() {
        let mut encoder = Encoder::new(STEREO, BITRATE_DEFAULT).unwrap();
        let mut packet = Vec::new();
        assert!(matches!(encoder.encode(&[0; 1919], &mut packet), Err(Error::Block { got: 1919, want: 1920 })));
        assert!(matches!(encoder.encode_float(&[0.0; 960], &mut packet), Err(Error::Block { got: 960, want: 1920 })));
        assert!(packet.is_empty());
        encoder.encode(&[0; 1920], &mut packet).unwrap();
        assert!(!packet.is_empty());
    }

    /// A packet is appended, so a user's message header in front of it stays.
    #[test]
    fn a_packet_is_appended_to_what_is_there() {
        let mut encoder = Encoder::new(STEREO, BITRATE_DEFAULT).unwrap();
        let mut packet = vec![7, 7, 7];
        encoder.encode(&tone(STEREO, 1), &mut packet).unwrap();
        assert_eq!(&packet[..3], &[7, 7, 7]);
        assert!(packet.len() > 3 && packet.len() <= 3 + MAX_PACKET);
    }

    /// Every rate and channel count decodes, in an independent decoder, to the
    /// tone that went in: each channel its own, so a transposed interleave or a
    /// wrong channel count would show.
    #[test]
    fn every_stream_survives_an_independent_decoder() {
        for rate in RATES {
            for channels in [1, 2] {
                let stream = Stream { rate, channels };
                let pcm = tone(stream, 25);
                let mut encoder = Encoder::new(stream, BITRATE_DEFAULT).unwrap();
                let decoded = decode(stream, &packets(&mut encoder, &pcm));
                // Past the encoder settling; the decoder's samples are at
                // 48 kHz, and a tone's energy is the same at any rate.
                let (before, after) = (energy(pcm[5 * stream.samples()..].iter().copied()), energy(decoded[decoded.len() / 5..].iter().copied()));
                assert!((0.5..2.0).contains(&(after / before)), "{stream:?}: {after} against {before}");
            }
        }
    }

    /// A signal on the left alone is still on the left alone after the trip.
    #[test]
    fn a_hard_panned_signal_keeps_its_side() {
        let mut pcm = tone(STEREO, 25);
        pcm.iter_mut().skip(1).step_by(2).for_each(|right| *right = 0);
        let mut encoder = Encoder::new(STEREO, BITRATE_DEFAULT).unwrap();
        let decoded = decode(STEREO, &packets(&mut encoder, &pcm));
        let settled = &decoded[5 * STEREO.samples()..];
        let left = energy(settled.iter().copied().step_by(2));
        let right = energy(settled.iter().copied().skip(1).step_by(2));
        assert!(left > 1_000_000.0, "the left channel carries the tone: {left}");
        assert!(right * 10.0 < left, "the right is far quieter: {right} against {left}");
    }

    /// Floats and 16-bit samples of the same sound are the same stream.
    #[test]
    fn float_samples_code_the_same_sound() {
        let pcm = tone(STEREO, 25);
        let floats: Vec<f32> = pcm.iter().map(|&s| f32::from(s) / 32_768.0).collect();
        let mut encoder = Encoder::new(STEREO, BITRATE_DEFAULT).unwrap();
        let packets: Vec<Vec<u8>> = floats
            .chunks(STEREO.samples())
            .map(|block| {
                let mut packet = Vec::new();
                encoder.encode_float(block, &mut packet).unwrap();
                packet
            })
            .collect();
        let decoded = decode(STEREO, &packets);
        let settled = 5 * STEREO.samples();
        let ratio = energy(decoded[settled..].iter().copied()) / energy(pcm[settled..].iter().copied());
        assert!((0.5..2.0).contains(&ratio), "{ratio}");
    }

    /// Silence is where a variable rate shows: a packet of it is a few bytes,
    /// nothing like the rate's share.
    #[test]
    fn silence_costs_almost_nothing() {
        let mut encoder = Encoder::new(STEREO, 96_000).unwrap();
        let packets = packets(&mut encoder, &vec![0; 20 * STEREO.samples()]);
        let at_rate = 96_000 / 8 / 50;
        for packet in packets.iter().skip(2) {
            assert!(packet.len() * 4 < at_rate, "{} bytes", packet.len());
        }
    }

    /// The rate moves on a running encoder, and a decoder reads straight across
    /// the change with nothing said to it.
    #[test]
    fn the_rate_moves_on_a_running_encoder() {
        let mut encoder = Encoder::new(STEREO, 96_000).unwrap();
        let pcm = tone(STEREO, 20);
        let before = packets(&mut encoder, &pcm);
        encoder.set_bitrate(16_000).unwrap();
        let after = packets(&mut encoder, &pcm);
        let average = |packets: &[Vec<u8>]| packets.iter().map(Vec::len).sum::<usize>() / packets.len();
        assert!(average(&after) * 2 < average(&before), "{} against {}", average(&after), average(&before));
        let all: Vec<Vec<u8>> = before.into_iter().chain(after).collect();
        decode(STEREO, &all);
    }
}
