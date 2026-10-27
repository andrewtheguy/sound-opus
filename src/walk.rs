//! The rate a link will bear, walked under the configured one.
//!
//! The configured bitrate is a ceiling. Between it and [`BITRATE_FLOOR`] the
//! rate walks down while the link falls behind and back up while it keeps up,
//! and a ceiling at or under the floor stays where it is: nothing under it is
//! worth giving up. The signal is lag: how long the newest sound waits for the
//! link, read off a ping written on the sound's own connection ahead of a
//! batch, which comes back once everything written before it has crossed. One
//! is in flight at a time ([`BitrateWalk::ping`]), its round trip over the
//! least any took is the lag ([`BitrateWalk::pong`]), and a ping still out is
//! heard as lagging by as long as it has been out, every [`OVERDUE`]
//! ([`BitrateWalk::tick`]), so a queue growing behind it is heard while it
//! grows rather than once it has drained. How long a write blocked
//! ([`BitrateWalk::sent`]) is heard too, as the longer of the two: a socket
//! takes a lot before a write blocks, but a sender whose pongs some proxy
//! answers still hears a link that will not take its sound. The ping, the
//! socket and what the rate the walk arrives at is done with — moved on an
//! [`Encoder`] here, or named to a remote that codes its own — are the
//! caller's.
//!
//! Blocking alone was the signal, and it is not one: Linux's socket to a
//! browser takes about 85 KB before a write blocks, seconds of sound at these
//! rates, and then blocks for a second or more at a time, so a walk that
//! heard only that stepped down ten seconds after the link narrowed and
//! climbed straight back while the socket drained (`docs/walk.md`).
//!
//! The shape is screen-vp9's quality walk, on the one dial Opus has.
//! One-directional by construction: the walk never goes above the ceiling,
//! since a link with room to spare shows no more of it than one that is merely
//! keeping up, and a better sound than the operator asked for was never the
//! goal. Quick to give rate up — by more the further behind the link is — and
//! slow to take it back, in steps that double while the link keeps taking them
//! and stop short of a rate it refused, so a link that is intermittently bad
//! settles at a rate it can hold rather than oscillating around one it cannot.
//! The floor is fixed rather than configured, as every adaptive stream's is:
//! past it a lower rate buys nothing a listener would call the same sound, and
//! a configured floor was a key with one right value.
//!
//! The walk also says what a sender may drop to catch up. While the link is
//! *behind*, silence is free to shed before the encoder: it drains a backlog by
//! exactly that much and loses nothing anyone hears. A lag past [`SKIP_LAG`]
//! is sound queued that would be heard that late, and the walk is
//! *skipping* ([`BitrateWalk::skipping`]): the sender drops what comes next
//! and pings back to back until a pong says the queue has drained, so what is
//! heard comes back to live rather than staying behind for as long as a lower
//! rate takes to drain it. A lag that does not drain while nothing is written
//! is not this sound's — another stream's queue at the bottleneck, a longer
//! route — and becomes the round trip it is measured over, so the walk
//! neither skips nor gives rate up for what neither would drain. A sender
//! that dropped sound it could not send tells the walk so
//! ([`BitrateWalk::dropped`]), and the link is behind from that alone. A walk
//! that is not adaptive — the operator asked for the rate held — pings
//! nothing, hears nothing, holds the ceiling and never counts the link behind.
//!
//! Pure, and takes `now` rather than reading a clock, so every decision is
//! testable without waiting for one. Every constant was settled on
//! `examples/walk-sim.rs`, the gateway's pipeline in virtual time over links
//! that narrow, recover, swing, dip, jitter and fall under the floor, with an
//! RDP host's buffers and with wlshare's units; the figures are on each, and
//! the cases and runs in `docs/walk.md`.
//!
//! [`Encoder`]: crate::Encoder

use std::time::{Duration, Instant};

use crate::{BITRATE_MAX, BITRATE_MIN};

/// Lag at which the link counts as one that could not keep up. Above the
/// jitter a Wi-Fi hop adds, so a link with room is never read as behind: at
/// 20 ms, a packet's length, up to 60 ms of jitter on a link with room
/// walked the rate from 320 to 48–64 kbit/s, and at 60 and 80 ms up to
/// 100 ms of it walked the rate to 56–145, where this held 320 through both.
/// 150 ms lost over twice the sound where a link narrowed, swung or dipped,
/// the queue let grow that much longer before the rate moved.
const BEHIND_LAG: Duration = Duration::from_millis(100);

/// Lag below which the link counts as clear. Between this and [`BEHIND_LAG`]
/// is hysteresis: a link hovering there earns neither a lower rate nor its
/// rate back. 25 ms measured within 3% of it.
const CLEAR_LAG: Duration = Duration::from_millis(50);

/// Lag that is not a little behind but a lot: a step down here gives up two
/// steps of [`STEP_DOWN`], and past [`LAG_SEVERE`] three. A queue this long
/// is a link far narrower than the rate, and every packet coded at the rate in
/// the meantime is more of it. One step whatever the lag lost 46% more sound
/// where a link narrowed, swung or dipped, and 300 and 800 ms in place of
/// these 39% more.
const LAG_HEAVY: Duration = Duration::from_millis(150);
/// See [`LAG_HEAVY`].
const LAG_SEVERE: Duration = Duration::from_millis(400);

