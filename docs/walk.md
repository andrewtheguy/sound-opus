# The walk

How `walk::BitrateWalk` was settled for its two users, the zdesk gateway and
wlshare through it: what its sound goes through, measured where it could be
and modelled where it could not; how the walk as released (v0.0.4) did on
that; what it hears now and why; and what each constant was measured
against. Every figure here is from a log under `bench/`, and every log can
be made again:

    sudo python3 -I tools/socket-probe.py 200 40 100 cubic "4096 16384 4194304" 7440 186 20
    cargo build --release --example walk-sim
    tools/walk-cases.sh > walk.jsonl
    tools/walk-sweep.sh OUT
    python3 -I tools/walk.py walk.jsonl

The probes ran on lab2, alone (`benchmarks.md` has the machine). The
simulation is deterministic and runs in its own time, so how busy a machine
is changes nothing, but libopus codes a little differently on ARM and on
x86: the logs here are an Apple-silicon Mac's. lab2 ran the sweep of every
constant too (`bench/walk-sweep-lab2.jsonl` beside `bench/walk-sweep.jsonl`,
2,494 runs each): 2,073 lines are the same in every figure and in the
rate's path, and of the 421 that differ, 416 are the speech's, nearly all
at the floors, by at most 6 kbit/s, 110 ms at the 95th percentile, 0.9 s of
sound and 0.7 s stalled in a run; the walk as it is differs on 10 of its 58
runs, by at most 0.7 kbit/s, 9 ms and 0.2 s. Every variant's figure against
the base reads the same on both machines to a point of a per cent, so the
constants' table below is either machine's.

## What the sound goes through

`examples/walk-sim.rs` plays a recording as zdesk plays a desktop's sound,
in virtual time:

- **The source.** An RDP host's wave buffers, 8192 frames at 44.1 kHz,
  about 186 ms, coded by the gateway's encoder, nine or ten packets a
  buffer; or wlshare's 20 ms units, coded by wlshare at the rate the
  gateway last named, over wlshare's own connection to the gateway, which
  carries the pictures too.
- **The gateway.** zdesk's broadcast queue, sixteen deep, dropping its
  oldest; for RDP the encoder behind it, with silence shed while the walk
  says the link is behind; for wlshare every queued unit batched into one
  send. A channel two deep to the socket's writer, one batch a WebSocket
  message in a TLS record.
- **The socket and the link.** A TCP connection to a bottleneck a hop away,
  modelled on the probes below: the congestion window is what the
  bottleneck and its buffer hold, the socket takes 85 KB unsent and
  unacknowledged before a write blocks, and wakes a blocked writer at two
  thirds of that. The link's rate is the sound's share of it, and can change
  at given times; a round trip of 40 ms, jitter on top at random, and a
  delay another stream's queue adds.
- **The browser.** Each packet played back to back from when its message
  arrives, an underrun a stall, any lead past 300 ms trimmed, as zdesk's
  page does.

What a run says: the rate the sound was coded at, how late each packet's
sound was heard after it was made (the RDP buffer's 186 ms is in it), what
was not heard — dropped by the broadcast queue, skipped by the walk,
trimmed by the browser — the silence shed, the stalls in sound that should
have run on, and how often the rate moved.

The cases, each with both sources over a minute of music (*Stay Awhile*,
the indie rock of the bench) and a minute of speech (Leviticus 13) with four
seconds of digital silence in every ten, a player paused — recorded speech
has almost none of its own, 3.7 s in Leviticus's twelve minutes and none in
the other two, and silence is the one thing the gateway sheds:

| Case | The sound's link |
|---|---|
| room | 1000 kbit/s throughout |
| room-jitter, room-jitter100 | the same, up to 60 or 100 ms more on any message, as a Wi-Fi hop adds |
| narrow, narrow-jitter | 1000, then 200 from 10 s; and with 60 ms of jitter |
| narrower | 1000, then 100 from 10 s |
| recover | 1000, 150 from 10 s, 1000 again from 25 s |
| wobble | swinging between 450 and 250 every 3 s |
| dips | 1000, but 60 for half a second every ten: a picture's keyframe on the same bottleneck |
| start-low | 150 from the start |
| between-floors | 1000, then 28 from 10 s |
| under-floor | 1000, then 16 from 10 s |
| path-delay | 1000, with another stream's queue adding 300 ms from 10 s to 40 s |
| hop-burst | wlshare only: 600 KB of picture on its 20 Mbit/s connection to the gateway every 10 s |

## What a socket holds

The walk's signal was how long a send blocked, so what matters is how much
a socket takes before one does. `tools/socket-probe.py` measures it on
Linux: a sender, a router and a browser in network namespaces, the
bottleneck on the router's way to the browser so the sender's TCP sees it
only through its acks, and the sender writing 7440 bytes every 186 ms, an
RDP buffer at 320 kbit/s, one write at a time as zdesk does
(`bench/lab2-socket.jsonl`). Linux's own settings (CUBIC, `tcp_wmem` 4096
16384 4194304) and lab2's (BBR, buffers to 32 MB), with and without
`TCP_NOTSENT_LOWAT`, a bottleneck buffer of 100 and 500 ms:

