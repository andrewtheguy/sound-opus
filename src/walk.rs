//! The rate a link will bear, walked under the configured one.
//!
//! The configured bitrate is a ceiling. Between it and [`BITRATE_FLOOR`] the
//! rate walks down while the link falls behind and back up while it keeps up,
//! and a ceiling at or under the floor stays where it is: nothing under it is
//! worth giving up. The signal is pressure: how long sending a packet blocked,
//! which is the socket, or the shallow queue in front of it, having no room
//! because the packets before it are still unsent. Where it is read from is
//! the caller's — the gateway's send to a browser's audio socket, say — and so
//! is what the rate the walk arrives at is done with: moved on an [`Encoder`]
//! here, or named to a remote that codes its own. Nothing of a client's own lag
//! is here: sound is small beside a picture and carries no fence, and pressure
//! at the sender is where a link too narrow for it shows first.
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
//! The walk also says whether the link is *behind* right now, for a user with
//! something free to shed while it is: a gateway that codes the sound itself
//! sheds silence before its encoder, which drains a backlog by exactly that
//! much and loses nothing anyone hears. A walk that is not adaptive — the
//! operator asked for the rate held — hears nothing in any send, however long
//! it blocked, holds the ceiling and never counts the link behind.
//!
//! Pure, and takes `now` rather than reading a clock, so every decision is
//! testable without waiting for one. The thresholds and steps are those
//! settled on shaped links for the picture, scaled to the sound's own cadence,
//! and the reasons are on each.
//!
//! [`Encoder`]: crate::Encoder

use std::time::{Duration, Instant};

use crate::{BITRATE_MAX, BITRATE_MIN};

/// How long sending a packet may block before it counts as one the link could
/// not keep up with: a packet's own length. The queue between the encoder and
/// the socket is shallow on purpose, so this stays at zero while the link has
/// room and becomes obvious the moment it does not.
const BEHIND_BLOCK: Duration = Duration::from_millis(20);

/// Blocking below which a send counts as clear. Between this and
/// [`BEHIND_BLOCK`] is hysteresis: a link hovering there earns neither a
/// lower rate nor its rate back.
const CLEAR_BLOCK: Duration = Duration::from_millis(10);

/// Blocking that is not a little behind but a lot: a step down here gives up
/// two steps of [`STEP_DOWN`], and past [`BLOCK_SEVERE`] three. A link this
/// far behind has a queue growing by the packet, and a third a step takes
/// three cooldowns to reach the floor from the default rate while the sound
/// falls seconds behind.
const BLOCK_HEAVY: Duration = Duration::from_millis(150);
/// See [`BLOCK_HEAVY`].
const BLOCK_SEVERE: Duration = Duration::from_millis(400);

/// Behind sends among the last [`VERDICT_WINDOW`] before rate is given up,
/// the latest among them. Two rather than one, so a single unlucky send — a
/// scheduler hiccup, a heartbeat mid-write — is not a verdict about the link;
/// a window rather than a run, because the pressure a link that is barely too
/// narrow shows is intermittent.
const BEHIND_SENDS: u32 = 2;
/// See [`BEHIND_SENDS`].
const VERDICT_WINDOW: u32 = 4;

/// How long the link must have been clear before rate is taken back, and the
/// fewest clear sends that span must hold: deliberately far more than
/// [`BEHIND_SENDS`]. A span rather than a count, because sends come at the
/// source's own cadence — a wave buffer five times a second from one host, a
/// packet fifty times a second from another — and a count would be seconds of
/// proof on one and a blink on the other.
const CLEAR_SPAN: Duration = Duration::from_secs(3);
/// See [`CLEAR_SPAN`].
const CLEAR_SENDS: u32 = 4;

/// How long the link must have been clear before it stops counting as behind,
/// the state that sheds. Much shorter than [`CLEAR_SPAN`]: shedding exists to
/// drain a backlog, and a second of clear sends means it has drained.
const RELIEF_SPAN: Duration = Duration::from_secs(1);

/// The least time between two moves of the rate, so a burst of slow sends is
/// one decision rather than one per packet, and the queue a step down leaves
/// has drained before the next verdict is read against it.
const ADJUST_COOLDOWN: Duration = Duration::from_secs(2);