/// Lag past which the sound queued is skipped: what comes next is dropped
/// before the encoder, and pings written back to back, until one comes back
/// under [`BEHIND_LAG`], so the queue drains while nothing joins it and what
/// is heard is live again. Under it a lag is left to the rate to drain.
/// Without skipping, a link under the floor had the sound 39 to 42 s late at
/// the 95th percentile, where this keeps it to 0.9 to 1.2 s; 300 ms lost 6%
/// less and was heard 10% later, 150 ms lost 3% more for 4% sooner.
const SKIP_LAG: Duration = Duration::from_millis(200);

/// How often a ping still out is heard, once it is this far past the least
/// round trip: as lagging by as long as it has been out over that, which it
/// is at least. A queue growing faster than its pongs come back is heard as
/// it grows: waiting for the pong instead, a link that fell from 1000 kbit/s
/// to under the floor moved the rate once or twice in 50 s and the sound was
/// up to 43 s late at the 95th percentile, and over every case 4.4 times as
/// late as this. 200 ms lost 4% more sound, 50 ms measured within 4%.
pub const OVERDUE: Duration = Duration::from_millis(100);

/// Behind hearings among the last [`VERDICT_WINDOW`] before rate is given up,
/// the latest among them. Two rather than one, so a single late pong is not
/// a verdict about the link; a window rather than a run, because the lag a
/// link barely too narrow shows is intermittent. Three lost 24% more sound
/// where a link narrowed, swung or dipped.
const BEHIND_HEARINGS: u32 = 2;
/// See [`BEHIND_HEARINGS`].
const VERDICT_WINDOW: u32 = 4;

/// How long the link must have been clear before rate is taken back, and the
/// fewest clear hearings that span must hold: deliberately far more than
/// [`BEHIND_HEARINGS`]. A span rather than a count, because pongs come at the
/// sound's own cadence, a wave buffer five times a second from one host, a
/// packet fifty times a second from another. 2 s moved the rate 21% more
/// often and lost 26% more sound where a link narrowed, swung or dipped; 5 s
/// 20% less often and 26% less lost, but kept wlshare's sound at 134 to
/// 149 kbit/s where half-second dips every ten seconds — a picture's
/// keyframes on the same link — left it 228 to 254.
const CLEAR_SPAN: Duration = Duration::from_secs(3);
/// See [`CLEAR_SPAN`].
const CLEAR_HEARINGS: u32 = 4;

/// How long the link must have been clear before it stops counting as behind,
/// the state that sheds. Much shorter than [`CLEAR_SPAN`]: shedding exists to
/// drain a backlog, and a second of clear hearings means it has drained.
const RELIEF_SPAN: Duration = Duration::from_secs(1);

/// The least time between two moves of the rate, so a burst of late pongs is
/// one decision rather than one per pong, and the queue a step down leaves
/// has drained before the next verdict is read against it. 1 s and 3 s
/// measured within 3% of it over every case.
const ADJUST_COOLDOWN: Duration = Duration::from_secs(2);

/// The least time between a step up and the step down that walks it back. Far
/// shorter than [`ADJUST_COOLDOWN`]: a step up the link refuses shows in the
/// next few pongs, and every packet it is left in place is queue.
const REFUSAL_COOLDOWN: Duration = Duration::from_millis(500);

/// How far one step down moves the rate on a link a little behind: to this
/// fraction of itself, a third given up. Bigger down than up, for the same
/// reason [`CLEAR_HEARINGS`] is bigger than [`BEHIND_HEARINGS`]. A quarter
/// lost 23% more sound where a link narrowed, swung or dipped.
const STEP_DOWN: (u32, u32) = (2, 3);

/// What the first step up reclaims, as a fraction of the ceiling: a sixteenth,
/// 20 kbit/s under zdesk's 320. Every step up the link takes doubles the
/// next, to [`STEP_UP_MAX`], and a step down puts it back: a link that has
/// recovered is at the ceiling again within a few steps rather than a minute.
/// An eighth lost 18% more sound where a link narrowed, swung or dipped; a
/// thirty-second 5% less, for 3% less rate.
const STEP_UP: u32 = 16;
/// The most one step up reclaims, as a fraction of the ceiling: half.
const STEP_UP_MAX: u32 = 2;

/// How long a rate the link refused stays out of reach. A step down within
/// [`REFUSAL_WINDOW`] of a step up says the link would not take the rate it
/// was just given, and the walk keeps under it for this long before probing
/// there again, with the smallest step — TCP's slow-start threshold, on the
/// rate. Without it a walk whose steps double on the way up would spend a
/// session bouncing off the same rate. 5 s moved the rate 15% more often,
/// 30 s 15% less often for 2% less rate.
const REFUSAL_HOLD: Duration = Duration::from_secs(15);
/// See [`REFUSAL_HOLD`].
const REFUSAL_WINDOW: Duration = Duration::from_secs(4);

/// The adaptive floor, in bits per second: where the walk stops giving rate up
/// and a link still behind is left to skipping. Fixed rather than configured,
/// as every adaptive stream's is. 32 kbit/s is where Opus stereo still codes
/// the whole band at full width: degraded, but continuous, and continuity is
/// the whole point of giving rate up. Below it libopus narrows the band and
/// folds the stereo, which a listener hears as a different sound rather than
/// a worse one, and halving the rate again saves a few kilobits a second of a
/// link that is already dropping a picture's megabits. A ceiling at or under
/// it is never moved.
pub const BITRATE_FLOOR: u32 = 32_000;
const _: () = assert!(BITRATE_MIN < BITRATE_FLOOR && BITRATE_FLOOR < crate::BITRATE_DEFAULT);

