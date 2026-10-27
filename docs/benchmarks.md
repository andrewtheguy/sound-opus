# What the encoder was measured at

The figures the encoder's settings and the walk's constants are decided on,
taken as [plan.md](plan.md) lays out: on a quiet machine, alone, over the
recordings sound-flac's benchmarks are made on, with every run's log kept.
What the crate is, is in the module docs of `src/lib.rs` and `src/walk.rs`.

The tables are made by `examples/bench.rs`, which codes two sets at every
rate and setting it is given: five made sounds, fifty 20 ms blocks of each,
as sound-flac's bench makes them, and the fifteen recordings of sound-flac's
`tools/samples.txt`, twelve from archive.org, public domain or CC0, and three
of the user's own, whole. Every sound is the stream both users send, 48 kHz
stereo of 16-bit samples, in blocks of 20 ms: ffmpeg brings a recording to
it, resampling the six at 44.1 or 96 kHz and putting the one mono recording
on both channels at its own level, as a desktop plays it. A made sound's time
is one block's, the least of five passes' mean; a recording's the least of
three passes' mean block, each block timed on its own after one pass to warm
up, each pass from a new encoder, every pass's packets checked against the
warm-up's; with it the least of the passes' 99th-percentile and worst blocks.
The bytes are the packets', and *peak* the most any second of them (fifty
packets running) came to, which is what a link at the rate meets.

Quality is read through an independent decoder, FFmpeg's own Opus decoder as
the tests use it, against the sound that went in, the encoder's lookahead
taken off the front and each frame's mean off both signals (libopus takes an
offset out before it codes; the Shostakovich recording has one of a quarter
of full scale). The three figures are proxies, not listening, and are named
as such everywhere:

- *LSD*: the log-spectral distance, in dB, over the 25 critical bands up to
  20 kHz, of each channel, over 2048-sample Hann frames half overlapped,
  averaged over the frames of the source above −60 dBFS. Lower is nearer.