/// The least time between a step up and the step down that walks it back. Far
/// shorter than [`ADJUST_COOLDOWN`]: a step up the link refuses shows in the
/// next few sends, and every packet it is left in place is queue.
const REFUSAL_COOLDOWN: Duration = Duration::from_millis(500);

/// How far one step down moves the rate on a link a little behind: to this
/// fraction of itself, a third given up. Bigger down than up, for the same
/// reason [`CLEAR_SENDS`] is bigger than [`BEHIND_SENDS`].
const STEP_DOWN: (u32, u32) = (2, 3);

/// What the first step up reclaims, as a fraction of the ceiling: a sixteenth,
/// 6 kbit/s under the default rate. Every step up the link takes doubles the
/// next, to [`STEP_UP_MAX`], and a step down puts it back: a link that has
/// recovered is at the ceiling again within a few steps rather than a minute.
const STEP_UP: u32 = 16;
/// The most one step up reclaims, as a fraction of the ceiling: half.
const STEP_UP_MAX: u32 = 2;

/// How long a rate the link refused stays out of reach. A step down within
/// [`REFUSAL_WINDOW`] of a step up says the link would not take the rate it
/// was just given, and the walk keeps under it for this long before probing
/// there again, with the smallest step — TCP's slow-start threshold, on the
/// rate. Without it a walk whose steps double on the way up would spend a
/// session bouncing off the same rate.
const REFUSAL_HOLD: Duration = Duration::from_secs(15);
/// See [`REFUSAL_HOLD`].
const REFUSAL_WINDOW: Duration = Duration::from_secs(4);

/// The adaptive floor, in bits per second: where the walk stops giving rate up
/// and a link still behind is left to the sender's shedding. Fixed rather than
/// configured, as every adaptive stream's is. 32 kbit/s is where Opus stereo
/// still codes the whole band at full width: degraded, but continuous, and
/// continuity is the whole point of giving rate up. Below it libopus narrows
/// the band and folds the stereo, which a listener hears as a different sound
/// rather than a worse one, and halving the rate again saves a few kilobits a
/// second of a link that is already dropping a picture's megabits. A ceiling
/// at or under it is never moved.
pub const BITRATE_FLOOR: u32 = 32_000;
const _: () = assert!(BITRATE_MIN < BITRATE_FLOOR && BITRATE_FLOOR < crate::BITRATE_DEFAULT);

/// See the [module](self).
#[derive(Debug)]
pub struct BitrateWalk {
    /// The configured rate: the most this ever asks for.
    ceiling: u32,
    /// Whether the link's pressure moves this walk at all.
    adaptive: bool,
    /// The rate in force.
    bitrate: u32,
    /// Whether the link is behind: a send blocked, and the link has not been
    /// clear for [`RELIEF_SPAN`] since.
    behind: bool,
    /// The last [`VERDICT_WINDOW`] verdicts, newest in the low bit, a set bit
    /// for a send the link was behind on.
    verdicts: u8,
    /// The clear sends since the link was last behind: when the first came,
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
}