/// See the [module](self).
#[derive(Debug)]
pub struct BitrateWalk {
    /// The configured rate: the most this ever asks for.
    ceiling: u32,
    /// Whether the link's lag moves this walk at all.
    adaptive: bool,
    /// The rate in force.
    bitrate: u32,
    /// Whether the link is behind: a hearing behind or the sender dropped
    /// sound, and the link has not been clear for [`RELIEF_SPAN`] since.
    behind: bool,
    /// The last [`VERDICT_WINDOW`] verdicts, newest in the low bit, a set bit
    /// for a hearing the link was behind on.
    verdicts: u8,
    /// The clear hearings since the link was last behind: when the first came,
    /// and how many.
    clear: Option<(Instant, u32)>,
    /// When the rate last moved, for [`ADJUST_COOLDOWN`]. `None` before it
    /// ever has, so the first verdict does not wait out a cooldown that never
    /// ran.
    changed_at: Option<Instant>,
    /// What the next step up reclaims, in bits per second: a [`STEP_UP`]th of
    /// the ceiling, doubling with every step up the link takes, to a
    /// [`STEP_UP_MAX`]th.
    reclaim: u32,
    /// When the last step up was taken and the rate it left, while it is the
    /// last move made: a step down within [`REFUSAL_WINDOW`] of it is a
    /// refusal, walked back to that rate.
    reclaimed: Option<(u32, Instant)>,
    /// The highest rate the walk may reach while a refusal holds, and when it
    /// was refused: halfway between the rate the link bore and the one it
    /// would not take, for [`REFUSAL_HOLD`].
    refused: Option<(u32, Instant)>,
    /// The ping in flight: when it was written, and when the walk last heard
    /// it overdue.
    ping: Option<(Instant, Option<Instant>)>,
    /// The least round trip a ping has taken: the path with none of this
    /// sound queued on it.
    least: Option<Duration>,
    /// The longest a send blocked since the walk last heard.
    blocked: Duration,
    /// While skipping: since when, and the last ping written since then that
    /// came back behind, when it was written and its lag.
    skipping: Option<(Instant, Option<(Instant, Duration)>)>,
}

impl BitrateWalk {
    /// A walk that starts at `ceiling` bits per second and goes down to
    /// [`BITRATE_FLOOR`]; `adaptive` is whether the link's lag moves it, and
    /// without it the walk holds the ceiling.
    ///
    /// `ceiling` is clamped to what an encoder takes, [`BITRATE_MIN`] to
    /// [`BITRATE_MAX`], so every rate the walk arrives at is one.
    pub fn new(ceiling: u32, adaptive: bool) -> Self {
        let ceiling = ceiling.clamp(BITRATE_MIN, BITRATE_MAX);
        Self {
            ceiling,
            adaptive,
            bitrate: ceiling,
            behind: false,
            verdicts: 0,
            clear: None,
            changed_at: None,
            reclaim: ceiling / STEP_UP,
            reclaimed: None,
            refused: None,
            ping: None,
            least: None,
            blocked: Duration::ZERO,
            skipping: None,
        }
    }

    /// The configured rate, which the walk never goes above.
    pub fn ceiling(&self) -> u32 {
        self.ceiling
    }

    /// Whether the link's lag moves this walk.
    pub fn adaptive(&self) -> bool {
        self.adaptive
    }

    /// The rate in force, in bits per second.
    pub fn bitrate(&self) -> u32 {
        self.bitrate
    }

    /// Whether the link is behind right now: a hearing was, or the sender
    /// dropped sound ([`Self::dropped`]), and the link has not been clear for
    /// a second since. What a sender with something free to shed — silence,
    /// before an encoder — sheds it on. Never on a walk that is not adaptive.
    pub fn behind(&self) -> bool {
        self.behind
    }

    /// Whether to write a ping now: `true` when none is in flight, and the
    /// walk takes it as written at `now`. Asked ahead of every batch, and
    /// while [`Self::skipping`] at once when a pong is back, since no batch
    /// comes then. Never on a walk that is not adaptive.
    pub fn ping(&mut self, now: Instant) -> bool {
        if !self.adaptive || self.ping.is_some() {
            return false;
        }
        self.ping = Some((now, None));
        true
    }

    /// The ping in flight came back at `now`. Returns the rate to code the
    /// next packet at if the walk moved; nothing for a pong with no ping in
    /// flight.
    pub fn pong(&mut self, now: Instant) -> Option<u32> {
        let (went, _) = self.ping.take()?;
        let trip = now.saturating_duration_since(went);
        let least = self.least.map_or(trip, |least| least.min(trip));
        self.least = Some(least);
        let mut lag = trip - least;
        // A ping written while skipping had nothing of this sound written
        // after it, so a queue of this sound's drains between two of them by
        // the time between them.
        if let Some((since, last)) = &mut self.skipping
            && went >= *since
        {
            if lag < BEHIND_LAG {
                self.skipping = None;
            } else if last.is_some_and(|(before, was)| lag + went.saturating_duration_since(before) / 2 > was) {
                // One that did not drain by half that is the path's — another
                // stream's queue at the bottleneck, a longer route — which no
                // skip or rate drains, and the round trip it took is the
                // path's from now.
                self.least = Some(trip);
                self.skipping = None;
                lag = Duration::ZERO;
            } else {
                *last = Some((went, lag));
            }
        }
        self.hear(lag, now)
    }