| Link, kbit/s | Queue, ms | Congestion control | tcp_wmem | Low-water mark | First block, s | Longest, ms | Blocks | Late p50, ms | Late max, ms | Held p50, bytes | Unsent p50, bytes |
|---:|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| 200 | 100 | cubic | 4096 16384 4194304 | — | 5.2 | 1643 | 5 of 72 | 3281 | 3872 | 76312 | 72112 |
| 200 | 100 | cubic | 4096 16384 4194304 | 16384 | 2.2 | 1317 | 7 of 67 | 1396 | 1924 | 30272 | 27024 |
| 200 | 100 | cubic | 4096 16384 4194304 | 1 | 2.2 | 1697 | 6 of 69 | 1175 | 1876 | 27264 | 22080 |
| 200 | 100 | bbr | 4096 87380 33554432 | — | 8.0 | 2374 | 3 of 75 | 4787 | 5910 | 112912 | 108568 |
| 200 | 100 | bbr | 4096 87380 33554432 | 16384 | 2.2 | 1358 | 7 of 66 | 1409 | 1830 | 31920 | 26480 |
| 200 | 100 | bbr | 4096 87380 33554432 | 1 | 2.0 | 1739 | 5 of 69 | 1096 | 1857 | 26328 | 21032 |
| 200 | 500 | cubic | 4096 16384 4194304 | — | 4.5 | 1651 | 6 of 74 | 3265 | 3704 | 67872 | 53392 |
| 200 | 500 | cubic | 4096 16384 4194304 | 16384 | 4.5 | 2440 | 3 of 72 | 2282 | 3561 | 53144 | 35568 |
| 200 | 500 | cubic | 4096 16384 4194304 | 1 | 2.8 | 2446 | 4 of 70 | 1975 | 2960 | 45048 | 30368 |
| 200 | 500 | bbr | 4096 87380 33554432 | — | 7.6 | 1946 | 3 of 85 | 5769 | 7050 | 134880 | 123200 |
| 200 | 500 | bbr | 4096 87380 33554432 | 16384 | 2.8 | 2422 | 4 of 68 | 2268 | 3159 | 52088 | 43200 |
| 200 | 500 | bbr | 4096 87380 33554432 | 1 | 3.2 | 2858 | 4 of 74 | 2047 | 3230 | 47944 | 37808 |
| 1000 | 100 | cubic | 4096 16384 4194304 | — | never | 0 | 0 of 108 | 50 | 50 | 7440 | 0 |
| 1000 | 100 | bbr | 4096 87380 33554432 | — | never | 0 | 0 of 108 | 50 | 50 | 7440 | 4544 |
| 1000 | 500 | cubic | 4096 16384 4194304 | — | never | 0 | 0 of 108 | 50 | 50 | 7440 | 0 |
| 1000 | 500 | bbr | 4096 87380 33554432 | — | never | 0 | 0 of 108 | 50 | 50 | 7440 | 4544 |

What it says:

- On a link with room nothing ever blocks, and the socket holds one write.
- On a link too narrow, Linux holds 68 to 76 KB before a write blocks —
  almost all of it unsent, in the socket, not on the wire — and BBR with
  large buffers 113 to 135 KB: 4.5 to 8 seconds before the first block, by
  which time the sound is 3.3 to 5.8 s late.
- When a write does block, it blocks for 1.6 to 2.4 s at once: Linux wakes
  a writer only once a third of the buffer is free.
- `TCP_NOTSENT_LOWAT` halves what is held, and the first block comes at 2 to
  4.5 s, but the blocks are as long, 1.3 to 2.9 s: on this kernel a write is
  still taken while the unsent bytes are past the mark.
- Behind a reverse proxy the socket is a loopback one to the proxy, whose
  own buffers come first; it can only hold more.

So blocking is a late and lumpy signal wherever the gateway runs, and
nothing a socket option does changes that. The simulation's socket is the
first row: with the rate held over the same link,

    walk-sim PinkNoise.wav --secs 20 --link 200 --held --timeline t.csv

its first block comes at 5.2 s, the longest is 1.2 s, and the sound is
3.1 s late, against the probe's 5.2 s, 1.6 s and 3.3 s.

## The walk as released

v0.0.4's walk heard each batch's send: blocked 20 ms or more was behind,
two of the last four a verdict (`bench/walk-v0.0.4.jsonl`, the left column
below). These runs, and the walk's against them below, are at the ceiling
zdesk had then, 320 kbit/s. On these cases:

- A link narrowed from 1000 to 200 kbit/s was first stepped down 10.7 s
  after, on music over RDP, and in the three other runs never: the sound was
  1.3 to 3.2 s late at the median and 3.2 to 5.1 s at the 95th percentile,
  with 8.5 to 18 s of stalls in the minute.
- On wlshare's stream the walk never moved, in any case. Its sends come
  fifty a second, one unit each, and a blocked one is one in a run of clear
  ones: never two of four. The broadcast queue, 320 ms of units, dropped
  up to 45 s of sound a minute instead, which a passed stream does not tell
  the walk.
- Under the floor the sound was 40 to 73 s late at the 95th percentile.
- Where the link stayed narrow, the walk climbed back to the ceiling while
  the socket drained and blocked nothing, and fell again once it filled:
  320 at 36 s and 94 at 45 s, on music over RDP.

## What the walk hears now

**The lag, by a ping.** zdesk's audio socket is already pinged for its
heartbeat, and a browser answers a WebSocket ping as it reads it, behind
everything written before it. So a ping written ahead of a batch comes back
once the sound before it has crossed, and its round trip over the least any
ping took is how long the newest sound waits for the link: the queue in the
socket, at the bottleneck and in a proxy's buffers alike, in time. One is in
flight at a time, written ahead of a batch when none is, so on RDP one a
buffer and on wlshare one every round trip. A send that blocked is heard
too, as the longer of the two at the next pong: a proxy that answered pings
itself would leave that.

**A ping still out is heard.** A queue growing faster than pongs come back
delays its own pongs: one ping at a time behind 30 seconds of sound is one
hearing in 30 seconds. So a ping out more than 100 ms past the least round
trip is heard as lagging by that much at least, and again every 100 ms
while it is out ([`OVERDUE`](../src/walk.rs)).

**Skipping.** The rate drains a queue only by as much as it is under the
link, and a queue built in the second before the walk could step down took
seconds to drain at the rate it stepped to. A lag past 200 ms is sound that
would be heard that late, and the walk skips it: the gateway drops what
comes next before the encoder (a passed unit before the socket, with the
gap the page is already told of) and writes pings back to back, until one
comes back under 100 ms. What is heard comes back to live with a gap in it,
rather than staying late.