- *Side LSD*: the same of the side, (left − right) / 2, over the frames
  whose side is above −60 dBFS and within 30 dB of the channels: how much of
  the stereo image is kept. A recording that is mono, or mono in all but its
  noise (Shostakovich's side is 43 dB under its mid), has none.
- *SegSNR*: the segmental signal-to-noise ratio, in dB, over the 20 ms blocks
  above −60 dBFS, each held to −10..60 dB. A waveform measure, which a
  perceptual coder does not aim at: it ranks rates of one recording, not
  recordings against each other (white noise, which nothing can predict, is
  near 0 dB at every rate). FFmpeg's decoder puts the sound of a SILK or
  hybrid packet three or four samples before where it puts a CELT packet's,
  so the blocks of each mode are compared at that mode's best alignment.

`tools/tables.py` makes the tables below from the log, a JSON object a line,
and the logs are kept in `docs/bench/`, so that the next run's tables can set
this one beside them as *the run before*. Every cell's packets are kept as an
Ogg Opus file to listen to, with the run's log, under
`/Volumes/dasdata/backup/sound-artifacts/opus/<date>/<run>/` on the Mac (this
run's under `2026-10-26/baseline/`):

```sh
# on lab2, alone; the recordings rsynced from the store sound-flac's tools/samples.sh fills
cargo build --release --example bench
SAMPLES=../sound-opus-samples KEEP=../sound-opus-scratch/baseline/opus taskset -c 3 \
    target/release/examples/bench > ../sound-opus-scratch/baseline/bench.jsonl
# here
python3 -I tools/tables.py docs/bench/lab2.jsonl
python3 -I tools/tables.py docs/bench/lab2.jsonl --against crate              # settings against the crate's
python3 -I tools/tables.py docs/bench/lab2.jsonl --before before.jsonl        # with the run before
```

## The baseline

`docs/bench/lab2.jsonl`, the baseline: the crate as released, v0.0.4
(`015b983`), at 32, 48, 64, 96, 128, 160, 192, 256 and 320 kbit/s, the walk's
floor to zdesk's ceiling, on 2026-10-26. lab2 is a four-core AMD EPYC Milan
virtual machine, Debian 13, Linux 6.12, with AVX2 and no AVX-512, idle, swap
off, `/tmp` on disk; the bench ran alone, pinned to one core by `taskset`,
built by Rust 1.99.1, with ffmpeg 7.1.5. No figure from an earlier machine is
compared against: every *a few percent of a core* in the crate's comments
before this came from a desktop that was never quiet.

### What it says

**The cost is under one per cent of a core at every rate.** The mean block
over the recordings is 102 to 154 µs, 0.51 % to 0.77 % of one core of this
machine, and grows with the rate from 64 kbit/s up. The 99th-percentile
block of the worst recording is 152 to 245 µs from 64 kbit/s up. The
comment on `COMPLEXITY`, *a few percent of a core*, is several times what
it is; what complexity 10 costs against less is [below](#complexity).

**The lowest rates are the dearest.** At 32 and 48 kbit/s libopus codes
speech as SILK or hybrid (none of Leviticus's packets are CELT, a tenth of
JFK's, under two thirds of the Raven's), and a SILK block costs nearly twice
a CELT one: 192 µs a block for Leviticus at 32 kbit/s against 104 µs at 64,
and its 99th-percentile block 236 µs against 134; the Raven's, part SILK, is
the worst recording's at 32 kbit/s, 341 µs. So the walk's floor, where
it goes when a link is worst, is where the encoder costs most. From 64 kbit/s
up every packet of every recording is CELT.

**The rate set is the rate a link sees.** From 64 kbit/s up every
recording's packets come to within one per cent of the rate, a little under
rather than over (the loud master and the mono reading, whose quiet passages
are cheap, are the furthest under); at 32 and 48 kbit/s within 2.7 %. The
most any second of any recording comes to is at most 2.7 % over the rate
from 64 kbit/s up (65.7 at 64, 326.8 at 320), 9.4 % at 48 and 10.9 % at 32.
A single packet is not held near it, as constrained VBR does not promise:
the 99th-percentile packet is up to 1.5 times the mean at every rate (1209
bytes at 320 kbit/s against 800), where white noise's packets are all one
size. Over
a second, which is what a link's queue sees, constrained VBR keeps the
premise the walk is built on.

**What goes first under 128 kbit/s is the stereo image.** The LSD of each
channel falls steadily, 1.59 dB at 32 kbit/s to 0.14 at 320, and is under
0.4 dB from 128 up. The side's falls from 6.9 dB at 32 and 4.9 at 64 to
2.1 at 128 and 1.35 at 160, and under one dB from 256: below 128 kbit/s
libopus folds much of the image toward the middle, the more so on wide mixes
(the indie rock's side is 8.4 dB off at 32 kbit/s, Bach's piano 2.8). At 64
kbit/s and below it also codes some recordings below fullband: 2 % of the
Goldberg Aria's packets, 22 % of the Fantasy's and 14 % of the station's are
fullband; from 96 kbit/s every packet is. SegSNR rises by one to two dB a
step all the way to 320 kbit/s; above 192 the proxies keep improving but by
little, which is where item 5 asks whether zdesk's 320 kbit/s is worth its
cost on a link.

**The worst block is the machine's.** The least over three passes of each
pass's worst block is still 1.0 to 2.1 ms on the long recordings, at a
different block at every rate (Leviticus's at blocks 99, 139, 21127, 8350
and 21968 in turn): the virtual machine's, not the encoder's. The 99th
percentile is the encoder's figure; the worst is reported, not used.

## Complexity

Item 2 of the plan: libopus's effort, 0 to 10, against the crate's 10, the
rest of the crate's settings unchanged. Two runs on lab2 as the baseline
was made, the day after it: `docs/bench/lab2-complexity.jsonl`, every
complexity at 64, 128 and 320 kbit/s, where every packet is CELT; and
`docs/bench/lab2-complexity-silk.jsonl`, 5 and 7 to 10 at 32 and 48 kbit/s,
where libopus codes speech as SILK and where complexity 9 and 10 can differ
from 8 at all. The packets are under `2026-10-26/complexity/` and
`2026-10-26/complexity-silk/`.

```sh
SETTINGS=$(seq -s, 0 10 | sed 's/\([0-9]*\)/c\1-audio-cvbr/g') RATES=64,128,320 ... bench
SETTINGS=c5-audio-cvbr,c7-audio-cvbr,c8-audio-cvbr,c9-audio-cvbr,c10-audio-cvbr RATES=32,48 ... bench
```

`c10-audio-cvbr` is the crate's encoder, which the bench checks, and its
cells came out as the baseline's to the byte and in every proxy, and each
recording's mean block within 3.1 % of the baseline's.

### What it says

**Complexity 10 stays: what it costs is under one per cent of a core, and
what less gives up is a choice libopus makes.** The steps that change
anything are the ones libopus 1.6.1's source names:

- *9 and 10 are 8.* At 64 kbit/s and up their packets are 8's, byte for
  byte; at 32 and 48 they differ only in SILK's packets, and every proxy is
  8's to 0.02 dB, at the same cost. The top two steps buy nothing measured
  and cost nothing either.
- *8 searches the stereo angle by rate and distortion* (`theta_rdo`,
  `celt/bands.c:1618`). It costs 12 to 33 µs a block, at most a sixth
  of a per cent of a core, and moves the proxies two ways: each channel's LSD
  and the SegSNR are better on nearly every recording (SegSNR by up
  to 0.3 dB), and the side's LSD is worse on the stereo recordings with a
  quiet side, the Raven's 3.95 dB at 128 kbit/s against 4.60, the talk
  over music's 2.69 against 3.28. libopus trades error in a quiet side for
  error in the channels, by its own measure of what is heard; the side LSD
  weighs a quiet side as much as a loud one.
- *7 runs libopus's analysis* (`src/opus_encoder.c:1252`), which decides
  the bandwidth and whether speech is coded as SILK. Under 7 every packet is
  CELT and fullband at every rate measured. At 64 kbit/s the analysis codes 78 % of
  the Fantasy's packets, 86 % of the station's and 99 % of the Goldberg
  Aria's below fullband, and their LSD rises (the station's 0.66 dB to
  2.42) while their SegSNR rises too (the Aria's 21.1 dB to 22.4): fewer
  bands, each coded better. At 32 and 48 kbit/s it codes the readings as
  SILK or hybrid: Leviticus's SegSNR is 14.9 dB against CELT's 11.7, JFK's
  15.0 against 13.7, the Raven's 18.8 against 17.3, and their LSD worse,
  1.68 dB against 1.32 for Leviticus. That is the costliest coding the
  encoder does, 194 µs a block for Leviticus at 32 kbit/s against 62 at
  complexity 5, still under one per cent of a core.
- *5 adds CELT's pitch prefilter*, 12 µs a block, and +0.2 dB of SegSNR at
  64 kbit/s; 5 and 6 make the same packets.
- *0 to 4* lose a little more SegSNR each, 0.5 dB at 64 kbit/s from 10 to
  0, and complexity 0 holds the packets nearest the mean (a 99th-percentile
  packet of 348 bytes at 128 kbit/s against 481), which over a second, what
  a link sees, is the same rate.

Complexity 0 costs 37 to 68 µs a block against 10's 102 to 155: the most
any setting saves is 0.43 % of one core, beside a VP9 encoder on the same
desktop that takes tens of per cent. Where the proxies split, they do so
because libopus chose a bandwidth, a coding mode or a stereo angle by what
is heard; the proxies do not overrule it, and the saving is too small to
listen against. The `COMPLEXITY` comment now says this, in place of *a few
percent of a core*.

### The tables

The recordings together, a table for each rate, as `tools/tables.py` sets
the settings side by side.

#### 32 kbit/s

| Setting | µs a block | Of one core | 99th, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `c5-audio-cvbr` | 66.4 | 0.33% | 94.8 | 32.4 | 33.0 | 116 | 1.32 | 6.83 | 11.37 | 100% | 100% |
| `c7-audio-cvbr` | 114 | 0.57% | 290 | 31.9 | 35.5 | 119 | 1.61 | 6.75 | 11.75 | 69% | 83% |
| `c8-audio-cvbr` | 127 | 0.63% | 343 | 31.9 | 35.5 | 123 | 1.59 | 6.94 | 11.90 | 69% | 83% |
| `c9-audio-cvbr` | 127 | 0.63% | 332 | 31.9 | 35.5 | 123 | 1.59 | 6.93 | 11.89 | 69% | 83% |
| `c10-audio-cvbr` | 126 | 0.63% | 341 | 31.9 | 35.5 | 123 | 1.59 | 6.92 | 11.89 | 69% | 83% |

#### 48 kbit/s

| Setting | µs a block | Of one core | 99th, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `c5-audio-cvbr` | 70.3 | 0.35% | 95.5 | 48.3 | 49.4 | 178 | 1.02 | 5.83 | 13.32 | 100% | 100% |
| `c7-audio-cvbr` | 114 | 0.57% | 309 | 48.0 | 52.5 | 178 | 1.31 | 5.83 | 13.75 | 71% | 83% |
| `c8-audio-cvbr` | 126 | 0.63% | 342 | 48.0 | 52.5 | 183 | 1.28 | 6.02 | 13.89 | 72% | 83% |
| `c9-audio-cvbr` | 126 | 0.63% | 340 | 48.0 | 52.5 | 183 | 1.28 | 6.04 | 13.88 | 72% | 83% |
| `c10-audio-cvbr` | 126 | 0.63% | 348 | 48.0 | 52.5 | 183 | 1.28 | 6.03 | 13.88 | 72% | 83% |

#### 64 kbit/s

| Setting | µs a block | Of one core | 99th, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `c0-audio-cvbr` | 37.3 | 0.19% | 56.7 | 64.3 | 65.7 | 203 | 0.85 | 4.81 | 14.74 | 100% | 100% |
| `c1-audio-cvbr` | 48.3 | 0.24% | 69.0 | 64.3 | 65.7 | 242 | 0.86 | 4.75 | 14.63 | 100% | 100% |
| `c2-audio-cvbr` | 52.9 | 0.26% | 74.9 | 64.3 | 65.7 | 242 | 0.85 | 4.76 | 14.65 | 100% | 100% |
| `c3-audio-cvbr` | 61.9 | 0.31% | 87.3 | 64.3 | 65.7 | 242 | 0.85 | 4.75 | 14.64 | 100% | 100% |
| `c4-audio-cvbr` | 62.3 | 0.31% | 88.6 | 64.3 | 65.7 | 242 | 0.85 | 4.70 | 14.65 | 100% | 100% |
| `c5-audio-cvbr` | 74.5 | 0.37% | 101 | 64.3 | 65.7 | 242 | 0.82 | 4.66 | 14.86 | 100% | 100% |
| `c6-audio-cvbr` | 74.3 | 0.37% | 100 | 64.3 | 65.7 | 242 | 0.82 | 4.66 | 14.86 | 100% | 100% |
| `c7-audio-cvbr` | 90.4 | 0.45% | 125 | 64.3 | 65.7 | 237 | 1.00 | 4.68 | 15.09 | 100% | 83% |
| `c8-audio-cvbr` | 102 | 0.51% | 144 | 64.3 | 65.7 | 242 | 0.98 | 4.92 | 15.22 | 100% | 83% |
| `c9-audio-cvbr` | 102 | 0.51% | 143 | 64.3 | 65.7 | 242 | 0.98 | 4.92 | 15.22 | 100% | 83% |
| `c10-audio-cvbr` | 102 | 0.51% | 145 | 64.3 | 65.7 | 242 | 0.98 | 4.92 | 15.22 | 100% | 83% |

#### 128 kbit/s

| Setting | µs a block | Of one core | 99th, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `c0-audio-cvbr` | 48.5 | 0.24% | 71.6 | 128.3 | 131.0 | 348 | 0.41 | 1.99 | 20.47 | 100% | 100% |
| `c1-audio-cvbr` | 60.1 | 0.30% | 86.3 | 128.3 | 131.0 | 488 | 0.42 | 1.96 | 20.34 | 100% | 100% |
| `c2-audio-cvbr` | 65.2 | 0.33% | 93.0 | 128.3 | 131.0 | 488 | 0.41 | 1.96 | 20.34 | 100% | 100% |
| `c3-audio-cvbr` | 73.9 | 0.37% | 104 | 128.3 | 131.0 | 488 | 0.41 | 1.95 | 20.34 | 100% | 100% |
| `c4-audio-cvbr` | 74.9 | 0.37% | 105 | 128.3 | 131.0 | 488 | 0.41 | 1.92 | 20.34 | 100% | 100% |
| `c5-audio-cvbr` | 86.8 | 0.43% | 119 | 128.3 | 131.0 | 488 | 0.40 | 1.90 | 20.52 | 100% | 100% |
| `c6-audio-cvbr` | 86.6 | 0.43% | 119 | 128.3 | 131.0 | 488 | 0.40 | 1.90 | 20.52 | 100% | 100% |
| `c7-audio-cvbr` | 104 | 0.52% | 145 | 128.3 | 131.0 | 477 | 0.40 | 1.90 | 20.61 | 100% | 100% |
| `c8-audio-cvbr` | 128 | 0.64% | 189 | 128.3 | 131.0 | 481 | 0.39 | 2.07 | 20.70 | 100% | 100% |
| `c9-audio-cvbr` | 128 | 0.64% | 204 | 128.3 | 131.0 | 481 | 0.39 | 2.07 | 20.70 | 100% | 100% |
| `c10-audio-cvbr` | 128 | 0.64% | 192 | 128.3 | 131.0 | 481 | 0.39 | 2.07 | 20.70 | 100% | 100% |

#### 320 kbit/s

| Setting | µs a block | Of one core | 99th, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `c0-audio-cvbr` | 68.4 | 0.34% | 98.7 | 320.1 | 326.8 | 844 | 0.15 | 0.57 | 26.46 | 100% | 100% |
| `c1-audio-cvbr` | 77.0 | 0.39% | 111 | 320.0 | 326.8 | 1218 | 0.15 | 0.58 | 26.45 | 100% | 100% |
| `c2-audio-cvbr` | 82.2 | 0.41% | 118 | 320.0 | 326.8 | 1219 | 0.15 | 0.58 | 26.44 | 100% | 100% |
| `c3-audio-cvbr` | 90.8 | 0.45% | 127 | 320.0 | 326.8 | 1218 | 0.15 | 0.58 | 26.44 | 100% | 100% |
| `c4-audio-cvbr` | 91.6 | 0.46% | 130 | 320.0 | 326.8 | 1218 | 0.15 | 0.57 | 26.44 | 100% | 100% |
| `c5-audio-cvbr` | 104 | 0.52% | 146 | 320.0 | 326.8 | 1217 | 0.15 | 0.56 | 26.57 | 100% | 100% |
| `c6-audio-cvbr` | 104 | 0.52% | 145 | 320.0 | 326.8 | 1217 | 0.15 | 0.56 | 26.57 | 100% | 100% |
| `c7-audio-cvbr` | 121 | 0.61% | 168 | 320.0 | 326.8 | 1202 | 0.15 | 0.55 | 26.64 | 100% | 100% |
| `c8-audio-cvbr` | 154 | 0.77% | 230 | 320.0 | 326.8 | 1209 | 0.14 | 0.56 | 26.68 | 100% | 100% |
| `c9-audio-cvbr` | 155 | 0.77% | 246 | 320.0 | 326.8 | 1209 | 0.14 | 0.56 | 26.68 | 100% | 100% |
| `c10-audio-cvbr` | 155 | 0.77% | 236 | 320.0 | 326.8 | 1209 | 0.14 | 0.56 | 26.68 | 100% | 100% |

## Application

Item 3: `OPUS_APPLICATION_AUDIO`, the crate's, against
`OPUS_APPLICATION_RESTRICTED_LOWDELAY`, both at complexity 10 and
constrained VBR, at every rate of the baseline, in one run on lab2:
`docs/bench/lab2-application.jsonl`, the packets under
`2026-10-26/application/`. libopus 1.6.1's `RESTRICTED_CELT` codes as low
delay does, without SILK's state (`src/opus_encoder.c:1470`, `:1903`), so it
is not measured apart.

```sh
SETTINGS=c10-audio-cvbr,c10-lowdelay-cvbr ... bench
```

Low delay codes every packet as CELT and drops the 4 ms of delay
compensation in front of it: its lookahead is 120 samples, 2.5 ms, against
`PRE_SKIP`'s 312, 6.5 ms. Its analysis still runs, so it picks the same
bandwidths.

### What it says

**`AUDIO` stays: from 64 kbit/s up the two are one encoder, and under it
low delay gives up SILK for 4 ms.** From 64 kbit/s up, where `AUDIO` codes
everything as CELT anyway, the two cost the same to 0.4 % of mean block, come to the same rate and peak, and have the same LSD and side LSD
to 0.01 dB; their SegSNR differs by 0.03 to 0.05 dB. At 32 and 48 kbit/s
low delay is 0.71 and 0.75 of `AUDIO`'s mean block and 0.37 and 0.40 of
its worst 99th percentile (128 µs against 341 at 32 kbit/s), because it
codes the readings as CELT: Leviticus at 91 µs a block against 193, its
SegSNR 12.0 dB against 14.9 and its LSD 1.30 against 1.68; JFK's SegSNR
13.9 against 15.0, the Raven's 17.8 against 18.8. Every other recording is
within 0.16 dB of SegSNR, but the station, 0.8 dB better at 32 kbit/s. That is the split
complexity under 7 showed, the same choice libopus's analysis makes for
speech, given up.

What low delay buys is 4 ms of latency. Neither user is bound by it: zdesk
hands the browser a batch of 186 ms of RDP sound at a time, the browser
plays with a lead of up to 300 ms, and wlshare's blocks share one TCP
connection with the pixels. Taking it would change `PRE_SKIP`,
and every user's `OpusHead` with it, for nothing a listener of these
streams hears; what it saves at the floor is 0.19 % of a core, 0.5 % on a reading.

### The tables

The recordings together, low delay against `AUDIO` and on its own; `AUDIO`'s
are the baseline's.

#### `c10-lowdelay-cvbr` against `c10-audio-cvbr`

Time and bytes as this setting over the other; the proxies as this setting
less the other.

| Rate, kbit/s | Time | 99th | Came to | Peak | 99th packet | LSD | Side LSD | SegSNR | CELT, this | CELT, other |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | $\color{green}{0.71}$ | $\color{green}{0.37}$ | $\color{red}{1.01}$ | $\color{green}{0.93}$ | $\color{green}{0.99}$ | $\color{green}{-0.12}$ | $\color{red}{+0.09}$ | $\color{red}{-0.24}$ | 100% | 69% |
| 48 | $\color{green}{0.75}$ | $\color{green}{0.40}$ | $\color{red}{1.01}$ | $\color{green}{0.94}$ | 1.00 | $\color{green}{-0.10}$ | $\color{red}{+0.02}$ | $\color{red}{-0.18}$ | 100% | 72% |
| 64 | 1.00 | $\color{green}{0.98}$ | 1.00 | 1.00 | 1.00 | -0.00 | -0.00 | $\color{green}{+0.05}$ | 100% | 100% |
| 96 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | -0.00 | $\color{green}{-0.01}$ | $\color{green}{+0.05}$ | 100% | 100% |
| 128 | 1.00 | $\color{green}{0.99}$ | 1.00 | 1.00 | 1.00 | +0.00 | -0.00 | $\color{green}{+0.05}$ | 100% | 100% |
| 160 | 1.00 | $\color{red}{1.08}$ | 1.00 | 1.00 | 1.00 | -0.00 | -0.00 | $\color{green}{+0.05}$ | 100% | 100% |
| 192 | 1.00 | $\color{red}{1.08}$ | 1.00 | 1.00 | 1.00 | +0.00 | -0.00 | $\color{green}{+0.05}$ | 100% | 100% |
| 256 | 1.00 | $\color{red}{1.02}$ | 1.00 | 1.00 | 1.00 | -0.00 | -0.00 | $\color{green}{+0.03}$ | 100% | 100% |
| 320 | 1.00 | $\color{red}{1.01}$ | 1.00 | 1.00 | 1.00 | +0.00 | -0.00 | $\color{green}{+0.03}$ | 100% | 100% |

#### `c10-lowdelay-cvbr`

15 recordings, 103 minutes of sound. A block is 20 ms; *of one core* is the mean block over
20 ms. The 99th-percentile and worst blocks are the worst recording's.
*Peak* is the most any second of any recording came to.

| Rate, kbit/s | µs a block | Of one core | 99th, µs | Worst, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | 89.3 | 0.45% | 128 | 1572 | 32.4 | 33.0 | 122 | 1.46 | 7.01 | 11.65 | 100% | 83% |
| 48 | 94.7 | 0.47% | 136 | 1581 | 48.3 | 49.4 | 183 | 1.18 | 6.04 | 13.70 | 100% | 83% |
| 64 | 102 | 0.51% | 145 | 1582 | 64.3 | 65.7 | 243 | 0.98 | 4.92 | 15.27 | 100% | 83% |
| 96 | 116 | 0.58% | 175 | 1620 | 96.3 | 98.3 | 360 | 0.52 | 3.38 | 18.55 | 100% | 100% |
| 128 | 128 | 0.64% | 193 | 1597 | 128.3 | 131.0 | 481 | 0.39 | 2.06 | 20.74 | 100% | 100% |
| 160 | 139 | 0.69% | 228 | 1664 | 160.2 | 163.6 | 602 | 0.32 | 1.34 | 22.20 | 100% | 100% |
| 192 | 144 | 0.72% | 231 | 1662 | 192.2 | 196.2 | 722 | 0.27 | 1.07 | 23.38 | 100% | 100% |
| 256 | 151 | 0.75% | 234 | 4167 | 256.1 | 261.5 | 966 | 0.20 | 0.77 | 25.38 | 100% | 100% |
| 320 | 155 | 0.77% | 241 | 1666 | 320.0 | 326.8 | 1210 | 0.14 | 0.56 | 26.71 | 100% | 100% |

## Rate control

Item 4: constrained VBR, the crate's, against unconstrained VBR and CBR,
at complexity 10 and `AUDIO`, at every rate of the baseline, in one run on
lab2: `docs/bench/lab2-vbr.jsonl`, the packets under `2026-10-26/vbr/`.

```sh
SETTINGS=c10-audio-cvbr,c10-audio-vbr,c10-audio-cbr ... bench
```

### What it says

**Constrained VBR stays: unconstrained VBR breaks the walk's premise, and
CBR sends silence at the full rate.**

*Unconstrained VBR* comes to 2 to 8 % under the rate over a recording, but
its busiest second is 28 to 45 % over it (178 kbit/s at 128, 419 at 320),
against constrained VBR's 2.7 % at most from 64 kbit/s up, and its
99th-percentile packet is up to 1.30 times constrained VBR's. A link set
at the number would meet nearly half as much again. What it buys is moving bytes
between recordings, not quality over them: it gives the piano more (the
Goldberg Aria's SegSNR at 96 kbit/s 28.1 dB against 25.9) and speech less
(Leviticus's at 128 kbit/s 22.0 dB against 24.2), and over the recordings
its SegSNR is 0.29 dB lower to 0.18 higher and its side LSD up to 0.26 dB
worse, at the same cost.

*CBR* makes every packet the rate's size: its busiest second is the rate
exactly, 2 to 10 % under constrained VBR's, and its proxies are within
0.12 dB of constrained VBR's from 128 kbit/s up. But a packet of silence is the rate's
size too, 800 bytes at 320 kbit/s against constrained VBR's 3, and a
desktop is often silent: wlshare sends every block, and zdesk
sheds silence only while the link is behind. At 96 kbit/s CBR codes three
recordings below fullband, the Fantasy, the Aria and the station (libopus
counts a CBR rate a twelfth lower when it picks the bandwidth,
`src/opus_encoder.c:1037`), and their LSD rises, the station's 0.42 dB to
2.35. At 32 and 48 kbit/s its SILK costs 8 % more a block, and its worst
99th percentile 1.45 times as much.

### The tables

The recordings together, each against constrained VBR and on its own;
constrained VBR's are the baseline's.

#### `c10-audio-vbr` against `c10-audio-cvbr`

Time and bytes as this setting over the other; the proxies as this setting
less the other.

| Rate, kbit/s | Time | 99th | Came to | Peak | 99th packet | LSD | Side LSD | SegSNR | CELT, this | CELT, other |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | 1.00 | $\color{red}{1.02}$ | $\color{green}{0.98}$ | $\color{red}{1.45}$ | $\color{red}{1.24}$ | $\color{red}{+0.02}$ | $\color{red}{+0.20}$ | $\color{green}{+0.09}$ | 69% | 69% |
| 48 | 1.00 | $\color{red}{1.13}$ | $\color{green}{0.98}$ | $\color{red}{1.40}$ | $\color{red}{1.27}$ | -0.00 | $\color{red}{+0.03}$ | $\color{green}{+0.18}$ | 72% | 72% |
| 64 | $\color{green}{0.99}$ | $\color{red}{1.02}$ | $\color{green}{0.94}$ | $\color{red}{1.43}$ | $\color{red}{1.29}$ | $\color{red}{+0.01}$ | $\color{red}{+0.03}$ | $\color{red}{-0.01}$ | 100% | 100% |
| 96 | 1.00 | 1.00 | $\color{green}{0.92}$ | $\color{red}{1.41}$ | $\color{red}{1.30}$ | $\color{red}{+0.02}$ | $\color{red}{+0.04}$ | $\color{red}{-0.16}$ | 100% | 100% |
| 128 | $\color{green}{0.97}$ | $\color{red}{1.04}$ | $\color{green}{0.93}$ | $\color{red}{1.36}$ | $\color{red}{1.30}$ | $\color{red}{+0.01}$ | $\color{red}{+0.26}$ | $\color{red}{-0.29}$ | 100% | 100% |
| 160 | $\color{green}{0.98}$ | $\color{green}{0.99}$ | $\color{green}{0.94}$ | $\color{red}{1.32}$ | $\color{red}{1.30}$ | +0.00 | $\color{green}{-0.03}$ | $\color{red}{-0.13}$ | 100% | 100% |
| 192 | $\color{green}{0.99}$ | $\color{green}{0.99}$ | $\color{green}{0.94}$ | $\color{red}{1.31}$ | $\color{red}{1.30}$ | +0.00 | $\color{green}{-0.02}$ | $\color{red}{-0.01}$ | 100% | 100% |
| 256 | 1.00 | $\color{red}{1.03}$ | $\color{green}{0.96}$ | $\color{red}{1.31}$ | $\color{red}{1.29}$ | +0.00 | $\color{green}{-0.02}$ | $\color{green}{+0.03}$ | 100% | 100% |
| 320 | 1.00 | $\color{red}{1.03}$ | $\color{green}{0.97}$ | $\color{red}{1.28}$ | $\color{red}{1.06}$ | +0.00 | $\color{green}{-0.02}$ | $\color{green}{+0.17}$ | 100% | 100% |

#### `c10-audio-cbr` against `c10-audio-cvbr`

Time and bytes as this setting over the other; the proxies as this setting
less the other.

| Rate, kbit/s | Time | 99th | Came to | Peak | 99th packet | LSD | Side LSD | SegSNR | CELT, this | CELT, other |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | $\color{red}{1.08}$ | $\color{red}{1.45}$ | 1.00 | $\color{green}{0.90}$ | $\color{green}{0.65}$ | $\color{red}{+0.02}$ | $\color{red}{+0.13}$ | $\color{red}{-0.16}$ | 69% | 69% |
| 48 | $\color{red}{1.08}$ | $\color{red}{1.43}$ | 1.00 | $\color{green}{0.91}$ | $\color{green}{0.66}$ | $\color{red}{+0.03}$ | $\color{green}{-0.04}$ | $\color{red}{-0.09}$ | 71% | 72% |
| 64 | $\color{green}{0.99}$ | $\color{red}{1.01}$ | $\color{green}{0.99}$ | $\color{green}{0.97}$ | $\color{green}{0.66}$ | $\color{red}{+0.02}$ | $\color{green}{-0.01}$ | $\color{green}{+0.06}$ | 100% | 100% |
| 96 | $\color{green}{0.98}$ | $\color{red}{1.03}$ | 1.00 | $\color{green}{0.98}$ | $\color{green}{0.67}$ | $\color{red}{+0.21}$ | $\color{green}{-0.01}$ | $\color{green}{+0.16}$ | 100% | 100% |
| 128 | 1.00 | $\color{red}{1.01}$ | 1.00 | $\color{green}{0.98}$ | $\color{green}{0.67}$ | -0.00 | $\color{green}{-0.01}$ | $\color{green}{+0.03}$ | 100% | 100% |
| 160 | $\color{green}{0.99}$ | $\color{green}{0.98}$ | 1.00 | $\color{green}{0.98}$ | $\color{green}{0.67}$ | -0.00 | $\color{red}{+0.04}$ | $\color{green}{+0.12}$ | 100% | 100% |
| 192 | 1.00 | $\color{green}{0.99}$ | 1.00 | $\color{green}{0.98}$ | $\color{green}{0.66}$ | +0.00 | -0.00 | $\color{red}{-0.03}$ | 100% | 100% |
| 256 | 1.00 | 1.00 | 1.00 | $\color{green}{0.98}$ | $\color{green}{0.66}$ | -0.00 | +0.00 | -0.00 | 100% | 100% |
| 320 | 1.00 | 1.00 | 1.00 | $\color{green}{0.98}$ | $\color{green}{0.66}$ | +0.00 | -0.00 | $\color{red}{-0.05}$ | 100% | 100% |

#### `c10-audio-vbr`

15 recordings, 103 minutes of sound. A block is 20 ms; *of one core* is the mean block over
20 ms. The 99th-percentile and worst blocks are the worst recording's.
*Peak* is the most any second of any recording came to.

| Rate, kbit/s | µs a block | Of one core | 99th, µs | Worst, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | 126 | 0.63% | 351 | 1931 | 31.4 | 51.4 | 153 | 1.60 | 7.11 | 11.99 | 69% | 83% |
| 48 | 126 | 0.63% | 372 | 4115 | 47.2 | 73.6 | 232 | 1.28 | 6.06 | 14.06 | 72% | 83% |
| 64 | 101 | 0.51% | 148 | 1917 | 60.4 | 93.7 | 311 | 1.00 | 4.95 | 15.20 | 100% | 83% |
| 96 | 116 | 0.58% | 173 | 1725 | 89.0 | 138.4 | 467 | 0.54 | 3.42 | 18.33 | 100% | 100% |
| 128 | 125 | 0.63% | 190 | 1814 | 119.5 | 178.2 | 624 | 0.41 | 2.33 | 20.41 | 100% | 100% |
| 160 | 136 | 0.68% | 210 | 1741 | 150.3 | 215.9 | 780 | 0.33 | 1.31 | 22.02 | 100% | 100% |
| 192 | 141 | 0.71% | 214 | 1638 | 181.4 | 257.9 | 935 | 0.27 | 1.05 | 23.32 | 100% | 100% |
| 256 | 150 | 0.75% | 233 | 1690 | 246.1 | 341.6 | 1244 | 0.20 | 0.75 | 25.37 | 100% | 100% |
| 320 | 155 | 0.77% | 246 | 1729 | 310.1 | 419.1 | 1276 | 0.14 | 0.54 | 26.85 | 100% | 100% |

#### `c10-audio-cbr`

15 recordings, 103 minutes of sound. A block is 20 ms; *of one core* is the mean block over
20 ms. The 99th-percentile and worst blocks are the worst recording's.
*Peak* is the most any second of any recording came to.

| Rate, kbit/s | µs a block | Of one core | 99th, µs | Worst, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | 136 | 0.68% | 500 | 1753 | 32.0 | 32.0 | 80 | 1.61 | 7.05 | 11.73 | 69% | 83% |
| 48 | 135 | 0.68% | 474 | 2399 | 48.0 | 48.0 | 120 | 1.31 | 5.98 | 13.79 | 71% | 83% |
| 64 | 101 | 0.50% | 147 | 2488 | 64.0 | 64.0 | 160 | 1.01 | 4.92 | 15.27 | 100% | 83% |
| 96 | 114 | 0.57% | 178 | 1878 | 96.0 | 96.0 | 240 | 0.73 | 3.38 | 18.65 | 100% | 83% |
| 128 | 128 | 0.64% | 185 | 1849 | 128.0 | 128.0 | 320 | 0.39 | 2.06 | 20.73 | 100% | 100% |
| 160 | 137 | 0.69% | 209 | 2114 | 160.0 | 160.0 | 400 | 0.32 | 1.38 | 22.28 | 100% | 100% |
| 192 | 144 | 0.72% | 212 | 2260 | 192.0 | 192.0 | 480 | 0.27 | 1.07 | 23.30 | 100% | 100% |
| 256 | 151 | 0.75% | 226 | 1692 | 256.0 | 256.0 | 640 | 0.20 | 0.77 | 25.34 | 100% | 100% |
| 320 | 154 | 0.77% | 239 | 1700 | 320.0 | 320.0 | 800 | 0.14 | 0.56 | 26.63 | 100% | 100% |

## Rate against quality

Item 5, from the baseline, since items 2 to 4 changed no setting: where
the proxies say the stereo stream stops improving, which is what zdesk's
ceiling of 320 kbit/s (`AUDIO_BITRATE_KBPS`, chosen to match a Mac's
AAC-ELD) rests on. The means over the recordings, and what each step up
bought:

| Rate, kbit/s | LSD, dB | Step | Side LSD, dB | Step | SegSNR, dB | Step |
|---:|---:|---:|---:|---:|---:|---:|
| 32 | 1.59 | | 6.92 | | 11.89 | |
| 48 | 1.28 | −0.30 | 6.03 | −0.89 | 13.88 | +1.99 |
| 64 | 0.98 | −0.30 | 4.92 | −1.10 | 15.22 | +1.33 |
| 96 | 0.52 | −0.46 | 3.39 | −1.54 | 18.50 | +3.28 |
| 128 | 0.39 | −0.13 | 2.07 | −1.32 | 20.70 | +2.20 |
| 160 | 0.32 | −0.07 | 1.35 | −0.72 | 22.15 | +1.45 |
| 192 | 0.27 | −0.05 | 1.08 | −0.27 | 23.33 | +1.18 |
| 256 | 0.20 | −0.07 | 0.77 | −0.31 | 25.34 | +2.01 |
| 320 | 0.14 | −0.05 | 0.56 | −0.21 | 26.68 | +1.34 |

### What it says

**The spectra stop improving much at 160 to 192 kbit/s; only the waveform
goes on.** Each channel's LSD gains under 0.1 dB a step from 128 kbit/s up. The
side's, the stereo image, gains 0.9 to 1.5 dB a step to 128 and 0.7 to 160,
then 0.2 to 0.3 a step to 320: the knee is at 160. SegSNR rises one to two
dB a step all the way, as a waveform measure does for any coder given more
bits; it ranks rates, it does not say what is heard. The recording that
goes on gaining is the one whose side is quiet and busy, the Raven (side
LSD 3.99 dB at 160, 3.43 at 192, 1.77 at 320); every other recording's
side is within 2.1 dB of the source's from 192 up, the indie rock's 0.83.

So by the proxies a stereo stream at 192 kbit/s is within 0.13 dB of
320's LSD and 0.52 dB of its side's, at six tenths of the link. That is a finding for zdesk, not a setting here:
the ceiling is zdesk's, and a listen decides it. The pairs to listen to are
every recording at 160, 192, 256 and 320 kbit/s, under
`2026-10-26/baseline/opus/crate-<kbit>/` on the Mac. The cost is no reason
either way: 139 µs a block at 160 kbit/s against 155 at 320. Whether a
lower ceiling helps the walk, which starts from it and has further to come
down from 320 on a link that narrows, is for the simulator (item 6).

## The baseline: the recordings together

### `crate`

15 recordings, 103 minutes of sound. A block is 20 ms; *of one core* is the mean block over
20 ms. The 99th-percentile and worst blocks are the worst recording's.
*Peak* is the most any second of any recording came to.

| Rate, kbit/s | µs a block | Of one core | 99th, µs | Worst, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 32 | 126 | 0.63% | 341 | 1440 | 31.9 | 35.5 | 123 | 1.59 | 6.92 | 11.89 | 69% | 83% |
| 48 | 126 | 0.63% | 344 | 2088 | 48.0 | 52.5 | 183 | 1.28 | 6.03 | 13.88 | 72% | 83% |
| 64 | 102 | 0.51% | 152 | 1304 | 64.3 | 65.7 | 242 | 0.98 | 4.92 | 15.22 | 100% | 83% |
| 96 | 116 | 0.58% | 177 | 1152 | 96.3 | 98.3 | 360 | 0.52 | 3.39 | 18.50 | 100% | 100% |
| 128 | 128 | 0.64% | 196 | 1150 | 128.3 | 131.0 | 481 | 0.39 | 2.07 | 20.70 | 100% | 100% |
| 160 | 139 | 0.70% | 215 | 1472 | 160.2 | 163.6 | 600 | 0.32 | 1.35 | 22.15 | 100% | 100% |
| 192 | 144 | 0.72% | 225 | 1304 | 192.2 | 196.2 | 722 | 0.27 | 1.08 | 23.33 | 100% | 100% |
| 256 | 150 | 0.75% | 225 | 1815 | 256.1 | 261.5 | 964 | 0.20 | 0.77 | 25.34 | 100% | 100% |
| 320 | 154 | 0.77% | 245 | 1403 | 320.0 | 326.8 | 1209 | 0.14 | 0.56 | 26.68 | 100% | 100% |

## The baseline: the recordings one by one, `crate`

#### µs a block, the least of the passes' mean

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 80.7 | 84.3 | 92.2 | 116 | 127 | 138 | 140 | 149 | 152 |
| Chopin, Fantasy Op. 49, piano | 739 | 82.9 | 85.7 | 92.0 | 110 | 119 | 129 | 132 | 139 | 143 |
| Shostakovich 5, orchestra, live | 324 | 94.5 | 100 | 106 | 117 | 131 | 146 | 157 | 176 | 179 |
| Indie rock, 2022, mastered | 224 | 92.9 | 98.9 | 108 | 125 | 139 | 151 | 158 | 163 | 164 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 95.3 | 102 | 110 | 126 | 140 | 157 | 160 | 168 | 174 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 92.2 | 97.1 | 104 | 120 | 131 | 140 | 145 | 152 | 156 |
| Limehouse Blues, 1924 78 rpm | 197 | 103 | 102 | 113 | 135 | 148 | 163 | 167 | 175 | 174 |
| Prison work song, 1947 field recording | 252 | 100 | 102 | 110 | 133 | 154 | 167 | 173 | 179 | 187 |
| The Raven, read, produced | 559 | 166 | 161 | 109 | 126 | 141 | 155 | 166 | 179 | 185 |
| Private Track11, talk over soft music | 182 | 111 | 99.3 | 107 | 122 | 134 | 146 | 154 | 164 | 168 |
| Leviticus 13, read (mono) | 726 | 192 | 197 | 104 | 116 | 129 | 140 | 144 | 146 | 150 |
| JFK at Rice, 1962 | 837 | 181 | 186 | 104 | 114 | 129 | 140 | 144 | 143 | 147 |
| Train station, field recording | 141 | 130 | 135 | 105 | 135 | 151 | 166 | 167 | 176 | 177 |
| Pink noise | 600 | 117 | 90.5 | 93.5 | 101 | 110 | 115 | 117 | 122 | 126 |
| White noise | 600 | 93.2 | 96.0 | 94.0 | 102 | 110 | 115 | 119 | 123 | 127 |

#### 99th-percentile block, µs

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 108 | 116 | 122 | 162 | 175 | 196 | 190 | 208 | 231 |
| Chopin, Fantasy Op. 49, piano | 739 | 111 | 112 | 122 | 158 | 170 | 192 | 194 | 199 | 207 |
| Shostakovich 5, orchestra, live | 324 | 191 | 147 | 136 | 149 | 167 | 179 | 190 | 208 | 223 |
| Indie rock, 2022, mastered | 224 | 109 | 121 | 135 | 161 | 184 | 202 | 211 | 221 | 211 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 124 | 137 | 143 | 151 | 174 | 199 | 209 | 216 | 214 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 119 | 125 | 131 | 169 | 180 | 186 | 195 | 214 | 212 |
| Limehouse Blues, 1924 78 rpm | 197 | 310 | 140 | 152 | 177 | 190 | 210 | 215 | 222 | 208 |
| Prison work song, 1947 field recording | 252 | 295 | 125 | 135 | 164 | 196 | 215 | 214 | 222 | 231 |
| The Raven, read, produced | 559 | 341 | 344 | 136 | 158 | 187 | 200 | 225 | 225 | 245 |
| Private Track11, talk over soft music | 182 | 322 | 128 | 140 | 162 | 192 | 192 | 212 | 205 | 209 |
| Leviticus 13, read (mono) | 726 | 236 | 251 | 134 | 156 | 172 | 188 | 176 | 195 | 206 |
| JFK at Rice, 1962 | 837 | 231 | 251 | 139 | 147 | 170 | 182 | 178 | 176 | 194 |
| Train station, field recording | 141 | 326 | 330 | 133 | 167 | 190 | 206 | 196 | 225 | 207 |
| Pink noise | 600 | 305 | 287 | 119 | 142 | 158 | 172 | 173 | 179 | 181 |
| White noise | 600 | 295 | 296 | 122 | 143 | 159 | 173 | 181 | 181 | 183 |

#### The rate the packets came to, kbit/s

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 32.3 | 48.2 | 64.4 | 96.3 | 128.3 | 160.3 | 192.2 | 256.2 | 320.2 |
| Chopin, Fantasy Op. 49, piano | 739 | 32.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Shostakovich 5, orchestra, live | 324 | 32.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Indie rock, 2022, mastered | 224 | 32.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 32.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 32.0 | 47.8 | 63.6 | 95.3 | 126.9 | 158.5 | 190.2 | 253.4 | 316.6 |
| Limehouse Blues, 1924 78 rpm | 197 | 32.2 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Prison work song, 1947 field recording | 252 | 32.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| The Raven, read, produced | 559 | 32.2 | 48.0 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Private Track11, talk over soft music | 182 | 32.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Leviticus 13, read (mono) | 726 | 31.9 | 47.9 | 64.1 | 95.9 | 127.7 | 159.6 | 191.4 | 255.1 | 318.7 |
| JFK at Rice, 1962 | 837 | 31.1 | 47.1 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Train station, field recording | 141 | 31.4 | 47.2 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| Pink noise | 600 | 31.2 | 48.3 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| White noise | 600 | 32.1 | 48.1 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |

#### The most any second came to, kbit/s

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 33.2 | 49.4 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Chopin, Fantasy Op. 49, piano | 739 | 33.0 | 49.4 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Shostakovich 5, orchestra, live | 324 | 33.4 | 49.6 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Indie rock, 2022, mastered | 224 | 33.9 | 50.9 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 34.3 | 50.9 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 35.5 | 52.5 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Limehouse Blues, 1924 78 rpm | 197 | 34.2 | 49.4 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Prison work song, 1947 field recording | 252 | 33.4 | 49.7 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| The Raven, read, produced | 559 | 35.1 | 51.6 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Private Track11, talk over soft music | 182 | 34.5 | 49.4 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Leviticus 13, read (mono) | 726 | 34.1 | 50.4 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| JFK at Rice, 1962 | 837 | 34.2 | 50.6 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| Train station, field recording | 141 | 33.0 | 49.4 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 324.8 |
| Pink noise | 600 | 32.6 | 48.7 | 64.5 | 97.1 | 129.3 | 161.5 | 193.5 | 257.4 | 321.3 |
| White noise | 600 | 32.6 | 48.8 | 64.9 | 97.0 | 129.3 | 161.4 | 193.3 | 257.2 | 321.1 |

#### Log-spectral distance, dB

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 1.78 | 1.41 | 0.99 | 0.56 | 0.44 | 0.36 | 0.31 | 0.23 | 0.18 |
| Chopin, Fantasy Op. 49, piano | 739 | 2.11 | 1.83 | 1.56 | 0.50 | 0.38 | 0.30 | 0.25 | 0.17 | 0.12 |
| Shostakovich 5, orchestra, live | 324 | 1.08 | 0.81 | 0.62 | 0.37 | 0.26 | 0.22 | 0.20 | 0.16 | 0.12 |
| Indie rock, 2022, mastered | 224 | 1.47 | 1.13 | 0.92 | 0.62 | 0.46 | 0.37 | 0.31 | 0.22 | 0.15 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 1.45 | 1.11 | 0.94 | 0.62 | 0.46 | 0.38 | 0.31 | 0.22 | 0.15 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 1.44 | 1.16 | 0.96 | 0.64 | 0.48 | 0.39 | 0.33 | 0.24 | 0.16 |
| Limehouse Blues, 1924 78 rpm | 197 | 1.42 | 1.17 | 0.99 | 0.68 | 0.54 | 0.45 | 0.39 | 0.30 | 0.23 |
| Prison work song, 1947 field recording | 252 | 1.19 | 0.80 | 0.66 | 0.44 | 0.34 | 0.29 | 0.25 | 0.21 | 0.16 |
| The Raven, read, produced | 559 | 1.68 | 1.24 | 0.78 | 0.48 | 0.36 | 0.30 | 0.26 | 0.20 | 0.15 |
| Private Track11, talk over soft music | 182 | 1.58 | 1.13 | 0.89 | 0.58 | 0.43 | 0.34 | 0.29 | 0.21 | 0.15 |
| Leviticus 13, read (mono) | 726 | 1.68 | 1.39 | 0.74 | 0.44 | 0.32 | 0.26 | 0.20 | 0.11 | 0.09 |
| JFK at Rice, 1962 | 837 | 1.79 | 1.52 | 0.69 | 0.38 | 0.27 | 0.22 | 0.18 | 0.09 | 0.07 |
| Train station, field recording | 141 | 2.63 | 2.58 | 2.42 | 0.42 | 0.34 | 0.28 | 0.23 | 0.18 | 0.12 |
| Pink noise | 600 | 1.34 | 0.98 | 0.81 | 0.55 | 0.41 | 0.35 | 0.29 | 0.22 | 0.16 |
| White noise | 600 | 1.16 | 0.99 | 0.79 | 0.53 | 0.40 | 0.33 | 0.28 | 0.20 | 0.14 |

#### Log-spectral distance of the side, dB

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 2.81 | 2.16 | 1.55 | 0.93 | 0.71 | 0.57 | 0.50 | 0.38 | 0.31 |
| Chopin, Fantasy Op. 49, piano | 739 | 3.45 | 2.57 | 1.83 | 1.14 | 0.77 | 0.33 | 0.27 | 0.18 | 0.13 |
| Shostakovich 5, orchestra, live | 324 | — | — | — | — | — | — | — | — | — |
| Indie rock, 2022, mastered | 224 | 8.42 | 7.67 | 6.08 | 3.87 | 2.10 | 1.02 | 0.83 | 0.62 | 0.50 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 7.38 | 6.61 | 5.41 | 3.45 | 2.40 | 1.66 | 1.37 | 1.09 | 0.91 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 7.77 | 7.18 | 5.92 | 3.34 | 1.92 | 0.86 | 0.63 | 0.44 | 0.33 |
| Limehouse Blues, 1924 78 rpm | 197 | 6.24 | 5.28 | 4.72 | 3.07 | 1.68 | 0.98 | 0.78 | 0.58 | 0.42 |
| Prison work song, 1947 field recording | 252 | 8.14 | 7.86 | 7.29 | 4.79 | 3.35 | 2.79 | 2.05 | 1.52 | 1.05 |
| The Raven, read, produced | 559 | 8.66 | 8.11 | 7.33 | 5.69 | 4.60 | 3.99 | 3.43 | 2.50 | 1.77 |
| Private Track11, talk over soft music | 182 | 7.07 | 6.71 | 5.96 | 4.48 | 3.28 | 2.28 | 1.62 | 0.83 | 0.49 |
| Leviticus 13, read (mono) | 726 | — | — | — | — | — | — | — | — | — |
| JFK at Rice, 1962 | 837 | — | — | — | — | — | — | — | — | — |
| Train station, field recording | 141 | 6.86 | 5.08 | 3.92 | 2.35 | 1.35 | 0.95 | 0.83 | 0.64 | 0.50 |
| Pink noise | 600 | 7.92 | 6.45 | 4.41 | 3.63 | 1.31 | 0.38 | 0.31 | 0.23 | 0.17 |
| White noise | 600 | 8.29 | 6.65 | 4.67 | 3.91 | 1.33 | 0.36 | 0.29 | 0.21 | 0.15 |

#### Segmental SNR, dB

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 17.24 | 20.53 | 22.51 | 25.86 | 27.67 | 29.17 | 30.21 | 32.10 | 33.70 |
| Chopin, Fantasy Op. 49, piano | 739 | 14.87 | 17.70 | 19.49 | 23.51 | 25.56 | 26.98 | 28.08 | 30.11 | 31.73 |
| Shostakovich 5, orchestra, live | 324 | 17.15 | 19.32 | 21.23 | 25.27 | 26.98 | 27.81 | 28.58 | 29.76 | 30.42 |
| Indie rock, 2022, mastered | 224 | 11.66 | 13.50 | 14.93 | 18.22 | 20.37 | 21.91 | 23.25 | 25.46 | 27.20 |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 11.51 | 13.67 | 15.02 | 18.53 | 20.95 | 22.62 | 24.07 | 26.55 | 28.69 |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 8.97 | 10.98 | 12.41 | 15.91 | 18.26 | 20.03 | 21.73 | 24.70 | 27.28 |
| Limehouse Blues, 1924 78 rpm | 197 | 3.51 | 4.76 | 5.76 | 7.11 | 9.81 | 11.67 | 12.99 | 14.80 | 15.67 |
| Prison work song, 1947 field recording | 252 | 15.68 | 18.04 | 19.62 | 23.40 | 25.38 | 26.44 | 27.22 | 28.56 | 30.37 |
| The Raven, read, produced | 559 | 18.84 | 20.55 | 21.08 | 24.24 | 25.66 | 26.45 | 27.07 | 28.04 | 28.88 |
| Private Track11, talk over soft music | 182 | 14.21 | 16.74 | 18.22 | 22.04 | 24.43 | 25.83 | 26.92 | 28.75 | 30.31 |
| Leviticus 13, read (mono) | 726 | 14.88 | 16.44 | 16.77 | 21.00 | 24.23 | 26.28 | 28.16 | 31.38 | 32.47 |
| JFK at Rice, 1962 | 837 | 14.96 | 17.22 | 18.27 | 23.06 | 25.84 | 27.70 | 29.12 | 32.99 | 34.07 |
| Train station, field recording | 141 | 13.77 | 15.50 | 17.37 | 21.03 | 22.44 | 23.37 | 24.33 | 25.88 | 27.18 |
| Pink noise | 600 | 2.64 | 4.00 | 5.26 | 7.06 | 9.39 | 11.06 | 12.33 | 13.97 | 14.70 |
| White noise | 600 | -1.52 | -0.69 | 0.30 | 1.19 | 3.49 | 4.95 | 5.93 | 7.08 | 7.54 |

#### Packets that are CELT alone

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 99% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Chopin, Fantasy Op. 49, piano | 739 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Shostakovich 5, orchestra, live | 324 | 99% | 99% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Indie rock, 2022, mastered | 224 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Limehouse Blues, 1924 78 rpm | 197 | 96% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Prison work song, 1947 field recording | 252 | 98% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| The Raven, read, produced | 559 | 57% | 64% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Private Track11, talk over soft music | 182 | 92% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Leviticus 13, read (mono) | 726 | 0% | 0% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| JFK at Rice, 1962 | 837 | 9% | 10% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Train station, field recording | 141 | 80% | 81% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Pink noise | 600 | 84% | 99% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| White noise | 600 | 96% | 97% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |

#### Packets that are fullband

| Recording | Seconds | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Bach, Goldberg Aria, piano | 300 | 2% | 2% | 2% | 100% | 100% | 100% | 100% | 100% | 100% |
| Chopin, Fantasy Op. 49, piano | 739 | 22% | 22% | 22% | 100% | 100% | 100% | 100% | 100% | 100% |
| Shostakovich 5, orchestra, live | 324 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Indie rock, 2022, mastered | 224 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Private Track10, mix at −13 LUFS, wide stereo | 250 | 99% | 99% | 99% | 100% | 100% | 100% | 100% | 100% | 100% |
| Private Track_1, mix at −9 LUFS, loud master | 225 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Limehouse Blues, 1924 78 rpm | 197 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| Prison work song, 1947 field recording | 252 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| The Raven, read, produced | 559 | 96% | 96% | 96% | 100% | 100% | 100% | 100% | 100% | 100% |
| Private Track11, talk over soft music | 182 | 98% | 98% | 98% | 100% | 100% | 100% | 100% | 100% | 100% |
| Leviticus 13, read (mono) | 726 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| JFK at Rice, 1962 | 837 | 98% | 98% | 98% | 100% | 100% | 100% | 100% | 100% | 100% |
| Train station, field recording | 141 | 14% | 14% | 14% | 100% | 100% | 100% | 100% | 100% | 100% |
| Pink noise | 600 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| White noise | 600 | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |

## The baseline: the made sounds, `crate`

Fifty blocks of each; the time is one block's, the least of five passes.

#### µs a block, the least of the passes' mean

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 313 | 100 | 113 | 138 | 160 | 168 | 169 | 177 | 180 |
| speech | 189 | 194 | 99.9 | 110 | 124 | 134 | 140 | 140 | 144 |
| tone | 110 | 111 | 69.5 | 79.2 | 90.2 | 99.1 | 107 | 123 | 138 |
| noise | 292 | 299 | 92.0 | 98.8 | 106 | 111 | 113 | 119 | 123 |
| silence | 39.5 | 39.6 | 39.6 | 39.4 | 40.0 | 39.7 | 39.8 | 39.4 | 39.5 |

#### 99th-percentile block, µs

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 334 | 112 | 123 | 147 | 179 | 179 | 179 | 189 | 199 |
| speech | 272 | 278 | 108 | 123 | 136 | 145 | 149 | 149 | 153 |
| tone | 214 | 217 | 96.7 | 112 | 128 | 144 | 148 | 162 | 168 |
| noise | 312 | 314 | 109 | 130 | 146 | 157 | 160 | 164 | 168 |
| silence | 45.3 | 45.2 | 45.4 | 44.4 | 45.5 | 45.8 | 45.1 | 44.9 | 44.5 |

#### The rate the packets came to, kbit/s

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 34.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.5 | 320.4 |
| speech | 30.8 | 47.1 | 64.8 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| tone | 31.2 | 45.5 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| noise | 26.2 | 41.7 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| silence | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 |

#### The most any second came to, kbit/s

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 34.4 | 48.4 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.5 | 320.4 |
| speech | 30.8 | 47.1 | 64.8 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| tone | 31.2 | 45.5 | 65.7 | 98.3 | 131.0 | 163.6 | 196.2 | 261.5 | 326.8 |
| noise | 26.2 | 41.7 | 64.4 | 96.4 | 128.4 | 160.4 | 192.4 | 256.4 | 320.4 |
| silence | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 | 1.2 |

#### Log-spectral distance, dB

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 7.52 | 1.13 | 0.93 | 0.61 | 0.51 | 0.43 | 0.38 | 0.27 | 0.22 |
| speech | 2.11 | 1.70 | 0.92 | 0.58 | 0.44 | 0.37 | 0.33 | 0.22 | 0.20 |
| tone | 13.29 | 11.35 | 2.68 | 1.38 | 1.25 | 1.08 | 1.00 | 0.97 | 0.89 |
| noise | 2.04 | 2.50 | 0.78 | 0.55 | 0.39 | 0.34 | 0.28 | 0.21 | 0.14 |
| silence | — | — | — | — | — | — | — | — | — |

#### Log-spectral distance of the side, dB

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 7.43 | 8.43 | 7.16 | 6.19 | 5.58 | 4.01 | 3.77 | 3.52 | 3.17 |
| speech | — | — | — | — | — | — | — | — | — |
| tone | 9.03 | 7.60 | 4.08 | 2.46 | 2.44 | 2.21 | 1.34 | 0.95 | 0.84 |
| noise | 8.71 | 5.80 | 5.39 | 5.12 | 1.46 | 0.35 | 0.30 | 0.21 | 0.15 |
| silence | — | — | — | — | — | — | — | — | — |

#### Segmental SNR, dB

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 16.62 | 18.82 | 19.89 | 23.02 | 25.09 | 26.16 | 26.89 | 27.81 | 28.85 |
| speech | 17.68 | 18.90 | 18.03 | 20.70 | 23.18 | 24.41 | 25.17 | 26.34 | 26.49 |
| tone | 26.52 | 27.18 | 40.46 | 39.82 | 39.54 | 40.17 | 43.89 | 44.05 | 43.86 |
| noise | -1.63 | -1.07 | 0.20 | 1.08 | 3.31 | 4.89 | 5.88 | 7.03 | 7.50 |
| silence | — | — | — | — | — | — | — | — | — |

#### Packets that are CELT alone

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 0% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| speech | 0% | 0% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| tone | 60% | 62% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| noise | 0% | 0% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| silence | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |

#### Packets that are fullband

| Sound | 32 | 48 | 64 | 96 | 128 | 160 | 192 | 256 | 320 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| music | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| speech | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| tone | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| noise | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |
| silence | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% | 100% |