    /// Hear the ping in flight if it is overdue at `now`: out [`OVERDUE`]
    /// past the least round trip, and every [`OVERDUE`] after, it lags by as
    /// long as it has been out over that at least. Returns the rate if the
    /// walk moved. Called at [`Self::next_tick`], or whenever; early, it hears
    /// nothing.
    pub fn tick(&mut self, now: Instant) -> Option<u32> {
        if self.next_tick().is_none_or(|due| now < due) {
            return None;
        }
        let (went, _) = self.ping?;
        let least = self.least?;
        self.ping = Some((went, Some(now)));
        self.hear(now.saturating_duration_since(went).saturating_sub(least), now)
    }

    /// When [`Self::tick`] next has a ping to hear: `None` while none is in
    /// flight, or before any has come back to say what the round trip is.
    pub fn next_tick(&self) -> Option<Instant> {
        let (went, heard) = self.ping?;
        let least = self.least?;
        Some(heard.map_or(went + least + OVERDUE, |at| at + OVERDUE))
    }

    /// A send blocked for `blocked`: time the socket, or the queue before it,
    /// had no room. Heard at the next pong or tick, if longer than its lag.
    pub fn sent(&mut self, blocked: Duration) {
        if self.adaptive {
            self.blocked = self.blocked.max(blocked);
        }
    }

    /// Whether to skip what arrives now: drop it before the encoder, or a
    /// unit before the socket, because a lag past [`SKIP_LAG`] says that
    /// much is queued, until a pong says it has drained. Never on a walk that
    /// is not adaptive.
    pub fn skipping(&self) -> bool {
        self.skipping.is_some()
    }

    /// The sender dropped sound it could not send: the link is behind by at
    /// least the queue that overflowed, whatever its lag measured, and stays
    /// so until a second of clear hearings has followed. No verdict on the
    /// rate: the lag that filled the queue was heard already.
    pub fn dropped(&mut self) {
        if !self.adaptive {
            return;
        }
        self.behind = true;
        self.clear = None;
    }

    /// The link lags by `lag`, or by how long a send blocked since the last
    /// hearing if that is longer. Returns the rate to code the next packet at
    /// if the walk moved.
    fn hear(&mut self, lag: Duration, now: Instant) -> Option<u32> {
        let lag = lag.max(std::mem::take(&mut self.blocked));
        let behind = lag >= BEHIND_LAG;
        self.verdicts = ((self.verdicts << 1) | u8::from(behind)) & ((1 << VERDICT_WINDOW) - 1);
        if lag >= SKIP_LAG && self.skipping.is_none() {
            self.skipping = Some((now, None));
        }
        if behind {
            self.behind = true;
            self.clear = None;
        } else if lag <= CLEAR_LAG {
            // Between the two thresholds a hearing neither counts nor ends
            // the run: not evidence of room, not evidence against it either.
            let (since, count) = self.clear.map_or((now, 1), |(since, count)| (since, count + 1));
            self.clear = Some((since, count));
            if now.saturating_duration_since(since) >= RELIEF_SPAN {
                self.behind = false;
            }
        }
        // Walking back a step up the link refused does not wait out the full
        // cooldown: the refusal is in the next few pongs, and every packet is
        // queue. Only while that step is the last move, though.
        let cooldown = if behind && self.reclaimed.is_some_and(|(_, at)| self.changed_at == Some(at)) {
            REFUSAL_COOLDOWN
        } else {
            ADJUST_COOLDOWN
        };
        if self.changed_at.is_some_and(|at| now.saturating_duration_since(at) < cooldown) {
            return None;
        }
        let moved = if behind && self.verdicts.count_ones() >= BEHIND_HEARINGS {
            self.give_up(lag, now)
        } else if self.clear.is_some_and(|(since, count)| count >= CLEAR_HEARINGS && now.saturating_duration_since(since) >= CLEAR_SPAN) {
            self.take_back(now)
        } else {
            return None;
        };
        if !moved {
            return None;
        }
        self.verdicts = 0;
        self.clear = None;
        self.changed_at = Some(now);
        Some(self.bitrate)
    }

    /// Give rate up, by more the longer the lag. `false` when there is
    /// nothing left to give.
    fn give_up(&mut self, lag: Duration, now: Instant) -> bool {
        let steps = if lag >= LAG_SEVERE {
            3
        } else if lag >= LAG_HEAVY {
            2
        } else {
            1
        };
        let mut moved = false;
        if let Some((from, _)) = self.reclaimed.filter(|(_, at)| now.saturating_duration_since(*at) <= REFUSAL_WINDOW) {
            // A step up the link refused: back to the rate it bore, and the
            // walk may come halfway back up towards the one it would not take.
            // A link that is still behind there steps down from it as usual.
            let cap = from + (self.bitrate - from) / 2;
            self.refused = Some((cap, now));
            if self.bitrate > from {
                self.bitrate = from;
                moved = true;
            }
        } else if self.bitrate > BITRATE_FLOOR {
            let mut wanted = self.bitrate;
            for _ in 0..steps {
                wanted = wanted * STEP_DOWN.0 / STEP_DOWN.1;
            }
            self.bitrate = wanted.max(BITRATE_FLOOR);
            moved = true;
        }
        self.reclaim = self.ceiling / STEP_UP;
        self.reclaimed = None;
        moved
    }