**The path's lag.** A ping measures every queue on the way, and another
stream's — a download on the same home link, the picture's own — no skip or
rate of the sound's drains. While skipping, nothing of the sound is written
between two pings, so a queue of its own drains between them by the time
between them; one that did not drain by half that is the path's, and the
round trip it took becomes the least, measured over from then. Without
that, a standing 300 ms queue of someone else's would have the walk skip
the sound for as long as it stood. A lag under the 200 ms that skips is
told apart at the floor, where a link still behind is skipped whatever its
lag (below).

**The thresholds.** Lag is noisier than blocking, which a link with room
never does: 100 ms behind and 50 ms clear, above a Wi-Fi hop's jitter
(below). Everything else is as it was: the steps, the cooldowns, the clear
span, the refusal hold, each swept again on the new signal and kept.

On the same cases (`bench/walk-lag.jsonl`, the right column):

- A link narrowed to 200 kbit/s is stepped down 0.5 to 0.6 s after, and the
  rate then moves between 124 and 282 kbit/s, probing past the link and
  walked back; the sound is heard 0.5 to 0.6 s late at the 95th percentile
  over RDP and 0.2 s over wlshare, with 0.5 to 1.8 s of stalls, against 3.2
  to 5.1 s and 8.5 to 18 s.
- wlshare's stream walks like RDP's, and the broadcast queue drops nothing
  in any case: a passed stream's lag reaching the walk is not needed,
  and `--passed-lag` measures the same (`bench/walk-passed-lag.jsonl`).
- Under the floor the sound is 0.7 to 1.1 s late at the 95th percentile
  rather than 40 to 73: what the link cannot carry is skipped, 21 to 33 s of
  the minute, where it was dropped by the queue after being heard late.
- Another stream's 300 ms costs 0.7 to 0.8 s skipped once and the rate
  289 to 291 kbit/s; the 300 ms itself is the path's to add.
- A link with up to 100 ms of jitter and room to spare keeps 320 kbit/s.
- What it costs: half-second dips every ten seconds read as narrowings, and
  the rate sits at 163 to 254 kbit/s for stalls no shorter than v0.0.4's,
  which held 320.

Over every case the 95th-percentile lateness summed falls from 433 s to
21 s, the sound lost from 490 s to 190 s (skipped, now, under the floors),
and the stalls from 871 s to 164 s, with the floor at 24 kbit/s (below).
With zdesk's ceiling at 192 kbit/s and a passed stream batched at low
rates (`bench/walk-batched.jsonl`), 20 s, 133 s and 105 s; with a link
still behind at the floor skipped too, as v0.0.5 had it
(`bench/walk-v0.0.5.jsonl`), 19 s, 143 s and 99 s; and with the round after
wlshare's walk below (`bench/walk.jsonl`), 19 s, 138 s and 92 s. Those two
logs have the longer route's four runs as well, which the earlier logs do
not; the sums here leave them out, so every sum is over the same 54 runs,
and the round's own table below is over all 58.