impl BitrateWalk {
    /// A walk that starts at `ceiling` bits per second and goes down to
    /// [`BITRATE_FLOOR`]; `adaptive` is whether the link's pressure moves it,
    /// and without it the walk holds the ceiling.
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
        }
    }

    /// The configured rate, which the walk never goes above.
    pub fn ceiling(&self) -> u32 {
        self.ceiling
    }

    /// Whether the link's pressure moves this walk.
    pub fn adaptive(&self) -> bool {
        self.adaptive
    }

    /// The rate in force, in bits per second.
    pub fn bitrate(&self) -> u32 {
        self.bitrate
    }

    /// Whether the link is behind right now: a send blocked, and the link has
    /// not been clear for a second since. What a sender with something free to
    /// shed — silence, before an encoder — sheds it on. Never on a walk that
    /// is not adaptive.
    pub fn behind(&self) -> bool {
        self.behind
    }

    /// A packet took `blocked` to send: time the socket, or the queue before
    /// it, had no room for it. Returns the rate to code the next packet at if
    /// the walk moved. Nothing, ever, on a walk that is not adaptive.
    pub fn sent(&mut self, blocked: Duration, now: Instant) -> Option<u32> {
        if !self.adaptive {
            return None;
        }
        let behind = blocked >= BEHIND_BLOCK;
        self.verdicts = ((self.verdicts << 1) | u8::from(behind)) & ((1 << VERDICT_WINDOW) - 1);
        if behind {
            self.behind = true;
            self.clear = None;
        } else if blocked <= CLEAR_BLOCK {
            // Between the two thresholds a send neither counts nor ends the
            // run: not evidence of room, not evidence against it either.
            let (since, count) = self.clear.map_or((now, 1), |(since, count)| (since, count + 1));
            self.clear = Some((since, count));
            if now.saturating_duration_since(since) >= RELIEF_SPAN {
                self.behind = false;
            }
        }
        // Walking back a step up the link refused does not wait out the full
        // cooldown: the refusal is in the next few sends, and every one is
        // queue. Only while that step is the last move, though.
        let cooldown = if behind && self.reclaimed.is_some_and(|(_, at)| self.changed_at == Some(at)) {
            REFUSAL_COOLDOWN
        } else {
            ADJUST_COOLDOWN
        };
        if self.changed_at.is_some_and(|at| now.saturating_duration_since(at) < cooldown) {
            return None;
        }
        let moved = if behind && self.verdicts.count_ones() >= BEHIND_SENDS {
            self.give_up(blocked, now)
        } else if self.clear.is_some_and(|(since, count)| count >= CLEAR_SENDS && now.saturating_duration_since(since) >= CLEAR_SPAN) {
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

    /// Give rate up, by more the longer the send blocked. `false` when there
    /// is nothing left to give.
    fn give_up(&mut self, blocked: Duration, now: Instant) -> bool {
        let steps = if blocked >= BLOCK_SEVERE {
            3
        } else if blocked >= BLOCK_HEAVY {
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

    /// The cadence sends come at from a host that hands over a wave buffer
    /// five times a second.
    const SEND: Duration = Duration::from_millis(200);
    /// Clear sends at that cadence that make up a [`CLEAR_SPAN`], and then one.
    const CLEAR_RUN: u32 = 16;

    /// `count` sends `blocked` long, [`SEND`] apart from `at`: the last move,
    /// and when they ended.
    fn sends(walk: &mut BitrateWalk, count: u32, blocked: Duration, mut at: Instant) -> (Option<u32>, Instant) {
        let mut moved = None;
        for _ in 0..count {
            at += SEND;
            if let Some(bitrate) = walk.sent(blocked, at) {
                moved = Some(bitrate);
            }
        }
        (moved, at)
    }

    /// Every move of `walk` under `count` clear runs, each a cooldown apart.
    fn recovery(walk: &mut BitrateWalk, count: u32, mut at: Instant) -> Vec<u32> {
        let mut seen = Vec::new();
        for _ in 0..count {
            let (moved, then) = sends(walk, CLEAR_RUN, Duration::ZERO, at);
            seen.extend(moved);
            at = then + ADJUST_COOLDOWN;
        }
        seen
    }

    #[test]
    fn a_clear_link_stays_on_the_ceiling() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(BITRATE_DEFAULT, true);
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

    #[test]
    fn falling_behind_gives_rate_up_down_to_the_floor_once_a_cooldown() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        assert_eq!(walk.sent(BEHIND_BLOCK, start), None, "one slow send is not a verdict");
        assert!(walk.behind(), "but it does put the link behind");
        assert_eq!(walk.sent(BEHIND_BLOCK, start), Some(64_000), "a third given up");
        for _ in 0..4 {
            assert_eq!(walk.sent(BEHIND_BLOCK, start + ADJUST_COOLDOWN / 2), None, "inside the cooldown");
        }
        let later = start + ADJUST_COOLDOWN;
        assert_eq!(walk.sent(BEHIND_BLOCK, later), Some(42_666));
        let later = later + ADJUST_COOLDOWN;
        walk.sent(BEHIND_BLOCK, later);
        assert_eq!(walk.sent(BEHIND_BLOCK, later), Some(BITRATE_FLOOR), "stops at the floor");
        let later = later + ADJUST_COOLDOWN;
        walk.sent(BLOCK_SEVERE, later);
        assert_eq!(walk.sent(BLOCK_SEVERE, later), None, "nothing left to give");
        assert_eq!(walk.bitrate(), BITRATE_FLOOR);
        assert!(walk.behind());
    }

    /// A ceiling at or under the floor has nothing to give: the link behind
    /// is left to the sender's shedding, and the rate is the one asked for.
    #[test]
    fn a_ceiling_under_the_floor_is_never_moved() {
        let start = Instant::now();
        for ceiling in [BITRATE_FLOOR, BITRATE_FLOOR / 2, BITRATE_MIN] {
            let mut walk = BitrateWalk::new(ceiling, true);
            let (moved, _) = sends(&mut walk, 20, BLOCK_SEVERE, start);
            assert_eq!(moved, None, "{ceiling}");
            assert_eq!(walk.bitrate(), ceiling);
            assert!(walk.behind(), "the sender still hears that the link is behind");
        }
    }

    #[test]
    fn a_link_far_behind_gives_up_more_at_once() {
        let start = Instant::now();
        let mut heavy = BitrateWalk::new(96_000, true);
        heavy.sent(BLOCK_HEAVY, start);
        assert_eq!(heavy.sent(BLOCK_HEAVY, start), Some(42_666), "two thirds twice");
        let mut severe = BitrateWalk::new(96_000, true);
        severe.sent(BLOCK_SEVERE, start);
        assert_eq!(severe.sent(BLOCK_SEVERE, start), Some(BITRATE_FLOOR), "three, stopped by the floor");
        let mut high = BitrateWalk::new(300_000, true);
        high.sent(BLOCK_SEVERE, start);
        assert_eq!(high.sent(BLOCK_SEVERE, start), Some(88_888));
    }

    /// Two behind sends among four are a verdict, as a link barely too
    /// narrow gives them: one slow, one clear, one slow.
    #[test]
    fn intermittent_pressure_is_still_a_verdict() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        assert_eq!(walk.sent(BEHIND_BLOCK, start), None);
        assert_eq!(walk.sent(Duration::ZERO, start + SEND), None);
        assert_eq!(walk.sent(BEHIND_BLOCK, start + 2 * SEND), Some(64_000));
    }

    /// The rate comes back in steps that double, from a sixteenth of the
    /// ceiling to half of it, after a clear span each, and never past the
    /// ceiling.
    #[test]
    fn rate_comes_back_slowly_then_faster_and_never_past_the_ceiling() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        let mut at = start;
        for _ in 0..3 {
            walk.sent(BEHIND_BLOCK, at);
            walk.sent(BEHIND_BLOCK, at);
            at += ADJUST_COOLDOWN;
        }
        assert_eq!(walk.bitrate(), BITRATE_FLOOR);
        let (moved, at) = sends(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at);
        assert_eq!(moved, None, "half a clear span is not enough");
        assert_eq!(recovery(&mut walk, 6, at), [38_000, 50_000, 74_000, 96_000]);
        assert_eq!(walk.bitrate(), 96_000);
    }

    /// A span of clear sends is what earns rate back, not a count of them: a
    /// host that sends once a second proves its link in the same three seconds.
    #[test]
    fn a_clear_span_is_enough_however_slowly_the_sends_come() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        walk.sent(BEHIND_BLOCK, start);
        assert_eq!(walk.sent(BEHIND_BLOCK, start), Some(64_000));
        let mut at = start + ADJUST_COOLDOWN;
        let mut moved = None;
        for _ in 0..CLEAR_SENDS {
            at += Duration::from_secs(1);
            moved = moved.or(walk.sent(Duration::ZERO, at));
        }
        assert_eq!(moved, Some(70_000));
        let mut quick = BitrateWalk::new(96_000, true);
        quick.sent(BEHIND_BLOCK, start);
        quick.sent(BEHIND_BLOCK, start);
        let at = start + ADJUST_COOLDOWN;
        let (moved, _) = sends(&mut quick, 2 * CLEAR_SENDS, Duration::ZERO, at);
        assert_eq!(moved, None, "a burst of clear sends spans no time");
    }

    /// A step up the link refuses — it falls behind within seconds of it — is
    /// walked back without the full cooldown, to the rate the link bore, and
    /// the walk comes back no further than halfway towards the refused rate
    /// until the hold is over, when it probes past it with the smallest step.
    #[test]
    fn a_refused_step_up_is_walked_back_and_held_out_of_reach() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        let mut at = start;
        for _ in 0..3 {
            walk.sent(BEHIND_BLOCK, at);
            walk.sent(BEHIND_BLOCK, at);
            at += ADJUST_COOLDOWN;
        }
        let (moved, at) = sends(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, Some(38_000));
        let at = at + ADJUST_COOLDOWN;
        let (moved, at) = sends(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, Some(50_000));
        // Refused: behind within the window of that step, and walked back
        // after the refusal's short cooldown rather than the full one.
        let at = at + REFUSAL_COOLDOWN;
        assert_eq!(walk.sent(BEHIND_BLOCK, at), None);
        assert_eq!(walk.sent(BEHIND_BLOCK, at), Some(38_000), "back to the rate the link bore");
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
        let mut walk = BitrateWalk::new(96_000, true);
        walk.sent(BEHIND_BLOCK, start);
        assert_eq!(walk.sent(BEHIND_BLOCK, start), Some(64_000));
        let at = start + ADJUST_COOLDOWN;
        let (moved, at) = sends(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, Some(70_000));
        let at = at + REFUSAL_WINDOW + SEND;
        walk.sent(BEHIND_BLOCK, at);
        assert_eq!(walk.sent(BEHIND_BLOCK, at), Some(46_666), "a third of the rate it held");
    }

    /// Between clear and behind is a band that is evidence of neither: sends
    /// there neither give rate up nor earn it back, and do not end a clear run.
    #[test]
    fn the_hysteresis_band_neither_gives_up_nor_takes_back() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        walk.sent(BEHIND_BLOCK, start);
        assert_eq!(walk.sent(BEHIND_BLOCK, start), Some(64_000));
        let at = start + ADJUST_COOLDOWN;
        let between = (CLEAR_BLOCK + BEHIND_BLOCK) / 2;
        let (moved, at) = sends(&mut walk, 4 * CLEAR_RUN, between, at);
        assert_eq!(moved, None);
        assert_eq!(walk.bitrate(), 64_000);
        // Clear sends either side of one in the band are one run.
        let (moved, at) = sends(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at);
        assert_eq!(moved, None);
        assert_eq!(walk.sent(between, at + SEND), None);
        let (moved, _) = sends(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at + SEND);
        assert_eq!(moved, Some(70_000));
    }

    /// The link is behind from the first slow send and clear again after a
    /// second of clear ones, well before any rate comes back.
    #[test]
    fn behind_rises_on_the_first_slow_send_and_clears_after_a_second() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, true);
        assert!(!walk.behind());
        walk.sent(BEHIND_BLOCK, start);
        assert!(walk.behind());
        let at = start + SEND;
        assert_eq!(walk.sent(Duration::ZERO, at), None);
        assert!(walk.behind(), "one clear send is not relief");
        let (moved, at) = sends(&mut walk, 4, Duration::ZERO, at);
        assert_eq!(moved, None);
        assert!(walk.behind(), "{:?} of clear sends is not a second", 4 * SEND);
        let (moved, _) = sends(&mut walk, 2, Duration::ZERO, at);
        assert_eq!(moved, None, "no rate came back");
        assert!(!walk.behind());
    }

    /// A walk that is not adaptive hears nothing: the rate is the ceiling
    /// whatever the sends do, and the link is never behind.
    #[test]
    fn a_walk_that_is_not_adaptive_hears_nothing() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000, false);
        assert!(!walk.adaptive());
        let (moved, at) = sends(&mut walk, 50, BLOCK_SEVERE, start);
        assert_eq!(moved, None);
        assert!(!walk.behind());
        let (moved, _) = sends(&mut walk, 50, Duration::ZERO, at + ADJUST_COOLDOWN);
        assert_eq!(moved, None);
        assert_eq!(walk.bitrate(), 96_000);
    }

    /// Every rate the walk arrives at, down and up again, is one a running
    /// encoder takes.
    #[test]
    fn every_rate_the_walk_arrives_at_is_one_the_encoder_takes() {
        let start = Instant::now();
        let mut encoder = Encoder::new(Stream { rate: 48_000, channels: 2 }, BITRATE_MAX).unwrap();
        let mut walk = BitrateWalk::new(BITRATE_MAX, true);
        let mut at = start;
        let mut rates = Vec::new();
        while walk.bitrate() > BITRATE_FLOOR {
            walk.sent(BEHIND_BLOCK, at);
            rates.extend(walk.sent(BEHIND_BLOCK, at));
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