    /// Take rate back: a step that doubles while the link keeps taking them,
    /// held under a rate the link refused, and never past the ceiling. `false`
    /// when there is nothing to take back.
    fn take_back(&mut self, now: Instant) -> bool {
        if self.refused.is_some_and(|(_, at)| now.saturating_duration_since(at) >= REFUSAL_HOLD) {
            self.refused = None;
        }
        let ceiling = self.refused.map_or(self.ceiling, |(cap, _)| cap).min(self.ceiling);
        let wanted = self.bitrate.saturating_add(self.reclaim).min(ceiling);
        if wanted <= self.bitrate {
            return false;
        }
        // A step the refusal cut short is the last of its run: the probe past
        // the refused rate, once the hold is over, starts small again.
        self.reclaim = if wanted == ceiling && ceiling < self.ceiling {
            self.ceiling / STEP_UP
        } else {
            self.reclaim.saturating_mul(2).min(self.ceiling / STEP_UP_MAX)
        };
        self.reclaimed = Some((self.bitrate, now));
        self.bitrate = wanted;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BITRATE_DEFAULT, Encoder, Stream};

    /// The least round trip in these tests.
    const TRIP: Duration = Duration::from_millis(40);
    /// The cadence pongs come at from a host that hands over a wave buffer
    /// five times a second.
    const BATCH: Duration = Duration::from_millis(200);
    /// Clear hearings at that cadence that make up a [`CLEAR_SPAN`], and then
    /// one.
    const CLEAR_RUN: u32 = 16;

    /// A walk from `ceiling` whose first ping came back in [`TRIP`], at
    /// `start`.
    fn walk(ceiling: u32, start: Instant) -> BitrateWalk {
        let mut walk = BitrateWalk::new(ceiling, true);
        assert!(walk.ping(start - TRIP));
        assert_eq!(walk.pong(start), None);
        walk
    }

    /// A ping written at `at` and back `lag` past the least round trip: the
    /// move it made, if any.
    fn heard(walk: &mut BitrateWalk, lag: Duration, at: Instant) -> Option<u32> {
        assert!(walk.ping(at - TRIP - lag), "a ping was still in flight");
        walk.pong(at)
    }

    /// `count` pongs `lag` late, [`BATCH`] apart after `at`: the last move,
    /// and when they ended.
    fn hearings(walk: &mut BitrateWalk, count: u32, lag: Duration, mut at: Instant) -> (Option<u32>, Instant) {
        let mut moved = None;
        for _ in 0..count {
            at += BATCH;
            if let Some(bitrate) = heard(walk, lag, at) {
                moved = Some(bitrate);
            }
        }
        (moved, at)
    }

    /// Every move of `walk` under `count` clear runs, each a cooldown apart.
    fn recovery(walk: &mut BitrateWalk, count: u32, mut at: Instant) -> Vec<u32> {
        let mut seen = Vec::new();
        for _ in 0..count {
            let (moved, then) = hearings(walk, CLEAR_RUN, Duration::ZERO, at);
            seen.extend(moved);
            at = then + ADJUST_COOLDOWN;
        }
        seen
    }

    /// Two hearings behind at `at`, as a link far enough behind gives.
    fn verdict(walk: &mut BitrateWalk, lag: Duration, at: Instant) -> Option<u32> {
        heard(walk, lag, at);
        heard(walk, lag, at)
    }

    #[test]
    fn a_clear_link_stays_on_the_ceiling() {
        let start = Instant::now();
        let mut walk = walk(BITRATE_DEFAULT, start);
        assert!(recovery(&mut walk, 4, start).is_empty());
        assert_eq!((walk.ceiling(), walk.bitrate()), (BITRATE_DEFAULT, BITRATE_DEFAULT));
        assert!(walk.adaptive() && !walk.behind());
    }

    /// A ceiling outside what the encoder takes is clamped to it, so every
    /// rate the walk arrives at is one the encoder takes.
    #[test]
    fn a_ceiling_off_the_encoder_is_clamped_to_it() {
        assert_eq!(BitrateWalk::new(u32::MAX, true).bitrate(), BITRATE_MAX);
        assert_eq!(BitrateWalk::new(0, true).ceiling(), BITRATE_MIN);
    }