| case | source | sound | walk-v0.0.4 | walk-lag |
|---|---|---|---|---|
| | | | kbit/s, late p95/max ms, lost s, stalled s | kbit/s, late p95/max ms, lost s, stalled s |
| room | rdp | music | 320, 295/295, 0.0, 0.0 | 320, 295/295, 0.0, 0.0 |
| room | rdp | speech | 320, 296/296, 0.0, 0.1 | 320, 296/296, 0.0, 0.1 |
| room | wlshare | music | 320, 51/51, 0.0, 0.0 | 320, 51/51, 0.0, 0.0 |
| room | wlshare | speech | 320, 51/51, 0.0, 0.0 | 320, 51/51, 0.0, 0.0 |
| room-jitter | rdp | music | 320, 352/352, 0.0, 0.1 | 320, 350/350, 0.0, 0.0 |
| room-jitter | rdp | speech | 320, 350/350, 0.0, 0.1 | 320, 351/351, 0.0, 0.1 |
| room-jitter | wlshare | music | 320, 109/110, 0.0, 0.0 | 320, 109/109, 0.0, 0.0 |
| room-jitter | wlshare | speech | 320, 109/109, 0.0, 0.0 | 320, 109/109, 0.0, 0.0 |
| room-jitter100 | rdp | music | 320, 391/391, 0.0, 0.1 | 320, 390/390, 0.0, 0.0 |
| room-jitter100 | rdp | speech | 320, 390/390, 0.0, 0.3 | 320, 391/391, 0.0, 0.2 |
| room-jitter100 | wlshare | music | 320, 149/149, 0.0, 0.1 | 320, 149/149, 0.0, 0.0 |
| room-jitter100 | wlshare | speech | 320, 148/148, 0.0, 0.1 | 320, 148/148, 0.0, 0.0 |
| narrow-jitter | rdp | music | 214, 5153/5640, 0.0, 10.7 | 206, 545/715, 0.6, 0.6 |
| narrow-jitter | rdp | speech | 320, 3941/4221, 0.0, 18.4 | 204, 519/785, 0.9, 1.3 |
| narrow-jitter | wlshare | music | 320, 3414/3843, 17.6, 9.0 | 197, 228/452, 0.5, 0.5 |
| narrow-jitter | wlshare | speech | 320, 3254/3497, 3.6, 15.9 | 198, 202/318, 0.7, 0.5 |
| narrow | rdp | music | 214, 5145/5606, 0.0, 10.6 | 203, 590/865, 1.5, 1.6 |
| narrow | rdp | speech | 320, 3909/4172, 0.0, 18.4 | 210, 545/800, 1.3, 1.8 |
| narrow | wlshare | music | 320, 3376/3830, 17.6, 8.5 | 200, 182/403, 0.4, 0.5 |
| narrow | wlshare | speech | 320, 3207/3446, 3.6, 15.8 | 196, 199/397, 1.1, 1.0 |
| narrower | rdp | music | 162, 9578/11697, 5.9, 13.3 | 130, 566/1693, 1.9, 1.9 |
| narrower | rdp | speech | 257, 11064/11768, 9.8, 30.6 | 152, 648/1617, 3.3, 3.0 |
| narrower | wlshare | music | 320, 6756/6939, 33.1, 20.9 | 122, 229/1135, 2.0, 1.9 |
| narrower | wlshare | speech | 320, 6469/7029, 18.6, 28.7 | 121, 253/1092, 2.4, 2.1 |
| recover | rdp | music | 255, 6569/7854, 0.7, 7.6 | 219, 529/1079, 0.7, 0.9 |
| recover | rdp | speech | 294, 4482/7286, 0.0, 7.9 | 253, 461/1023, 0.7, 1.1 |
| recover | wlshare | music | 320, 4176/4618, 6.5, 5.7 | 221, 181/523, 0.6, 0.6 |
| recover | wlshare | speech | 320, 3127/4619, 3.2, 7.1 | 220, 170/546, 0.5, 0.6 |
| wobble | rdp | music | 320, 1055/1074, 0.0, 6.0 | 242, 606/698, 1.1, 1.4 |
| wobble | rdp | speech | 320, 1052/1066, 0.0, 4.5 | 237, 557/641, 0.2, 1.3 |
| wobble | wlshare | music | 320, 812/822, 0.0, 4.8 | 232, 239/239, 0.0, 0.2 |
| wobble | wlshare | speech | 320, 807/825, 0.0, 2.9 | 242, 246/276, 0.3, 0.4 |
| dips | rdp | music | 320, 571/708, 0.0, 1.5 | 163, 418/708, 0.4, 1.5 |
| dips | rdp | speech | 320, 493/659, 0.0, 1.5 | 200, 430/606, 0.0, 1.4 |
| dips | wlshare | music | 320, 391/465, 0.0, 0.9 | 228, 301/462, 0.9, 1.2 |
| dips | wlshare | speech | 320, 383/464, 0.0, 0.9 | 254, 303/465, 1.4, 1.5 |
| start-low | rdp | music | 172, 7582/8639, 3.2, 18.1 | 140, 567/1049, 0.7, 0.7 |
| start-low | rdp | speech | 219, 7042/7735, 0.6, 21.4 | 151, 596/1054, 1.5, 2.3 |
| start-low | wlshare | music | 320, 4555/5137, 30.5, 16.9 | 126, 197/652, 0.7, 0.7 |
| start-low | wlshare | speech | 320, 4341/4628, 12.0, 26.2 | 136, 258/609, 1.1, 1.1 |
| between-floors | rdp | music | 251, 31514/34903, 38.5, 60.9 | 84, 616/6430, 8.0, 7.7 |
| between-floors | rdp | speech | 236, 28204/31822, 31.6, 53.4 | 115, 559/5984, 6.3, 6.8 |
| between-floors | wlshare | music | 320, 23549/25018, 43.9, 44.6 | 80, 411/3465, 19.4, 15.1 |
| between-floors | wlshare | speech | 320, 22564/25087, 37.9, 53.8 | 80, 349/3350, 12.8, 10.0 |
| under-floor | rdp | music | 289, 73274/77026, 42.0, 104.5 | 126, 1084/11365, 29.9, 21.8 |
| under-floor | rdp | speech | 320, 54356/58399, 38.8, 82.3 | 152, 1000/8316, 20.6, 14.9 |
| under-floor | wlshare | music | 320, 41716/44570, 45.5, 64.2 | 79, 718/6244, 33.4, 28.0 |
| under-floor | wlshare | speech | 320, 40120/43998, 45.1, 70.2 | 79, 661/6028, 29.5, 23.8 |
| path-delay | rdp | music | 320, 595/595, 0.0, 0.3 | 291, 595/595, 0.7, 0.4 |
| path-delay | rdp | speech | 320, 595/595, 0.0, 0.4 | 291, 593/593, 0.7, 0.4 |
| path-delay | wlshare | music | 320, 351/351, 0.0, 0.3 | 289, 351/351, 0.8, 0.3 |
| path-delay | wlshare | speech | 320, 351/351, 0.0, 0.3 | 289, 351/351, 0.8, 0.3 |
| hop-burst | wlshare | music | 320, 289/289, 0.0, 0.2 | 320, 289/289, 0.0, 0.2 |
| hop-burst | wlshare | speech | 320, 287/287, 0.0, 0.2 | 320, 288/288, 0.0, 0.2 |

| log | kbit/s mean | late p95 sum | late max sum | lost s | stalled s | moves |
|---|---:|---:|---:|---:|---:|---:|
| walk-v0.0.4 | 302 | 433160 | 474270 | 489.8 | 871.2 | 58 |
| walk-lag | 219 | 21320 | 77241 | 190.4 | 164.4 | 478 |

## The ceiling

zdesk's ceiling was 320 kbit/s, the rate of a Mac's own AAC-ELD.
`benchmarks.md` found a stereo stream within 0.13 dB of it per channel at
192 by the proxies, and left whether a lower ceiling helps the walk to here.
It does: on the same cases with the ceiling at 192
(`bench/walk-ceiling192.jsonl`, against `bench/walk-lag.jsonl`), 16% less
sound lost, 19% less stalled, the worst lateness summed 48 s against 77, and
27% fewer moves; a link swinging between 250 and 450 kbit/s is never walked
at all. The rate is lower, 153 kbit/s on average against 219, most of it
the links with room held at 192 rather than 320, and under the dips 103 to
151 against 163 to 254. zdesk's ceiling is 192 kbit/s now, and every run
from here is at it.

## A passed stream's units, batched

