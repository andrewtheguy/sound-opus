# The plan: measuring the encoder and the walk for their two users

Written on 2026-10-26, before any of it is done. What the crate is now is in
the module docs of `src/lib.rs` and `src/walk.rs`; the figures this plan
calls for will go in [benchmarks.md](benchmarks.md) and [walk.md](walk.md),
and the constants' doc comments will carry their evidence, as
wlshare-vp9's `QualityWalk` does.

## Why

The encoder's settings (`OPUS_APPLICATION_AUDIO`, complexity 10, constrained
VBR, 20 ms) and every constant of `walk::BitrateWalk` were chosen by
reasoning and by scaling the picture's walk, not measured: the one number
behind them, "a few percent of a core", came from a desktop that was never
quiet, and the walk's thresholds were "settled on shaped links for the
picture, scaled to the sound's own cadence". sound-flac and wlshare have
since set the bar: a quiet machine, alone; kept artifacts; a harness that is
checked in; every number in a doc with the run it came from; every constant
with the simulator's evidence on it. This brings the Opus coding to that bar,
for the two pipelines that exist and no other.

## What the users are

Nothing here is tuned for a pipeline that does not exist. These two do.

**wlshare** (`crates/wlshare/src/audio.rs`, `rfb/audio.rs`, `session.rs`):
PipeWire hands 20 ms blocks (S16, 48 kHz stereo as the gateway asks) on the
client's own thread; each is coded by one `Encoder` at the rate the gateway
last named in a set-bitrate, and queued sixteen deep. A full queue skips the
whole PipeWire buffer before the encoder (a `dropped` count that is only
logged). The frames are written on the one TCP connection the pixels share,
untimed. No walk runs there and no silence is shed.

**zdesk**, the gateway, runs the one walk, for both of its sound streams:

- *RDP*: 44.1 kHz stereo wave buffers of about 186 ms, resampled to 48 kHz,
  coded by its own `Encoder` at a ceiling of 320 kbit/s, about nine packets a
  buffer. Between the engine and the encoder a sixteen-deep broadcast queue
  drops its oldest on overflow, which reaches the walk as `dropped()`. While
  the walk says the link is behind, a buffer that is silence is shed before
  the encoder.
- *wlshare passed through*: whatever units are queued are batched into one
  send; a lag there raises the gap frame to the browser but never reaches
  the walk as `dropped()`, and nothing is shed.

Both streams' `sent(blocked, now)` measures one thing: how long one batch
waited for room in the two-deep channel that feeds the browser's audio
WebSocket. One verdict is one batch, 186 ms of sound on RDP, not one 20 ms
packet, so `BEHIND_BLOCK` at 20 ms is a packet's length read against a
batch nine packets long. The browser plays through WebCodecs with no jitter
buffer, back to back, trimming anything past a 300 ms lead.

What that fixes about the measurement: the stream is 48 kHz stereo S16 in
every real path (44.1 kHz comes resampled), the ceiling is 320 kbit/s, the
floor 32 kbit/s, and the cost that matters is one core's share per stream at
whatever rate the walk is at, beside a VP9 encoder on the same desktop.

## The artifacts

- The fifteen recordings of sound-flac's `tools/samples.txt`, in
  `/Volumes/dasdata/backup/sound-artifacts/samples` on the Mac, on lab2 as
  `~/codes/staging-area/sound-opus-samples`: twelve from archive.org, public
  domain or CC0 (piano, orchestra, dance band, folk song, spoken verse,
  scripture, a speech, white and pink noise, a field recording, a pop track),
  and three of the user's own. All are coded whole, in 20 ms blocks, at the
  stream the users send: 48 kHz stereo (resampled first with ffmpeg where
  the file is not; a mono file sent as both channels, as a desktop would).
- Made sounds, as sound-flac's bench makes them: a tone, music, speech,
  noise, silence; fifty blocks each.
- Every run's log is kept under `docs/bench/` as JSON lines, and the run's
  packets, as Ogg Opus files to listen to, and any plots under
  `/Volumes/dasdata/backup/sound-artifacts/opus/<date>/`, so the next run
  can set this one beside it as *the run before*.

## Where it runs

lab2 (`CLAUDE.local.md`): a four-core EPYC Milan VM, Debian 13, idle, one
measurement at a time and nothing beside it, as wlshare's AGENTS.md puts it.
The harness runs single-threaded, as every real encoder does. `/tmp` is on
disk. Rust 1.99, ffmpeg 7.1. A baseline is taken there first; no figure from
any earlier machine is compared against.

## The harness

Two examples in this crate, so that it keeps to `thiserror` and the
examples build with `cargo build --examples` and nothing installed:

- `examples/bench.rs`: the encoder over every recording and made sound, at a
  grid of settings, one line of JSON per cell: microseconds a block (least of
  three passes' mean, and the least p99 and worst block, each block timed on
  its own, after a warm-up pass), bytes a block, and quality. Quality is
  measured through an independent decoder, ffmpeg's own Opus decoder (as the
  tests already do), against the source: a bark-band log-spectral distance
  and segmental SNR, both stated as proxies, with every cell's packets kept
  as Ogg Opus files so the user can listen. The grid is below. `tools/tables.py`, as sound-flac's,
  makes the tables of `docs/benchmarks.md` from the log.