    /// One ping at a time: none is asked for while one is in flight, and a
    /// pong with none in flight is nothing.
    #[test]
    fn one_ping_is_in_flight_at_a_time() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        assert_eq!(walk.pong(start), None, "no ping in flight");
        assert!(walk.ping(start));
        assert!(!walk.ping(start + BATCH), "one is in flight");
        walk.pong(start + TRIP);
        assert!(walk.ping(start + BATCH));
    }

    /// The least round trip is the path's, and a lag is a round trip over it:
    /// a quicker pong later lowers it.
    #[test]
    fn the_lag_is_over_the_least_round_trip() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        let late = BEHIND_LAG;
        assert_eq!(heard(&mut walk, late, start + BATCH), None);
        assert!(walk.behind(), "{late:?} over the trip is behind");
        // A trip shorter than the first: the path's is that, and the one
        // before it was lagging by the difference.
        let quick = TRIP / 2;
        assert!(walk.ping(start + 2 * BATCH));
        walk.pong(start + 2 * BATCH + quick);
        assert!(walk.ping(start + 3 * BATCH));
        assert_eq!(walk.pong(start + 3 * BATCH + quick + late), Some(64_000), "{late:?} over the new least, with the one before");
    }

    #[test]
    fn falling_behind_gives_rate_up_down_to_the_floor_once_a_cooldown() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(heard(&mut walk, BEHIND_LAG, start), None, "one late pong is not a verdict");
        assert!(walk.behind(), "but it does put the link behind");
        assert_eq!(heard(&mut walk, BEHIND_LAG, start), Some(64_000), "a third given up");
        for _ in 0..4 {
            assert_eq!(heard(&mut walk, BEHIND_LAG, start + ADJUST_COOLDOWN / 2), None, "inside the cooldown");
        }
        let later = start + ADJUST_COOLDOWN;
        assert_eq!(heard(&mut walk, BEHIND_LAG, later), Some(42_666));
        let later = later + ADJUST_COOLDOWN;
        assert_eq!(verdict(&mut walk, BEHIND_LAG, later), Some(BITRATE_FLOOR), "stops at the floor");
        let later = later + ADJUST_COOLDOWN;
        assert_eq!(verdict(&mut walk, LAG_SEVERE, later), None, "nothing left to give");
        assert_eq!(walk.bitrate(), BITRATE_FLOOR);
        assert!(walk.behind());
    }

    /// A ceiling at or under the floor has nothing to give: the link behind
    /// is left to skipping, and the rate is the one asked for.
    #[test]
    fn a_ceiling_under_the_floor_is_never_moved() {
        let start = Instant::now();
        for ceiling in [BITRATE_FLOOR, BITRATE_FLOOR / 2, BITRATE_MIN] {
            let mut walk = walk(ceiling, start);
            let mut at = start;
            for _ in 0..20 {
                at += BATCH;
                walk.sent(LAG_SEVERE);
                assert_eq!(heard(&mut walk, Duration::ZERO, at), None, "{ceiling}");
            }
            assert_eq!(walk.bitrate(), ceiling);
            assert!(walk.behind(), "the sender still hears that the link is behind");
        }
    }

    #[test]
    fn a_link_far_behind_gives_up_more_at_once() {
        let start = Instant::now();
        let mut heavy = walk(96_000, start);
        assert_eq!(verdict(&mut heavy, LAG_HEAVY, start), Some(42_666), "two thirds twice");
        let mut severe = walk(96_000, start);
        assert_eq!(verdict(&mut severe, LAG_SEVERE, start), Some(BITRATE_FLOOR), "three, stopped by the floor");
        let mut high = walk(300_000, start);
        assert_eq!(verdict(&mut high, LAG_SEVERE, start), Some(88_888));
    }

    /// Two hearings behind among four are a verdict, as a link barely too
    /// narrow gives them: one late, one clear, one late.
    #[test]
    fn intermittent_lag_is_still_a_verdict() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(heard(&mut walk, BEHIND_LAG, start), None);
        assert_eq!(heard(&mut walk, Duration::ZERO, start + BATCH), None);
        assert_eq!(heard(&mut walk, BEHIND_LAG, start + 2 * BATCH), Some(64_000));
    }

    /// A ping still out is heard [`OVERDUE`] past the least round trip and
    /// every [`OVERDUE`] after, lagging by as long as it has been out over
    /// the trip: a queue growing behind it is heard before its pong is back.
    #[test]
    fn a_ping_still_out_is_heard_as_it_lags() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(walk.next_tick(), None, "no ping in flight");
        assert!(walk.ping(start));
        let first = start + TRIP + OVERDUE;
        assert_eq!(walk.next_tick(), Some(first));
        assert_eq!(walk.tick(first - Duration::from_millis(1)), None, "early, it hears nothing");
        assert!(!walk.behind());
        assert_eq!(walk.tick(first), None, "one hearing is not a verdict");
        assert!(walk.behind(), "{OVERDUE:?} is behind");
        assert_eq!(walk.next_tick(), Some(first + OVERDUE));
        assert_eq!(walk.tick(first + OVERDUE), Some(42_666), "{:?} late is heavy", 2 * OVERDUE);
        // Its pong, when it is back, is one more hearing.
        assert_eq!(walk.pong(first + 2 * OVERDUE), None, "inside the cooldown");
        assert_eq!(walk.next_tick(), None);
    }

    /// A walk that has had no pong yet does not know the round trip, and
    /// hears nothing of a ping still out.
    #[test]
    fn a_ping_is_overdue_only_once_a_round_trip_is_known() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        assert!(walk.ping(start));
        assert_eq!(walk.next_tick(), None);
        assert_eq!(walk.tick(start + Duration::from_secs(10)), None);
        assert!(!walk.behind());
    }

    /// A send that blocked longer than the next pong's lag is what that pong
    /// is heard as: a socket that will not take the sound is behind whatever
    /// its pongs say.
    #[test]
    fn a_blocked_send_is_heard_with_the_next_pong() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        walk.sent(LAG_HEAVY);
        assert_eq!(heard(&mut walk, Duration::ZERO, start), None);
        assert!(walk.behind());
        walk.sent(LAG_HEAVY);
        assert_eq!(heard(&mut walk, Duration::ZERO, start + BATCH), Some(42_666));
        // Heard once: the pong after is its own.
        assert_eq!(heard(&mut walk, Duration::ZERO, start + 2 * BATCH), None);
    }

    /// A lag of [`SKIP_LAG`] or more is skipped until a ping written while
    /// skipping comes back under [`BEHIND_LAG`], the queue drained.
    #[test]
    fn a_long_lag_is_skipped_until_it_drains() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert!(!walk.skipping(), "nothing to skip on a clear link");
        let queued = 2 * SKIP_LAG;
        heard(&mut walk, queued, start);
        assert!(walk.skipping() && walk.behind());
        // Written while skipping, back to back, each behind what the one
        // before left: the queue drains by the time between them, and once
        // under the threshold it has.
        let first = queued - TRIP;
        assert!(walk.ping(start));
        walk.pong(start + TRIP + first);
        assert!(walk.skipping(), "{first:?} is still behind");
        let at = start + TRIP + first;
        let left = first.saturating_sub(TRIP + first);
        assert!(walk.ping(at));
        walk.pong(at + TRIP + left);
        assert!(!walk.skipping(), "drained");
        assert!(walk.behind(), "drained, but not yet a clear second");
    }

    /// A ping written before the skip began has sound written after it, and
    /// says nothing of whether the skip drained the queue.
    #[test]
    fn only_a_ping_written_while_skipping_ends_it() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert!(walk.ping(start));
        assert_eq!(walk.tick(start + TRIP + SKIP_LAG), None);
        assert!(walk.skipping(), "an overdue ping is a lag too");
        walk.pong(start + TRIP + SKIP_LAG + Duration::from_millis(1));
        assert!(walk.skipping(), "its pong, though clear of its own lag, went before the skip");
    }

    /// A lag that does not drain while nothing is written is the path's —
    /// another stream's queue, a longer route — and the round trip it took
    /// is the path's from then: nothing more is skipped for it, and the walk
    /// climbs back on it.
    #[test]
    fn a_lag_that_does_not_drain_is_the_paths() {
        let start = Instant::now();
        let mut walk = walk(BITRATE_DEFAULT, start);
        let path = SKIP_LAG + BEHIND_LAG;
        heard(&mut walk, path, start);
        assert_eq!(heard(&mut walk, path, start + BATCH), Some(42_666), "heard as this sound's, at first");
        assert!(walk.skipping());
        let mut at = start + BATCH;
        for _ in 0..2 {
            assert!(walk.ping(at));
            at += TRIP + path;
            walk.pong(at);
        }
        assert!(!walk.skipping(), "nothing more is skipped for the path's lag");
        let (moved, _) = hearings(&mut walk, CLEAR_RUN, path, at + ADJUST_COOLDOWN);
        assert_eq!(moved, Some(48_666), "and the walk climbs back on it");
        assert!(!walk.behind());
    }

    /// The rate comes back in steps that double, from a sixteenth of the
    /// ceiling to half of it, after a clear span each, and never past the
    /// ceiling.
    #[test]
    fn rate_comes_back_slowly_then_faster_and_never_past_the_ceiling() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        let mut at = start;
        for _ in 0..3 {
            verdict(&mut walk, BEHIND_LAG, at);
            at += ADJUST_COOLDOWN;
        }
        assert_eq!(walk.bitrate(), BITRATE_FLOOR);
        let (moved, at) = hearings(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at);
        assert_eq!(moved, None, "half a clear span is not enough");
        assert_eq!(recovery(&mut walk, 6, at), [38_000, 50_000, 74_000, 96_000]);
        assert_eq!(walk.bitrate(), 96_000);
    }

    /// A span of clear hearings is what earns rate back, not a count of them:
    /// pongs that come once a second prove the link in the same three seconds.
    #[test]
    fn a_clear_span_is_enough_however_slowly_the_pongs_come() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(verdict(&mut walk, BEHIND_LAG, start), Some(64_000));
        let mut at = start + ADJUST_COOLDOWN;
        let mut moved = None;
        for _ in 0..CLEAR_HEARINGS {
            at += Duration::from_secs(1);
            moved = moved.or(heard(&mut walk, Duration::ZERO, at));
        }
        assert_eq!(moved, Some(70_000));
        let mut quick = self::walk(96_000, start);
        verdict(&mut quick, BEHIND_LAG, start);
        let mut at = start + ADJUST_COOLDOWN;
        for _ in 0..2 * CLEAR_HEARINGS {
            at += Duration::from_millis(1);
            assert_eq!(heard(&mut quick, Duration::ZERO, at), None, "a burst of clear pongs spans no time");
        }
    }

    /// A step up the link refuses — it falls behind within seconds of it — is
    /// walked back without the full cooldown, to the rate the link bore, and
    /// the walk comes back no further than halfway towards the refused rate
    /// until the hold is over, when it probes past it with the smallest step.
    #[test]
    fn a_refused_step_up_is_walked_back_and_held_out_of_reach() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        let mut at = start;
        for _ in 0..3 {
            verdict(&mut walk, BEHIND_LAG, at);
            at += ADJUST_COOLDOWN;
        }
        let (moved, at) = hearings(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, Some(38_000));
        let at = at + ADJUST_COOLDOWN;
        let (moved, at) = hearings(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, Some(50_000));
        // Refused: behind within the window of that step, and walked back
        // after the refusal's short cooldown rather than the full one.
        let at = at + REFUSAL_COOLDOWN;
        assert_eq!(heard(&mut walk, BEHIND_LAG, at), None);
        assert_eq!(heard(&mut walk, BEHIND_LAG, at), Some(38_000), "back to the rate the link bore");
        // Clear again: up to the cap, halfway to the refused rate, and no further.
        let at = at + ADJUST_COOLDOWN;
        assert_eq!(recovery(&mut walk, 2, at), [44_000]);
        // The hold over, the walk probes past it, from the smallest step.
        let at = at + REFUSAL_HOLD;
        assert_eq!(recovery(&mut walk, 6, at), [50_000, 62_000, 86_000, 96_000]);
    }

    /// A step down long after a step up is no refusal: the link bore the rate
    /// for a while and then narrowed, which is a plain step down from it.
    #[test]
    fn a_rate_borne_for_a_while_is_not_a_refused_one() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(verdict(&mut walk, BEHIND_LAG, start), Some(64_000));
        let at = start + ADJUST_COOLDOWN;
        let (moved, at) = hearings(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, Some(70_000));
        let at = at + REFUSAL_WINDOW + BATCH;
        assert_eq!(verdict(&mut walk, BEHIND_LAG, at), Some(46_666), "a third of the rate it held");
    }

    /// Between clear and behind is a band that is evidence of neither:
    /// hearings there neither give rate up nor earn it back, and do not end a
    /// clear run.
    #[test]
    fn the_hysteresis_band_neither_gives_up_nor_takes_back() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(verdict(&mut walk, BEHIND_LAG, start), Some(64_000));
        let at = start + ADJUST_COOLDOWN;
        let between = (CLEAR_LAG + BEHIND_LAG) / 2;
        let (moved, at) = hearings(&mut walk, 4 * CLEAR_RUN, between, at);
        assert_eq!(moved, None);
        assert_eq!(walk.bitrate(), 64_000);
        // Clear hearings either side of one in the band are one run.
        let (moved, at) = hearings(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at);
        assert_eq!(moved, None);
        assert_eq!(heard(&mut walk, between, at + BATCH), None);
        let (moved, _) = hearings(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at + BATCH);
        assert_eq!(moved, Some(70_000));
    }

    /// The link is behind from the first late pong and clear again after a
    /// second of clear ones, well before any rate comes back.
    #[test]
    fn behind_rises_on_the_first_late_pong_and_clears_after_a_second() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert!(!walk.behind());
        heard(&mut walk, BEHIND_LAG, start);
        assert!(walk.behind());
        let at = start + BATCH;
        assert_eq!(heard(&mut walk, Duration::ZERO, at), None);
        assert!(walk.behind(), "one clear pong is not relief");
        let (moved, at) = hearings(&mut walk, 4, Duration::ZERO, at);
        assert_eq!(moved, None);
        assert!(walk.behind(), "{:?} of clear pongs is not a second", 4 * BATCH);
        let (moved, _) = hearings(&mut walk, 2, Duration::ZERO, at);
        assert_eq!(moved, None, "no rate came back");
        assert!(!walk.behind());
    }

    /// Sound the sender dropped puts the link behind without a pong to say
    /// so, and a clear pong right after is no relief: the second of them
    /// starts over from the drop, and the rate is not moved by it.
    #[test]
    fn dropped_sound_puts_the_link_behind_until_a_clear_second() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        // Clear pongs spanning 800 ms before the drop: with the next, a
        // second, which would be relief if the drop did not start the span over.
        let (moved, at) = hearings(&mut walk, 5, Duration::ZERO, start);
        assert_eq!(moved, None);
        walk.dropped();
        assert!(walk.behind());
        assert_eq!(heard(&mut walk, Duration::ZERO, at + BATCH), None);
        assert!(walk.behind(), "the clear pongs before a drop do not span it");
        let (moved, _) = hearings(&mut walk, 5, Duration::ZERO, at + BATCH);
        assert_eq!(moved, None);
        assert!(!walk.behind());
        assert_eq!(walk.bitrate(), 96_000);
    }

    /// A walk that is not adaptive pings nothing and hears nothing: the rate
    /// is the ceiling whatever the sends do, the link is never behind, and
    /// nothing is skipped.
    #[test]
    fn a_walk_that_is_not_adaptive_hears_nothing() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, false);
        assert!(!walk.adaptive());
        assert!(!walk.ping(start));
        assert_eq!(walk.pong(start + LAG_SEVERE), None);
        walk.dropped();
        walk.sent(LAG_SEVERE);
        assert_eq!(walk.tick(start + Duration::from_secs(10)), None);
        assert!(!walk.behind());
        assert!(!walk.skipping());
        assert_eq!(walk.bitrate(), 96_000);
    }

    /// Every rate the walk arrives at, down and up again, is one a running
    /// encoder takes.
    #[test]
    fn every_rate_the_walk_arrives_at_is_one_the_encoder_takes() {
        let start = Instant::now();
        let mut encoder = Encoder::new(Stream { rate: 48_000, channels: 2 }, BITRATE_MAX).unwrap();
        let mut walk = walk(BITRATE_MAX, start);
        let mut at = start;
        let mut rates = Vec::new();
        while walk.bitrate() > BITRATE_FLOOR {
            rates.extend(verdict(&mut walk, BEHIND_LAG, at));
            at += ADJUST_COOLDOWN;
        }
        rates.extend(recovery(&mut walk, 12, at));
        assert_eq!(rates.first(), Some(&340_000));
        assert_eq!(rates.last(), Some(&BITRATE_MAX));
        assert!(rates.len() > 10, "{rates:?}");
        for rate in rates {
            encoder.set_bitrate(rate).unwrap();
        }
    }
}