Every unit of a passed stream was a WebSocket message of its own in a TLS
record of its own: about 30 bytes of framing on a packet of 60 at 24 kbit/s,
so the floor cost 36 kbit/s of the link, where an RDP buffer's nine packets
in one message cost 25, and between the floors wlshare's sound lost 13 to
18 s of the minute against RDP's 4 to 9. zdesk now holds a passed stream's
units while the walk's rate is 64 kbit/s or less, until a message carries
three of them, 60 ms (`walk-sim --batch-below 64,60`, the default).

The rule is the rate's, not the bytes': held until the units came to so
many bytes, a paused desktop's 3-byte units of silence were held at every
rate, and the browser, which plays from when sound arrives, kept the delay
on links with room too. Held on the rate, nothing changes above it. The
thresholds and holds tried (`bench/walk-batch.jsonl`), 48, 64 and
96 kbit/s by 40, 60 and 100 ms: 48 and 64 measure the same, 96 begins to
hold where a link narrowed; 40 ms cut what wlshare lost at the floors by
21%, 60 by 30% and 100 by 39%, for 48, 77 and 162 ms more median lateness
there. Against each send taking what is queued
(`bench/walk-batched.jsonl` against `bench/walk-ceiling192.jsonl`):

| case | source | sound | unbatched | batched |
|---|---|---|---|---|
| | | | kbit/s, late p95/max ms, lost s, stalled s | kbit/s, late p95/max ms, lost s, stalled s |
| between-floors | wlshare | music | 55, 405/2040, 18.2, 13.7 | 55, 352/2040, 8.6, 5.8 |
| between-floors | wlshare | speech | 56, 378/1979, 13.0, 10.1 | 57, 363/1979, 7.0, 5.5 |
| under-floor | wlshare | music | 55, 718/3750, 32.4, 26.8 | 55, 634/3750, 28.9, 20.2 |
| under-floor | wlshare | speech | 55, 649/3628, 28.4, 22.6 | 55, 624/3628, 20.3, 14.6 |

Between the floors wlshare's sound now loses 7 to 9 s, as RDP's does, and
is heard no later at the 95th percentile, 0.35 to 0.36 s against 0.38 to
0.41; over every case 17% less sound is lost and 21% less stalled. Where
the walk touches 64 kbit/s on its way elsewhere, on the narrowest link and
under the dips, nothing moves by more than 6 ms of lateness.

## A lag that stands at the floor

A route 150 ms longer from ten seconds on (`path-longer`) is behind on
every pong but short of the 200 ms that skips, and only skipping tells a
lag of the path's apart: the walk gave rate up to the floor and held it
there for the rest of the minute, 54 to 68 kbit/s on average. At the floor
no rate is left to drain a lag, so a link still behind there — a verdict,
two hearings behind among four a cooldown after the last move — is now
skipped whatever its lag; one that drains was the sound's, and one that
does not is the path's, measured over from then. With it, against without
(`bench/walk-sweep.jsonl`, `base` against `noprobe`, on the walk as it is
after the round below), the cases that moved:

| case | source | sound | without | with |
|---|---|---|---|---|
| | | | kbit/s, late p95/max ms, lost s, stalled s | kbit/s, late p95/max ms, lost s, stalled s |
| between-floors | rdp | music | 56, 583/3825, 4.5, 4.5 | 56, 583/3825, 5.0, 4.5 |
| between-floors | wlshare | music | 54, 357/2162, 8.5, 5.8 | 54, 263/2162, 10.7, 5.9 |
| between-floors | wlshare | speech | 55, 355/2359, 5.8, 4.5 | 55, 313/2359, 6.8, 4.7 |
| under-floor | rdp | music | 78, 1084/6805, 27.7, 18.5 | 84, 686/6805, 30.5, 13.9 |
| under-floor | rdp | speech | 94, 1054/4974, 17.6, 11.9 | 103, 768/4974, 19.3, 9.2 |
| under-floor | wlshare | music | 54, 684/3978, 28.3, 21.1 | 54, 423/3978, 29.9, 17.4 |
| under-floor | wlshare | speech | 54, 644/4351, 19.5, 14.7 | 54, 453/4351, 22.0, 13.6 |
| path-longer | rdp | music | 54, 405/405, 0.0, 0.2 | 165, 417/417, 0.4, 0.2 |
| path-longer | rdp | speech | 68, 404/404, 0.0, 0.2 | 159, 418/418, 0.4, 0.2 |
| path-longer | wlshare | music | 54, 233/251, 0.0, 0.2 | 165, 235/235, 0.4, 0.2 |
| path-longer | wlshare | speech | 54, 234/251, 0.0, 0.2 | 160, 236/236, 0.4, 0.2 |

The rate is back at the ceiling within half a minute of the route growing,
for 0.4 s skipped once, 159 to 165 kbit/s over the minute against 54 to
68. Under the floors the sound is skipped from 100 ms rather than 200: 0.5
to 2.8 s more of the minute is lost there, heard 0.1 to 0.4 s sooner at the
95th percentile with 1 to 5 s less stalled; over every case 11% more is
lost and the sound is heard 6% sooner. Nothing else moves.

## After wlshare's walk

wlshare's quality walk, the shape this one is, took three rules after
v0.0.5, each measured on its own simulator over its kept captures: it
climbs back sooner on a link that shows nothing queued, steps down again
sooner while the link is far behind, and waits while the queue a step left
drains rather than stepping on it. Each was tried here the same way, on the
cases above with zdesk's ceiling and batching, against the walk as v0.0.5
had it (`bench/walk-v0.0.5.jsonl`), one on top of the one before; with them a
fourth of this walk's own, fewer pings on a passed stream. `tools/walk-sweep.sh`
ran each with its constant moved one way and the other, and the figures
are over every case and over those where a link narrowed, swung or dipped,
as the constants' table below reads them.

### A climb on a link with nothing queued

A step up the link took, with no hearing since that measured more than
15 ms of lag (`LAG_IDLE`), is followed by the next after a second of clear
hearings (`CLIMB_SPAN`) rather than three: a link that has recovered shows
nothing queued, where one that is only keeping up shows a little on most
pongs and climbs at the pace it did.

