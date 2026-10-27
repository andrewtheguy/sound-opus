# The plan: the walk after wlshare's, on its two users' pipelines

Written on 2026-10-27, before any of it is done. What the crate is now is in
the module docs of `src/lib.rs` and `src/walk.rs`, and how it was measured in
[benchmarks.md](benchmarks.md) and [walk.md](walk.md). The figures this plan
calls for go into `walk.md` and the constants' doc comments, each with the
log it came from under `bench/`, as before.

## Why

v0.0.5 settled the encoder and the walk by measurement, and `walk.md` names
what the walk still costs its users: a picture's keyframe on the browser's
link, half a second every ten seconds, is read as a narrowing and the rate
sits at 103 to 151 kbit/s under zdesk's ceiling of 192 for stalls no shorter
than holding the ceiling; a link that recovers is back at the ceiling only
after four clear spans of three seconds and the cooldowns between them; a
link narrowed to 200 kbit/s has the rate probing between 124 and 282 and
walked back. Since then wlshare's quality walk (`wlshare-vp9`'s
`QualityWalk`, releases 0.0.89 to 0.0.96) took three improvements, each
measured on its own simulator over the kept captures: it steps down sooner
while the link is far behind, waits while the queue a step left drains
rather than stepping on it again, and climbs back sooner on a link that
shows nothing queued. This walk has the same shape on the one dial Opus has,
and the same cases in its simulator, so each is tried here the same way, on
the gateway's pipeline over the users' links, and kept only where it
measures better.

The encoder's settings stand as measured: complexity 10, `AUDIO`,
constrained VBR, 20 ms, a floor of 24 kbit/s. The simulator codes every
case through the real encoder, so how libopus's bytes follow a move of the
rate is in every figure already. Nothing of the encoder changes here.

## What the users are, since v0.0.5

- **zdesk** (`src/audio.rs`, `src/ws.rs`): the one walk, for an RDP host's
  sound coded at the gateway and for wlshare's passed through. The audio
  socket's task asks the walk for a ping ahead of every batch, at once
  after a pong while skipping, hears the pong and the overdue tick, and
  tells the walk how long a send blocked; the pump drops what comes while
  the walk skips and sheds silence while it says the link is behind. Its
  ceiling is 192 kbit/s, and it batches a passed stream's units to 60 ms
  while the rate is 64 kbit/s or under. On RDP a batch is a 186 ms wave
  buffer, so a ping goes every 186 ms; on wlshare a batch is a unit every
  20 ms, or 60 ms batched, and a ping goes as soon as the last pong is
  back, every round trip.
- **wlshare** (`crates/wlshare/src/rfb/audio.rs`): codes each 20 ms block at
  the rate the gateway last named. Nothing of the walk runs there.

Neither user changes for anything below: every rule is the walk's, behind
`ping`, `pong`, `tick` and `sent` as they are. The pin bump is the one
change each takes.

## What is measured, in order

Every item is a run of `tools/walk-cases.sh` on the walk with it and
without, the sweep (`tools/walk-sweep.sh`) with the item's constant moved
one way and the other, and its figure against the walk as it is
(`bench/walk.jsonl`): the rate, the sound lost, the lateness at the 95th
percentile, the stalls and the moves, over every case and over those where
a link narrowed, swung or dipped, and the range of rates held on the
jittery ones. An item is kept where it loses less sound or holds more rate
for no more lost, and is heard no later; one that trades one for the other
is written down and not taken.

1. **Climb sooner on a link with nothing queued.** After a step up the link
   took, the next comes after a shorter span of clear hearings while none
   of them measured more than a little lag (`LAG_IDLE`, `CLIMB_SPAN`): a
   link that has recovered shows nothing queued, where one that is only
   keeping up shows a little on most pongs and climbs at the pace it did.
   What it is for: the dips and the recovery, where the rate takes four
   spans to come back. What it may cost: a step up a link refuses sooner on
   a link barely wide enough, and more moves. Spans of 0.5, 1 and 1.5 s and
   idle lags of 10, 15 and 25 ms.
2. **Step down sooner while the link is far behind.** While the lag is
   `LAG_HEAVY` or more after a step down, the next verdict is read after a
   shorter cooldown (`HEAVY_COOLDOWN`): every packet coded at a rate that
   far over the link is more queue. What it is for: the link that narrows
   from 1000 to 100 or 200 kbit/s, where the first step is at 0.5 s and the
   rest a cooldown each. What it may cost: a second step on the queue the
   first left. 0.5, 1 and 1.5 s against the cooldown of 2.
3. **Wait while the queue a step left drains.** A verdict behind whose lag
   has fallen by a fifth or more since the last step down (`DRAIN_FRACTION`)
   is the step draining its queue, not a step too small: the walk waits a
   cooldown and looks again. What it is for: the rate walked under the link
   and back on the narrowed links, and whatever item 2 costs. A fifth, a
   tenth and a third.
4. **Fewer pings on a passed stream.** A ping is written ahead of a batch
   when none is in flight, so on wlshare's stream one goes every round trip:
   about 32 bytes each in its record, 4 to 6 kbit/s at a 40 ms round trip,
   which at the floor is a sixth of the sound and on a link of 28 kbit/s a
   seventh of the link. A ping is not written within `PING_SPACING` of the
   last unless the walk is skipping, when they are the probe that ends it.
   What it is for: the floors on wlshare's stream. What it may cost: a lag
   heard later on a passed stream, and fewer clear hearings in a span. 100,
   150 and 200 ms; RDP's batches are 186 ms apart already, so nothing moves
   there. `walk-sim` says how many pings a run wrote.
5. **The sweep again**, every constant with the items in, so the figures
   on each comment are against the walk as it will be.

## What is not done

- No windowed least round trip, as wlshare's walk floors its fences over a
  minute: here a lag that does not drain while the sound is skipped is
  measured as the path's from then (`walk.md`, the path's lag), which tells
  a longer route apart within a second and never reads a standing queue as
  the path. A window would add nothing the probe does not and would believe
  a queue the walk should drain.
- No keyframe hold: the sound's walk is not told the picture's keyframes,
  and a dip is a narrowing until it ends; item 1 is what makes a dip cheap.
- Nothing of the encoder: no DTX for a paused desktop's 3-byte units (both
  wires send a unit a block and the browser's decoder reads one), no frame
  size but 20 ms, no setting under complexity 10 (`benchmarks.md`).
- Nothing in either user but the pin.

## The order of work

1. This plan.
2. Items 1 to 4, one at a time, each a commit whose message carries the
   figures, with its section in `walk.md` and its constant's comment; the
   cases' log of each kept under `bench/`.
3. Item 5; the constants' table in `walk.md` made again; this plan removed,
   its reasons in `walk.md`.
4. Release v0.0.6; bump the pins in wlshare and zdesk, each on its own
   branch and PR.