- `examples/walk-sim.rs`: wlshare-vp9's `walk-sim` for sound, in virtual
  time, deterministic, runnable anywhere. It models the gateway's pipeline
  exactly: a source that yields sound in the real cadence (20 ms units from
  wlshare, 186 ms buffers from RDP), the real queues at their real depths
  (sixteen-deep broadcast that drops its oldest, the two-deep channel the
  walk times), a link that is a bottleneck of a given rate with changes
  over time and an RTT, and the browser's 300 ms clamp. Packet sizes come
  from the bench's measured bytes a block at each rate over a real
  recording, so the walk is tested on what silence, speech and music cost,
  not a constant. Its output is one line a case: the rate's path, time behind,
  sound shed or dropped, seconds not heard, lead at the browser (p50/p95),
  moves down and up, and whether the walk settled or oscillated.

## What is measured, in order

1. **Baseline.** The crate as released (v0.0.4) on lab2: time, bytes and
   quality at 32, 48, 64, 96, 128, 160, 192, 256 and 320 kbit/s. This is the
   *run before* every later row compares with.
2. **Complexity** 0 to 10, at 64, 128 and 320 kbit/s. Where the quality the
   top buys stops showing in the proxies and at what cost in time: the users
   run one encoder beside a VP9 encoder on four cores or fewer.
3. **Application**: `AUDIO` against `RESTRICTED_LOWDELAY` (which has a 2.5 ms
   lookahead against 6.5 and disables the pre-analysis; it would change
   `PRE_SKIP`, and every user's `OpusHead` with it, so this is measured and
   then decided once, not left open).
4. **Rate control**: constrained VBR against unconstrained and CBR, on
   bytes, on the spread of packet sizes (what a link at the rate actually
   sees: the p99 packet against the mean), and on quality. The walk's whole
   premise is that the number set is what a link sees.
5. **Rate against quality**, with the above settled: where the proxies and
   a listen say the stereo stream stops improving. The ceiling is zdesk's
   setting, not this crate's, but the result is what zdesk's 320 kbit/s
   constant rests on, and goes to zdesk as a finding.
6. **The walk, on the simulator**, every constant in turn, on cases drawn
   from the users: a link that narrows below the rate and stays; one that
   narrows for seconds and recovers; one that wobbles around the rate; a
   burst (a keyframe's worth of pixels on wlshare's shared connection);
   a link below the floor; each with RDP's 186 ms batches and with
   wlshare's units, and with a recording of music and one of speech with
   silences. What is tuned: `BEHIND_BLOCK` and `CLEAR_BLOCK` read against a
   batch, the verdict window, the cooldowns, the step sizes, the clear and
   relief spans, the refusal hold. The goal is the picture walk's: quick to
   give up, slow to take back, settle rather than oscillate, and the least
   sound not heard. Each constant's doc comment gets the case figures.
7. **The floor.** Whether 32 kbit/s stereo is where libopus still keeps the
   band and the stereo, measured, and whether a lower floor buys continuity
   anywhere in the cases.

## What may change in the users

The user allowed changes in wlshare and zdesk where the redo needs them.
Those seen so far, each to be confirmed by the simulator before it is made:

- zdesk: the passed wlshare stream's lag reaches the walk as `dropped()`,
  as the RDP stream's does. Found by reading, not yet shown to matter.
- zdesk: `sent` told how much sound a batch holds, if the simulator shows a
  verdict per batch needs it (`BEHIND_BLOCK` scaled to the batch rather
  than fixed at a packet).
- wlshare: its skipped buffers are only logged; if the simulator shows the
  walk needs to hear them, that is a protocol change (`wlshare-proto`
  v3) and goes on wlshare's roadmap rather than being done here.
- Both: the pin bump to the release this makes.

## What is not done

- No tuning for a pipeline neither user has: no per-packet walk, no FEC,
  no DTX (both ride TCP), no frame size other than 20 ms (the units' cadence
  is fixed by both users' wire).
- No listening test is a measurement here; the proxies are named as proxies
  and the decoded files are kept for the user to listen to.
- No change to wire framing, stream negotiation or where blocking is
  measured: those are the users' (AGENTS.md).

## The order of work

1. `examples/bench.rs` and `tools/tables.py`; run the baseline (1) on lab2;
   write `docs/benchmarks.md` with it.
2. Items 2 to 5, one at a time, each a run on lab2 and a row in the doc;
   each setting change a commit whose message carries the before and after.
3. `examples/walk-sim.rs`; items 6 and 7; `docs/walk.md`; the constants'
   comments.
4. Release v0.0.5; bump the pins in wlshare and zdesk, with whatever changes
   the simulator called for there, each on its own branch and PR.