Alone, it lost 61% more sound where a link narrowed, swung or dipped
(`bench/walk-climb.jsonl`, `noquiet`): the speech is paused four seconds in
every ten, a paused desktop's packets are three bytes whatever the rate, and
a pong behind them is as clear as any. The climb stepped the rate past a
link narrowed to 100 kbit/s in every pause, 121 to 169 kbit/s in 1.3 s, and
the sound that resumed was skipped, 3.2 s of the minute against 0.9. So
zdesk tells the walk what every batch carried, its packets and bytes
(`sent`), and a pong behind sound that came to under half the rate's share
(`QUIET_FRACTION`) is clear but no hearing with nothing queued: it counts
towards a clear span as before, and never towards a climb. Under a quarter
or an eighth instead, the floor case on wlshare's speech lost 10.5 s against
8.3.

With that, against the walk before it:

- Under the dips, the rate is 155 to 163 kbit/s where it sat at 103 to
  151, back at the ceiling within 7 s of each dip rather than never; what
  it costs is 0.2 s more sound lost and 0.3 s more stalled on music over
  RDP, the probe past a dip's refusal.
- On the longer route the rate is back 9 to 14 kbit/s higher, 138 to 147.
- Over every case the rate is 2% higher for 0.4% more sound lost and 7%
  more moves; where a link narrowed, swung or dipped, 3% higher for 5% more
  lost and 5% more stalled, the narrowest link's music over wlshare heard
  90 ms later at the 95th percentile. A link that recovers moves no sooner:
  the refusal of its narrow stretch holds, and the pongs after it carry a
  little lag.
- A span of 500 ms moves the rate 16% more often for the same rate and
  9% more lost where a link narrowed; 1.5 s gains 6 kbit/s less under the
  dips. An idle lag of 10 or 25 ms measures as 15.

### A step down again sooner while far behind

While the link is still `LAG_HEAVY` behind after a step down, 150 ms or
more, the next verdict is read a second after it (`HEAVY_COOLDOWN`) rather
than two: every packet coded at a rate that far over the link is that much
more queue. On top of the climb (`bench/walk-heavy.jsonl`):

- Over every case 5% less sound is lost, heard 3% sooner at the 95th
  percentile with 5% less stalled, for 1% more rate; where a link narrowed,
  swung or dipped 12% less is lost.
- A link narrowed to 100 kbit/s loses 0.6 s of the minute against 0.9, on
  music and on speech over RDP; between the floors speech over RDP loses
  4.6 s against 8.9 and is heard 0.2 s sooner, over wlshare 7.0 against 8.3.
  What it costs is rate on the link that starts narrow, 122 to 124 kbit/s
  against 130 to 133, heard 60 ms sooner.
- Half a second loses as much over every case and 18% less where a link
  narrowed, at 94 to 96 kbit/s on the narrowest link against 102 to 106;
  1.5 s loses 2% less. A second, half the cooldown, as wlshare's walk has
  it.

### Fewer pings on a passed stream

A ping goes no sooner than 100 ms after the last (`PING_SPACING`) unless
the walk is skipping, when they are the probe that ends it. On wlshare's
stream one went every round trip, ahead of every batch, about 32 bytes
each in its record; an RDP host's batches are 186 ms apart, so nothing
moves on that stream. On top of the climb (`bench/walk-spacing.jsonl`):

- A quarter fewer pings over every case, 4% less stalled and heard 2%
  sooner at the 95th percentile for the same sound lost; where a link
  narrowed, swung or dipped 2% less is lost. Between the floors wlshare's
  speech loses 7.0 s against 8.3, under the floor its sound is heard 0.1 to
  0.2 s sooner at the 95th percentile with 1 to 2.6 s less stalled, and the
  worst packet 0.2 to 0.7 s later. A jittery link narrowed to 200 kbit/s
  holds 163 kbit/s on speech over wlshare against 169.
- 150 ms writes 39% fewer pings and 180 ms 44%, but hearings come that
  much further apart on a passed stream: under the dips wlshare's speech
  holds 139 kbit/s against 160, on the link that starts narrow 119 to 121
  against 130, and where a link narrowed, swung or dipped 9% and 13% more
  is lost.

### A wait while a step's queue drains, not taken

wlshare's walk reads a verdict whose lag has fallen by a fifth or more since
the last step down as that step draining the queue it left, not as a step
too small, and waits a cooldown before stepping again. Tried here the same
way on top of the climb (`bench/walk-drain.jsonl`), and with the two rules
above (`bench/walk-sweep.jsonl`'s round, `base` against `nodrain` in the
sums below), everything it moves is at the floors: 7% less sound is lost
over every case, 27.7 s of the minute under the floor on music over RDP
against 29.9, and the sound is heard 6% later at the 95th percentile with
12% more stalled, 0.94 s against 0.42 on wlshare's music under the floor.
Where a link narrowed, swung or dipped it loses 5% more alone and the same
with the rules above. A skip drains this walk's queue, not the rate, so a
lag that is falling is one the skip is draining and the verdict waits on
nothing; a wait there is sound queued longer. A trade, and not taken.

### What the round comes to

The walk with the climb, the shorter cooldown while far behind and the
spaced pings (`bench/walk.jsonl`) against v0.0.5's
(`bench/walk-v0.0.5.jsonl`), over the same cases:

| log | kbit/s mean | late p95 sum | late max sum | lost s | stalled s | moves | pings |
|---|---:|---:|---:|---:|---:|---:|---:|
| walk-v0.0.5 | 152 | 20731 | 49066 | 145.1 | 100.3 | 389 | 30923 |
| walk | 155 | 19971 | 50412 | 139.1 | 92.3 | 402 | 23189 |

- Under the dips the rate is 155 to 163 kbit/s where it was 103 to 151;
  a link narrowed to 100 kbit/s loses 0.6 s of the minute against 0.9 on
  three of four runs and 1.2 against 0.8 on speech over wlshare; between
  the floors speech loses 4.6 s against 8.9 over RDP and 6.8 against 8.3
  over wlshare; under the floor wlshare's sound is heard 0.1 to 0.2 s sooner
  at the 95th percentile with 1 to 3 s less stalled, and its worst packet
  0.2 to 0.7 s later.
- What it costs: on the link that recovers wlshare's speech holds
  144 kbit/s against 164, the pings it is heard by a quarter fewer; on a
  link narrowed to 200 kbit/s wlshare's speech holds 163 to 167 against
  169 to 172; on the link that starts narrow 119 to 124 against 130 to 132.
- Over every case 3 kbit/s more, 4% less sound lost, 8% less stalled,
  heard 4% sooner at the 95th percentile and 3% later at the worst, with
  a quarter fewer pings.

## The constants

`tools/walk-sweep.sh` builds the crate with each constant moved one way and
the other and runs every case on it, at zdesk's ceiling and batching
(`bench/walk-sweep.jsonl`, a line a case and variant; `v0.0.5` is the walk
before the round above). Against the walk as it is, over every case and
over those where a link narrowed, swung or dipped — every case but the
jittery ones and the floors' — and the range of rates held on the jittery
ones:

| Variant | Rate | Lost | Late p95 | Moves | Rate, narrowed | Lost, narrowed | Late p95, narrowed | Stalled, narrowed | Moves, narrowed | kbit/s, jittery |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| base | +0% | +0% | +0% | +0% | +0% | +0% | +0% | +0% | +0% | 163–192 |
| behind150 | +1% | +9% | +9% | +2% | +1% | +195% | +2% | +81% | +3% | 170–192 |
| behind20 | -20% | +91% | -8% | +16% | -0% | -2% | -7% | -8% | +11% | 32–43 |
| behind60 | -7% | +18% | -1% | +14% | +0% | -11% | -1% | -5% | +4% | 39–192 |
| behind80 | -5% | +5% | -1% | +8% | +1% | -10% | -1% | -4% | -0% | 48–192 |
| clear25 | +0% | +0% | -0% | -1% | +0% | +0% | -0% | -0% | +0% | 175–192 |
| climb1500 | -1% | +1% | -0% | -1% | -1% | +12% | -0% | +4% | -1% | 163–192 |
| climb500 | +0% | +2% | +0% | +13% | +0% | +28% | +1% | +12% | +17% | 163–192 |
| cool1 | +0% | +1% | +0% | +8% | +0% | +4% | +0% | +2% | +9% | 169–192 |
| cool3 | -0% | -0% | +0% | +0% | -0% | +0% | +0% | +0% | +0% | 163–192 |
| down34 | +1% | +2% | -0% | +5% | +1% | +25% | +0% | +10% | +7% | 177–192 |
| floor16 | -1% | -30% | -1% | +11% | -0% | +0% | -0% | +0% | +1% | 163–192 |
| floor32 | +1% | +37% | +6% | -7% | +0% | +0% | -0% | -0% | +0% | 163–192 |
| hearings3 | +1% | +0% | +4% | -3% | +1% | +25% | +1% | +15% | -3% | 169–192 |
| heavy1500 | +0% | +3% | +2% | +1% | +0% | +4% | +1% | +3% | +0% | 163–192 |
| heavy300 | +0% | +2% | +1% | +3% | +0% | +23% | +1% | +8% | +3% | 169–192 |
| heavy500 | -0% | +0% | +1% | +5% | -1% | -2% | +2% | +0% | +7% | 163–192 |
| hold30 | -0% | -1% | -0% | -11% | -1% | -7% | +0% | -2% | -11% | 175–192 |
| hold5 | +0% | +4% | +1% | +19% | +0% | +17% | +1% | +11% | +19% | 168–192 |
| idle10 | -0% | +0% | -0% | -0% | -0% | +0% | -0% | -0% | -0% | 163–192 |
| idle25 | +0% | +0% | +0% | +0% | +0% | +0% | +0% | +0% | +0% | 163–192 |
| noclimb | -2% | +1% | -0% | -6% | -3% | +8% | -1% | +1% | -8% | 163–192 |
| noheavy | -0% | +5% | +3% | +2% | -0% | +16% | +2% | +6% | +2% | 163–192 |
| nooverdue | +3% | +30% | +69% | -10% | +1% | +58% | +1% | +29% | -9% | 177–192 |
| noprobe | -5% | -10% | +6% | -5% | -7% | -15% | -0% | -0% | -5% | 163–192 |
| noquiet | +1% | +11% | +3% | +18% | +1% | +88% | +2% | +37% | +20% | 163–192 |
| noskip | -9% | -94% | +730% | -8% | -12% | -100% | +6% | -6% | -2% | 163–192 |
| nospacing | +0% | -1% | +2% | +2% | +0% | +4% | -0% | +3% | +2% | 169–192 |
| onestep | +1% | +12% | +7% | +5% | +0% | +27% | +2% | +10% | +1% | 169–192 |
| overdue200 | +0% | +3% | +2% | -2% | +0% | +43% | -0% | +22% | -1% | 177–192 |
| overdue50 | -0% | -1% | +1% | +5% | -0% | +8% | -0% | +3% | +4% | 164–192 |
| quiet4 | -0% | +4% | +1% | +2% | +0% | +0% | -0% | +0% | +0% | 163–192 |
| quiet8 | +0% | +4% | +1% | +2% | +0% | +0% | -0% | +0% | +0% | 163–192 |
| skip150 | +1% | +7% | -3% | +1% | +2% | +55% | -4% | +10% | -1% | 163–192 |
| skip300 | +0% | -2% | +2% | -1% | +0% | -34% | +3% | -6% | -1% | 163–192 |
| spacing150 | +0% | -1% | -0% | +0% | -0% | +11% | -2% | +4% | +1% | 163–192 |
| spacing180 | -0% | -1% | +1% | +3% | -0% | +13% | +0% | +6% | +5% | 162–192 |
| span2 | +1% | +6% | +2% | +22% | +1% | +54% | +2% | +22% | +24% | 169–192 |
| span5 | -2% | -1% | -1% | -17% | -2% | -7% | -2% | -6% | -16% | 163–192 |
| together | -2% | +0% | -2% | -18% | -3% | -3% | -2% | -6% | -16% | 163–192 |
| up32 | -1% | -0% | -0% | +15% | -2% | +3% | -0% | +1% | +20% | 167–192 |
| up8 | +1% | +4% | +0% | -8% | +1% | +32% | +0% | +14% | -9% | 170–192 |
| v0.0.5 | -2% | +4% | +4% | -3% | -4% | +12% | +0% | +2% | -6% | 169–192 |

What it says, and what each constant's comment carries:

- The behind threshold is set by jitter: at 20 ms the walk read up to
  60 ms of it on a link with room as a link behind and walked to 32 to
  43 kbit/s; at 60 and 80 ms, up to 100 ms of it walked to 39 to 57 and 48
  to 88. At 150 ms three times the sound was lost where a link narrowed,
  swung or dipped.
- Without skipping, the sound under the floor is 16 to 41 s late; without
  overdue hearings, 69% later at the 95th percentile summed, and 30% more
  is lost over every case. Without the floor's skipping (`noprobe`), 10%
  less is lost, all of it under the floors, and the longer route holds the
  rate at the floor (above).
- Without the climb the rate is 2% lower, most of it under the dips and on
  the longer route; without quiet sound told apart 11% more is lost over
  every case and 88% more where a link narrowed, swung or dipped; without
  the shorter cooldown while far behind 5% more is lost, 16% where a link
  narrowed; without the spaced pings 1% less is lost for a third more pings
  and 4% more lost where a link narrowed.
- Every step-size, span and cooldown variant either loses more sound or
  trades rate for less loss. A 30 s refusal hold loses 7% less where a
  link narrowed, swung or dipped and moves the rate 11% less often, but
  holds it at 128 to 164 kbit/s after a link recovers where this is back
  at 144 to 174; a 5 s clear span, alone or with the 1 s cooldown and the
  30 s hold, holds speech at 110 to 138 kbit/s under the dips where this
  keeps 160 to 163. Neither is taken.

## The floor

The floor was 32 kbit/s, on the word that libopus narrows the band and
folds the stereo below it. Measured (`benchmarks.md`, the floor), it does
neither at 24: every packet of every recording is stereo at 24 and none at
16, and the band is the same at 16, 24 and 32. So the walk's floor is
24 kbit/s now, a worse sound than 32 but the same one. Where the link sits
between the two, and under both (`bench/walk-sweep.jsonl`, the `floor32`
variant, against `bench/walk.jsonl`, on the walk as it is after the round
above):

| case | source | sound | floor 32 | floor 24 |
|---|---|---|---|---|
| | | | kbit/s, late p95/max ms, lost s, stalled s | kbit/s, late p95/max ms, lost s, stalled s |
| between-floors | rdp | music | 71, 637/3825, 17.3, 10.4 | 56, 583/3825, 5.0, 4.5 |
| between-floors | rdp | speech | 92, 823/3726, 13.2, 7.3 | 71, 574/3726, 4.6, 5.1 |
| between-floors | wlshare | music | 60, 326/2162, 21.0, 11.2 | 54, 263/2162, 10.7, 5.9 |
| between-floors | wlshare | speech | 61, 382/2359, 12.7, 8.3 | 55, 313/2359, 6.8, 4.7 |
| under-floor | rdp | music | 96, 887/6805, 34.0, 16.4 | 84, 686/6805, 30.5, 13.9 |
| under-floor | rdp | speech | 116, 921/4974, 21.0, 10.5 | 103, 768/4974, 19.3, 9.2 |
| under-floor | wlshare | music | 60, 729/3978, 33.4, 22.3 | 54, 423/3978, 29.9, 17.4 |
| under-floor | wlshare | speech | 60, 580/4351, 27.5, 16.5 | 54, 453/4351, 22.0, 13.6 |

Between the floors 4.6 to 10.7 s of the minute is skipped against 12.7 to
21.0; over every case 32 lost 37% more. 16 kbit/s would lose 30% less again
(the `floor16` variant), in mono.

## What the users do

The walk is the users' to drive, and zdesk drives it differently now:

- **zdesk.** The audio socket's task writes a ping ahead of a batch when
  `ping` says to, and at once after a pong while `skipping`; tells the walk
  `pong` for a pong of its own (the heartbeat's pings carry another
  payload), `tick` at `next_tick`, and `sent` for every batch with how long
  its send blocked and what it carried, its packets and bytes, which says
  whether the sound behind the next ping was quiet;
  and publishes `skipping` beside `behind` for the side that reads the
  queue, which drops what comes while it is set: an RDP buffer before the
  encoder, a passed unit before the socket with the gap marked. That task
  has to keep reading pongs while a write is blocked, which today's single
  select does not: the write goes to a task of its own. Its ceiling is
  192 kbit/s, and it batches a passed stream's units as above. Everything
  else — the queue's depth, the channel's, the shedding — stays.
- **wlshare.** Nothing: it codes at the rate it is told, and the gateway's
  queue is where its sound is skipped.

## What is not modelled

- Losses and retransmissions: the bottleneck's buffer never overflows, the
  window being what it holds. A lost segment is a lag like any other to a
  ping, and a longer one.
- The picture as a stream of its own: it appears as the dips, and as the
  path's delay, not as a TCP flow sharing the bottleneck.
- Jitter as a Wi-Fi hop has it, in bursts: here it is uniform and
  independent, message by message.
- The browser's decoding, and any time a page takes to read a message.
- A link's share moving with the picture's: each case gives the sound's own.
