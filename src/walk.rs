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
//! grows rather than once it has drained. Every batch sent is told
//! ([`BitrateWalk::sent`]): how long its write blocked is heard too, as the
//! longer of the two, since a socket takes a lot before a write blocks but a
//! sender whose pongs some proxy answers still hears a link that will not
//! take its sound; and what it carried says whether the sound behind a ping
//! was quiet, which a clear pong then proves nothing about. Pings are
//! spaced ([`PING_SPACING`]), since a passed stream's batches come every
//! 20 ms and a ping costs a link as much as a packet of silence. The ping,
//! the socket and what the rate the walk arrives at is done with — moved on
//! an [`Encoder`] here, or named to a remote that codes its own — are the
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
//! goal. Quick to give rate up — by more the further behind the link is, and
//! again sooner while it stays far behind — and slow to take it back, in
//! steps that double while the link keeps taking them, come sooner while it
//! shows nothing queued behind sound, and stop short of a rate it refused,
//! so a link that is intermittently bad settles at a rate it can hold rather
//! than oscillating around one it cannot.
//! The floor is fixed rather than configured, as every adaptive stream's is:
//! past it a lower rate buys nothing a listener would call the same sound, and
//! a configured floor was a key with one right value.
//!
//! The walk also says what a sender may drop to catch up. While the link is
//! *behind*, silence is free to shed before the encoder: it drains a backlog by
//! exactly that much and loses nothing anyone hears. A lag past 200 ms is
//! sound queued that would be heard that late — at the floor any lag
//! behind, no rate being left to drain it — and the walk is
//! *skipping* ([`BitrateWalk::skipping`]): the sender drops what comes next
//! and pings back to back until a pong says the queue has drained, so what is
//! heard comes back to live rather than staying behind for as long as a lower
//! rate takes to drain it. A lag that does not drain while nothing is written
//! is not this sound's — another stream's queue at the bottleneck, a longer
//! route — and becomes the round trip it is measured over, so the walk
//! neither skips nor gives rate up for what neither would drain. A sender
//! that dropped sound it could not send tells the walk so
//! ([`BitrateWalk::dropped`]), and the link is behind from that alone. Every
//! walk walks: there is no holding the ceiling, which a link that cannot carry
//! it would only hear as sound arriving later and later.
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
/// walked the rate from zdesk's 192 to 32–43 kbit/s, and at 60 and 80 ms up
/// to 100 ms of it walked the rate to 39–57 and 48–88, where this held 192
/// through both. 150 ms lost three times the sound where a link narrowed,
/// swung or dipped, the queue let grow that much longer before the rate
/// moved.
const BEHIND_LAG: Duration = Duration::from_millis(100);

/// Lag below which the link counts as clear. Between this and [`BEHIND_LAG`]
/// is hysteresis: a link hovering there earns neither a lower rate nor its
/// rate back. 25 ms measured within 1% of it.
const CLEAR_LAG: Duration = Duration::from_millis(50);

/// Lag that is not a little behind but a lot: a step down here gives up two
/// steps of [`STEP_DOWN`], and past [`LAG_SEVERE`] three. A queue this long
/// is a link far narrower than the rate, and every packet coded at the rate in
/// the meantime is more of it. One step whatever the lag lost 27% more sound
/// where a link narrowed, swung or dipped, and 300 and 800 ms in place of
/// these 23%.
const LAG_HEAVY: Duration = Duration::from_millis(150);
/// See [`LAG_HEAVY`].
const LAG_SEVERE: Duration = Duration::from_millis(400);

/// Lag past which the sound queued is skipped: what comes next is dropped
/// before the encoder, and pings written back to back, until one comes back
/// under [`BEHIND_LAG`], so the queue drains while nothing joins it and what
/// is heard is live again. Under it a lag is left to the rate to drain, but
/// at the floor, where no rate is left to. Without skipping, a link under
/// the floor had the sound 16 to 41 s late at the 95th percentile, where
/// this keeps it to 0.4 to 0.8 s; 300 ms lost 2% less and was heard 2%
/// later, 150 ms lost 7% more for 3% sooner.
const SKIP_LAG: Duration = Duration::from_millis(200);

/// How often a ping still out is heard, once it is this far past the least
/// round trip: as lagging by as long as it has been out over that, which it
/// is at least. A queue growing faster than its pongs come back is heard as
/// it grows: waiting for the pong instead, a link that fell from 1000 kbit/s
/// to under the floor had the sound up to 13 s late at the 95th percentile,
/// and over every case 1.7 times as late as this, with 30% more lost. 200 ms
/// lost 3% more sound, 43% more where a link narrowed, swung or dipped, and
/// 50 ms 1% less but 8% more there.
pub const OVERDUE: Duration = Duration::from_millis(100);

/// The least time between two pings while the walk is not skipping. A ping
/// is asked for ahead of every batch, and a passed stream's come every 20 ms
/// or every 60 batched, so one went every round trip there, about 32 bytes
/// each in its record: 4 to 6 kbit/s at a 40 ms round trip, a sixth of the
/// sound at the floor. Under the cadence an RDP host's buffers set, 186 ms,
/// so nothing moves on that stream. A quarter fewer pings over every case,
/// 4% less stalled for the same sound lost, and between the floors
/// wlshare's speech loses 7.0 s of the minute against 8.3. 150 and 180 ms
/// write 39% and 44% fewer, but a passed stream is then heard that much
/// less often: under half-second dips wlshare's speech held 139 kbit/s
/// against 160, and 9% and 13% more was lost where a link narrowed, swung
/// or dipped. While skipping, pings are the probe that ends it, written as
/// each pong is back.
pub const PING_SPACING: Duration = Duration::from_millis(100);

/// Behind hearings among the last [`VERDICT_WINDOW`] before rate is given up,
/// the latest among them. Two rather than one, so a single late pong is not
/// a verdict about the link; a window rather than a run, because the lag a
/// link barely too narrow shows is intermittent. Three lost 25% more sound
/// where a link narrowed, swung or dipped.
const BEHIND_HEARINGS: u32 = 2;
/// See [`BEHIND_HEARINGS`].
const VERDICT_WINDOW: u32 = 4;

/// How long the link must have been clear before rate is taken back, and the
/// fewest clear hearings that span must hold: deliberately far more than
/// [`BEHIND_HEARINGS`]. A span rather than a count, because pongs come at the
/// sound's own cadence, a wave buffer five times a second from one host, a
/// packet fifty times a second from another. 2 s moved the rate 22% more
/// often and lost 54% more sound where a link narrowed, swung or dipped; 5 s
/// 17% less often and 7% less lost, but kept speech at 110 to 138 kbit/s
/// where half-second dips every ten seconds — a picture's keyframes on the
/// same link — leave it 160 to 163.
const CLEAR_SPAN: Duration = Duration::from_secs(3);
/// See [`CLEAR_SPAN`].
const CLEAR_HEARINGS: u32 = 4;

/// Lag at or under which a clear hearing shows nothing queued. A step up the
/// link took, every hearing since this clear and none of them behind quiet
/// sound ([`QUIET_FRACTION`]), is followed by the next after [`CLIMB_SPAN`]
/// rather than [`CLEAR_SPAN`]: a link that has recovered shows nothing
/// queued, where one that is only keeping up shows a little on most pongs
/// and climbs at the pace it did. Under half-second dips every ten seconds
/// the rate is 155 to 163 kbit/s where three seconds a step left it at 103
/// to 151, and on a longer route 9 to 14 kbit/s higher; over every case 2%
/// higher for 0.4% more sound lost, and where a link narrowed, swung or
/// dipped 3% higher for 5% more lost. A span of 500 ms moved the rate 16%
/// more often for the same rate, 1.5 s gained 6 kbit/s less under the dips,
/// and 10 or 25 ms of idle lag measured as this.
const LAG_IDLE: Duration = Duration::from_millis(15);
/// See [`LAG_IDLE`].
const CLIMB_SPAN: Duration = Duration::from_secs(1);

/// The fraction of the rate's share under which what was sent between two
/// pings was quiet: silence, or next to it, which costs a few bytes a packet
/// whatever the rate. A pong behind it is heard as clear as any, but says
/// nothing of room at the rate, so it is no hearing with nothing queued: a
/// reading paused four seconds in every ten had the climb step the rate past
/// a link narrowed to 100 kbit/s in every pause, 121 to 169 in 1.3 s, and
/// the sound that resumed was skipped, 3.2 s of the minute against 0.9; over
/// every case where a link narrowed, swung or dipped 61% more was lost. A
/// quarter or an eighth in place of a half lost 2% more over every case, the
/// floor case's speech over wlshare 10.5 s against 8.3.
const QUIET_FRACTION: u32 = 2;

/// How long the link must have been clear before it stops counting as behind,
/// the state that sheds. Much shorter than [`CLEAR_SPAN`]: shedding exists to
/// drain a backlog, and a second of clear hearings means it has drained.
const RELIEF_SPAN: Duration = Duration::from_secs(1);

/// The least time between two moves of the rate, so a burst of late pongs is
/// one decision rather than one per pong, and the queue a step down leaves
/// has drained before the next verdict is read against it. 3 s measured
/// within 1% of it over every case; 1 s moved the rate 8% more often for 4%
/// more sound lost where a link narrowed, swung or dipped.
const ADJUST_COOLDOWN: Duration = Duration::from_secs(2);

/// The least time between a step up and the step down that walks it back. Far
/// shorter than [`ADJUST_COOLDOWN`]: a step up the link refuses shows in the
/// next few pongs, and every packet it is left in place is queue.
const REFUSAL_COOLDOWN: Duration = Duration::from_millis(500);

/// The least time between two steps down while the link is [`LAG_HEAVY`]
/// behind or more: every packet coded at a rate that far over the link is
/// that much more queue. Half of [`ADJUST_COOLDOWN`], as wlshare's walk has
/// it: 5% less sound lost over every case and 12% less where a link
/// narrowed, swung or dipped, heard 3% sooner; between the floors speech
/// over RDP loses 4.6 s of the minute against 8.9. Half a second loses as
/// much, 18% less where a link narrowed, at 94 to 96 kbit/s on the narrowest
/// link against 102 to 106; 1.5 s 2% less.
const HEAVY_COOLDOWN: Duration = Duration::from_secs(1);

/// How far one step down moves the rate on a link a little behind: to this
/// fraction of itself, a third given up. Bigger down than up, for the same
/// reason [`CLEAR_HEARINGS`] is bigger than [`BEHIND_HEARINGS`]. A quarter
/// lost 25% more sound where a link narrowed, swung or dipped.
const STEP_DOWN: (u32, u32) = (2, 3);

/// What the first step up reclaims, as a fraction of the ceiling: a sixteenth,
/// 12 kbit/s under zdesk's 192. Every step up the link takes doubles the
/// next, to [`STEP_UP_MAX`], and a step down puts it back: a link that has
/// recovered is at the ceiling again within a few steps rather than a minute.
/// An eighth lost 32% more sound where a link narrowed, swung or dipped; a
/// thirty-second the same, for 1% less rate and 15% more moves.
const STEP_UP: u32 = 16;
/// The most one step up reclaims, as a fraction of the ceiling: half.
const STEP_UP_MAX: u32 = 2;

/// How long a rate the link refused stays out of reach. A step down within
/// [`REFUSAL_WINDOW`] of a step up says the link would not take the rate it
/// was just given, and the walk keeps under it for this long before probing
/// there again, with the smallest step — TCP's slow-start threshold, on the
/// rate. Without it a walk whose steps double on the way up would spend a
/// session bouncing off the same rate. 5 s moved the rate 19% more often;
/// 30 s 11% less often and lost 7% less where a link narrowed, swung or
/// dipped, but held the rate at 128 to 164 kbit/s after a link recovered,
/// where this is back at 144 to 174.
const REFUSAL_HOLD: Duration = Duration::from_secs(15);
/// See [`REFUSAL_HOLD`].
const REFUSAL_WINDOW: Duration = Duration::from_secs(4);

/// The adaptive floor, in bits per second: where the walk stops giving rate up
/// and a link still behind is left to skipping. Fixed rather than configured,
/// as every adaptive stream's is. 24 kbit/s is the lowest of the rates
/// measured, 12, 16, 24 and 32, at which libopus still codes a stereo stream
/// as stereo and with every band the sound has (`docs/benchmarks.md`):
/// degraded, but continuous, and continuity is the whole point of giving
/// rate up. At 16 every packet is mono, the side gone, and at 12 the band
/// narrows too, which a listener hears as a different sound rather than a
/// worse one. Against 32, 24 gives up 0.27 dB of log-spectral distance and
/// 1.6 dB of segmental SNR over the recordings, and where a link carries
/// 28 kbit/s of sound 5 to 11 s of the minute is skipped, against 13 to 21 s
/// at 32, which lost 37% more sound over every case. 16 would lose 30% less
/// than this, in mono. A ceiling at or under it is never moved.
pub const BITRATE_FLOOR: u32 = 24_000;
const _: () = assert!(BITRATE_MIN < BITRATE_FLOOR && BITRATE_FLOOR < crate::BITRATE_DEFAULT);

/// See the [module](self).
#[derive(Debug)]
pub struct BitrateWalk {
    /// The configured rate: the most this ever asks for.
    ceiling: u32,
    /// The rate in force.
    bitrate: u32,
    /// Whether the link is behind: a hearing behind or the sender dropped
    /// sound, and the link has not been clear for [`RELIEF_SPAN`] since.
    behind: bool,
    /// The last [`VERDICT_WINDOW`] verdicts, newest in the low bit, a set bit
    /// for a hearing the link was behind on.
    verdicts: u8,
    /// The clear hearings since the link was last behind: when the first came,
    /// how many, and the most lag heard since, a hearing in the band between
    /// clear and behind among them, `None` once one came behind quiet sound.
    clear: Option<(Instant, u32, Option<Duration>)>,
    /// Whether the last move was a step down: what reads a link still far
    /// behind as one to look at again sooner ([`HEAVY_COOLDOWN`]).
    stepped_down: bool,
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
    /// The ping in flight: when it was written, when the walk last heard it
    /// overdue, and whether what was sent since the ping before was quiet.
    ping: Option<(Instant, Option<Instant>, bool)>,
    /// When the last ping was written, for [`PING_SPACING`].
    pinged: Option<Instant>,
    /// What was sent since the last ping: packets, and their bytes.
    carried: (u32, usize),
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
    /// [`BITRATE_FLOOR`] as the link's lag moves it.
    ///
    /// `ceiling` is clamped to what an encoder takes, [`BITRATE_MIN`] to
    /// [`BITRATE_MAX`], so every rate the walk arrives at is one.
    pub fn new(ceiling: u32) -> Self {
        let ceiling = ceiling.clamp(BITRATE_MIN, BITRATE_MAX);
        Self {
            ceiling,
            bitrate: ceiling,
            behind: false,
            verdicts: 0,
            clear: None,
            stepped_down: false,
            changed_at: None,
            reclaim: ceiling / STEP_UP,
            reclaimed: None,
            refused: None,
            ping: None,
            pinged: None,
            carried: (0, 0),
            least: None,
            blocked: Duration::ZERO,
            skipping: None,
        }
    }

    /// The configured rate, which the walk never goes above.
    pub fn ceiling(&self) -> u32 {
        self.ceiling
    }

    /// The rate in force, in bits per second.
    pub fn bitrate(&self) -> u32 {
        self.bitrate
    }

    /// Whether the link is behind right now: a hearing was, or the sender
    /// dropped sound ([`Self::dropped`]), and the link has not been clear for
    /// a second since. What a sender with something free to shed — silence,
    /// before an encoder — sheds it on.
    pub fn behind(&self) -> bool {
        self.behind
    }

    /// Whether to write a ping now: `true` when none is in flight and the
    /// last went [`PING_SPACING`] ago or more, and the walk takes it as
    /// written at `now`. Asked ahead of every batch, and while
    /// [`Self::skipping`] at once when a pong is back, since no batch comes
    /// then and the spacing does not hold.
    pub fn ping(&mut self, now: Instant) -> bool {
        if self.ping.is_some() {
            return false;
        }
        if self.skipping.is_none() && self.pinged.is_some_and(|at| now.saturating_duration_since(at) < PING_SPACING) {
            return false;
        }
        let (packets, bytes) = std::mem::take(&mut self.carried);
        let quiet = packets > 0 && (bytes * 8 * crate::PACKETS_PER_SECOND as usize * QUIET_FRACTION as usize) < self.bitrate as usize * packets as usize;
        self.ping = Some((now, None, quiet));
        self.pinged = Some(now);
        true
    }

    /// The ping in flight came back at `now`. Returns the rate to code the
    /// next packet at if the walk moved; nothing for a pong with no ping in
    /// flight.
    pub fn pong(&mut self, now: Instant) -> Option<u32> {
        let (went, _, quiet) = self.ping.take()?;
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
        self.hear(lag, quiet, now)
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
        let (went, _, quiet) = self.ping?;
        let least = self.least?;
        self.ping = Some((went, Some(now), quiet));
        self.hear(now.saturating_duration_since(went).saturating_sub(least), quiet, now)
    }

    /// When [`Self::tick`] next has a ping to hear: `None` while none is in
    /// flight, or before any has come back to say what the round trip is.
    pub fn next_tick(&self) -> Option<Instant> {
        let (went, heard, _) = self.ping?;
        let least = self.least?;
        Some(heard.map_or(went + least + OVERDUE, |at| at + OVERDUE))
    }

    /// A batch of `packets` packets, `bytes` in all, was sent, after the
    /// socket, or the queue before it, had no room for `blocked`. The wait is
    /// heard at the next pong or tick, if longer than its lag; the bytes say
    /// whether the sound behind the next ping was quiet, under half the
    /// rate's share, which a clear pong then is no evidence of room through.
    /// Told as the pump hands a batch to the socket's writer, before the
    /// writer pings ahead of it, so the ping ahead of a batch counts that
    /// batch with the ones before: one batch off, the same in the simulator,
    /// and nothing to a flag read over spans of a second.
    pub fn sent(&mut self, blocked: Duration, packets: u32, bytes: usize) {
        self.blocked = self.blocked.max(blocked);
        self.carried.0 += packets;
        self.carried.1 += bytes;
    }

    /// Whether to skip what arrives now: drop it before the encoder, or a
    /// unit before the socket, because a lag past 200 ms says that much is
    /// queued, or at the floor a link is still behind, until a pong says it
    /// has drained.
    pub fn skipping(&self) -> bool {
        self.skipping.is_some()
    }

    /// The sender dropped sound it could not send: the link is behind by at
    /// least the queue that overflowed, whatever its lag measured, and stays
    /// so until a second of clear hearings has followed. No verdict on the
    /// rate: the lag that filled the queue was heard already.
    pub fn dropped(&mut self) {
        self.behind = true;
        self.clear = None;
    }

    /// The link lags by `lag`, or by how long a send blocked since the last
    /// hearing if that is longer, behind sound that was `quiet` or not.
    /// Returns the rate to code the next packet at if the walk moved.
    fn hear(&mut self, lag: Duration, quiet: bool, now: Instant) -> Option<u32> {
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
            // Clear behind quiet sound is no evidence of room at the rate.
            let heard = (!quiet).then_some(lag);
            let (since, count, most) = self.clear.map_or((now, 1, heard), |(since, count, most)| (since, count + 1, most.zip(heard).map(|(most, lag)| most.max(lag))));
            self.clear = Some((since, count, most));
            if now.saturating_duration_since(since) >= RELIEF_SPAN {
                self.behind = false;
            }
        } else if let Some((_, _, most)) = &mut self.clear {
            // Between the two thresholds a hearing neither counts nor ends
            // the run: not evidence of room, not evidence against it either.
            // It is queue, though, which the run then shows.
            *most = most.map(|most| most.max(lag));
        }
        // A step up the link took, with nothing queued since, is followed by
        // the next sooner.
        let climbing = self.reclaimed.is_some_and(|(_, at)| self.changed_at == Some(at)) && self.clear.is_some_and(|(.., most)| most.is_some_and(|most| most <= LAG_IDLE));
        let span = if climbing { CLIMB_SPAN } else { CLEAR_SPAN };
        // Walking back a step up the link refused does not wait out the full
        // cooldown: the refusal is in the next few pongs, and every packet is
        // queue. Only while that step is the last move, though. A link far
        // behind is looked at again sooner after a step down, every packet
        // coded over it being that much more queue.
        let cooldown = if behind && self.reclaimed.is_some_and(|(_, at)| self.changed_at == Some(at)) {
            REFUSAL_COOLDOWN
        } else if behind && lag >= LAG_HEAVY && self.stepped_down {
            HEAVY_COOLDOWN
        } else if climbing {
            CLIMB_SPAN
        } else {
            ADJUST_COOLDOWN
        };
        if self.changed_at.is_some_and(|at| now.saturating_duration_since(at) < cooldown) {
            return None;
        }
        let moved = if behind && self.verdicts.count_ones() >= BEHIND_HEARINGS {
            // At the floor a link still behind is left to skipping, whatever
            // the lag: no rate drains it, and a lag that does not drain while
            // nothing is written is the path's. Without it, a route 150 ms
            // longer held the rate at the floor for good, where this has it
            // back at the ceiling within half a minute; it loses 11% more
            // sound over every case, all of it under the floors, heard 6%
            // sooner.
            if self.bitrate <= BITRATE_FLOOR && self.skipping.is_none() {
                self.skipping = Some((now, None));
            }
            self.give_up(lag, now)
        } else if self.clear.is_some_and(|(since, count, _)| count >= CLEAR_HEARINGS && now.saturating_duration_since(since) >= span) {
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
        self.stepped_down = true;
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
        self.stepped_down = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BITRATE_DEFAULT, Encoder, Stream};

    const MS: Duration = Duration::from_millis(1);
    /// The least round trip in these tests.
    const TRIP: Duration = Duration::from_millis(40);
    /// The cadence pings go at from a host that hands over a wave buffer
    /// five times a second.
    const BATCH: Duration = Duration::from_millis(200);
    /// Clear hearings at that cadence that make up a [`CLEAR_SPAN`], and then
    /// one.
    const CLEAR_RUN: u32 = 16;
    /// Clear hearings at that cadence that make up a [`CLIMB_SPAN`], and then
    /// one.
    const CLIMB_RUN: u32 = 6;
    /// A batch of sound at the ceiling of most of these walks, 96 kbit/s:
    /// nine packets of 240 bytes.
    const LOUD: (u32, usize) = (9, 9 * 240);

    /// A batch of [`LOUD`] sound was sent, and waited `blocked`.
    fn sent(walk: &mut BitrateWalk, blocked: Duration) {
        walk.sent(blocked, LOUD.0, LOUD.1);
    }

    /// A walk from `ceiling` whose first ping, written two batches before
    /// `start`, came back in [`TRIP`].
    fn walk(ceiling: u32, start: Instant) -> BitrateWalk {
        let mut walk = BitrateWalk::new(ceiling);
        assert!(walk.ping(start - 2 * BATCH));
        assert_eq!(walk.pong(start - 2 * BATCH + TRIP), None);
        walk
    }

    /// A ping written at `at` and back `lag` past the least round trip: the
    /// move it made, if any.
    fn heard(walk: &mut BitrateWalk, lag: Duration, at: Instant) -> Option<u32> {
        sent(walk, Duration::ZERO);
        assert!(walk.ping(at), "no ping could go at {:?} after the start", at.saturating_duration_since(Instant::now()));
        walk.pong(at + TRIP + lag)
    }

    /// `count` pings `lag` late, [`BATCH`] apart after `at`: every move, and
    /// when the last ping went.
    fn hearings(walk: &mut BitrateWalk, count: u32, lag: Duration, mut at: Instant) -> (Vec<u32>, Instant) {
        let mut moved = Vec::new();
        for _ in 0..count {
            at += BATCH;
            moved.extend(heard(walk, lag, at));
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

    /// Two hearings behind, a batch apart, the last written at `at`, as a
    /// link far enough behind gives.
    fn verdict(walk: &mut BitrateWalk, lag: Duration, at: Instant) -> Option<u32> {
        heard(walk, lag, at - BATCH);
        heard(walk, lag, at)
    }

    #[test]
    fn a_clear_link_stays_on_the_ceiling() {
        let start = Instant::now();
        let mut walk = walk(BITRATE_DEFAULT, start);
        assert!(recovery(&mut walk, 4, start).is_empty());
        assert_eq!((walk.ceiling(), walk.bitrate()), (BITRATE_DEFAULT, BITRATE_DEFAULT));
        assert!(!walk.behind());
    }

    /// A ceiling outside what the encoder takes is clamped to it, so every
    /// rate the walk arrives at is one the encoder takes.
    #[test]
    fn a_ceiling_off_the_encoder_is_clamped_to_it() {
        assert_eq!(BitrateWalk::new(u32::MAX).bitrate(), BITRATE_MAX);
        assert_eq!(BitrateWalk::new(0).ceiling(), BITRATE_MIN);
    }

    /// One ping at a time: none is asked for while one is in flight, and a
    /// pong with none in flight is nothing.
    #[test]
    fn one_ping_is_in_flight_at_a_time() {
        let start = Instant::now();
        let mut walk = BitrateWalk::new(96_000);
        assert_eq!(walk.pong(start), None, "no ping in flight");
        assert!(walk.ping(start));
        assert!(!walk.ping(start + BATCH), "one is in flight");
        walk.pong(start + TRIP);
        assert!(walk.ping(start + BATCH));
    }

    /// A ping goes no sooner than [`PING_SPACING`] after the last, except
    /// while the walk is skipping, when they are the probe that ends it and
    /// go as each pong is back.
    #[test]
    fn pings_are_spaced_unless_the_walk_is_skipping() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert!(walk.ping(start));
        walk.pong(start + TRIP);
        assert!(!walk.ping(start + TRIP), "too soon after the last");
        assert!(!walk.ping(start + PING_SPACING - MS));
        assert!(walk.ping(start + PING_SPACING));
        let at = start + PING_SPACING + TRIP + SKIP_LAG;
        walk.pong(at);
        assert!(walk.skipping());
        assert!(walk.ping(at), "at once while skipping");
        walk.pong(at + TRIP);
        assert!(!walk.skipping(), "drained");
        assert!(!walk.ping(at + TRIP), "and spaced again");
        assert!(walk.ping(at + PING_SPACING));
    }

    /// The least round trip is the path's, and a lag is a round trip over it:
    /// a quicker pong later lowers it.
    #[test]
    fn the_lag_is_over_the_least_round_trip() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        let late = BEHIND_LAG;
        assert_eq!(heard(&mut walk, late, start), None);
        assert!(walk.behind(), "{late:?} over the trip is behind");
        // A trip shorter than the first: the path's is that, and the one
        // before it was lagging by the difference.
        let quick = TRIP / 2;
        let at = start + BATCH;
        assert!(walk.ping(at));
        walk.pong(at + quick);
        let at = at + BATCH;
        assert!(walk.ping(at));
        assert_eq!(walk.pong(at + quick + late), Some(64_000), "{late:?} over the new least, with the one before");
    }

    #[test]
    fn falling_behind_gives_rate_up_down_to_the_floor_once_a_cooldown() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(heard(&mut walk, BEHIND_LAG, start), None, "one late pong is not a verdict");
        assert!(walk.behind(), "but it does put the link behind");
        let at = start + BATCH;
        assert_eq!(heard(&mut walk, BEHIND_LAG, at), Some(64_000), "a third given up");
        let moved = at + TRIP + BEHIND_LAG;
        let (inside, _) = hearings(&mut walk, 4, BEHIND_LAG, at);
        assert!(inside.is_empty(), "inside the cooldown");
        let later = moved + ADJUST_COOLDOWN;
        assert_eq!(heard(&mut walk, BEHIND_LAG, later), Some(42_666));
        let later = later + ADJUST_COOLDOWN;
        assert_eq!(verdict(&mut walk, BEHIND_LAG, later), Some(28_444));
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
                sent(&mut walk, LAG_SEVERE);
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
        assert_eq!(verdict(&mut severe, LAG_SEVERE, start), Some(28_444), "three");
        let mut low = walk(64_000, start);
        assert_eq!(verdict(&mut low, LAG_SEVERE, start), Some(BITRATE_FLOOR), "stopped by the floor");
        let mut high = walk(300_000, start);
        assert_eq!(verdict(&mut high, LAG_SEVERE, start), Some(88_888));
    }

    /// A link still [`LAG_HEAVY`] behind after a step down is stepped down
    /// again after [`HEAVY_COOLDOWN`], where one a little behind waits the
    /// full cooldown.
    #[test]
    fn a_link_far_behind_is_stepped_down_again_sooner() {
        let start = Instant::now();
        let mut far = walk(BITRATE_DEFAULT, start);
        assert_eq!(verdict(&mut far, LAG_HEAVY, start), Some(42_666));
        let moved = start + TRIP + LAG_HEAVY;
        assert_eq!(verdict(&mut far, LAG_HEAVY, moved + HEAVY_COOLDOWN), Some(BITRATE_FLOOR), "still that far behind");
        let mut little = walk(BITRATE_DEFAULT, start);
        assert_eq!(verdict(&mut little, LAG_HEAVY, start), Some(42_666));
        assert_eq!(verdict(&mut little, LAG_HEAVY - MS, moved + HEAVY_COOLDOWN), None, "a little behind waits the full cooldown");
        assert_eq!(verdict(&mut little, LAG_HEAVY - MS, moved + ADJUST_COOLDOWN), Some(28_444));
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
        assert_eq!(walk.tick(first - MS), None, "early, it hears nothing");
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
        let mut walk = BitrateWalk::new(96_000);
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
        sent(&mut walk, LAG_HEAVY);
        assert_eq!(heard(&mut walk, Duration::ZERO, start), None);
        assert!(walk.behind());
        sent(&mut walk, LAG_HEAVY);
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
        let at = start + TRIP + queued;
        let first = queued - TRIP;
        assert!(walk.ping(at));
        walk.pong(at + TRIP + first);
        assert!(walk.skipping(), "{first:?} is still behind");
        let at = at + TRIP + first;
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
        walk.pong(start + TRIP + SKIP_LAG + MS);
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
        let mut at = start + BATCH + TRIP + path;
        for _ in 0..2 {
            assert!(walk.ping(at));
            at += TRIP + path;
            walk.pong(at);
        }
        assert!(!walk.skipping(), "nothing more is skipped for the path's lag");
        // The clear run began with the pong that told the path's lag apart,
        // and a step up with nothing queued since is followed by the next
        // after a climb span.
        let (moved, _) = hearings(&mut walk, CLEAR_RUN, path, at + ADJUST_COOLDOWN);
        assert_eq!(moved, [48_666, 60_666, 84_666], "and the walk climbs back on it");
        assert!(!walk.behind());
    }

    /// At the floor a link still behind is skipped under [`SKIP_LAG`] too,
    /// no rate being left to drain it, and a lag that then does not drain is
    /// the path's: a route that grew by less than would skip does not hold
    /// the walk at the floor for good.
    #[test]
    fn a_lag_still_behind_at_the_floor_is_skipped_and_told_apart() {
        let start = Instant::now();
        let mut walk = walk(32_000, start);
        let path = LAG_HEAVY;
        assert_eq!(verdict(&mut walk, path, start), Some(BITRATE_FLOOR));
        assert!(!walk.skipping(), "{path:?} above the floor is left to the rate");
        let at = start + ADJUST_COOLDOWN;
        assert_eq!(verdict(&mut walk, path, at), None, "nothing left to give");
        assert!(walk.skipping(), "at the floor it is skipped");
        let mut at = at + TRIP + path;
        for _ in 0..2 {
            assert!(walk.ping(at));
            at += TRIP + path;
            walk.pong(at);
        }
        assert!(!walk.skipping(), "nothing more is skipped for the path's lag");
        let (moved, _) = hearings(&mut walk, CLEAR_RUN, path, at + ADJUST_COOLDOWN);
        assert_eq!(moved, [26_000, 30_000], "and the walk climbs back on it");
    }

    /// The rate comes back in steps that double, from a sixteenth of the
    /// ceiling to half of it, after a clear span each, and never past the
    /// ceiling.
    #[test]
    fn rate_comes_back_slowly_then_faster_and_never_past_the_ceiling() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        let mut at = start;
        for _ in 0..4 {
            verdict(&mut walk, BEHIND_LAG, at);
            at += ADJUST_COOLDOWN;
        }
        assert_eq!(walk.bitrate(), BITRATE_FLOOR);
        let (moved, at) = hearings(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at);
        assert!(moved.is_empty(), "half a clear span is not enough");
        assert_eq!(recovery(&mut walk, 6, at), [30_000, 42_000, 66_000, 96_000]);
        assert_eq!(walk.bitrate(), 96_000);
    }

    /// A step up the link took, with nothing queued on any hearing since, is
    /// followed by the next after a [`CLIMB_SPAN`] of clear hearings; a link
    /// keeping up with a little queued climbs after a [`CLEAR_SPAN`] as
    /// before.
    #[test]
    fn the_walk_climbs_sooner_on_a_link_with_nothing_queued() {
        let start = Instant::now();
        let mut idle = walk(96_000, start);
        let mut busy = walk(96_000, start);
        let little = LAG_IDLE + MS;
        for walk in [&mut idle, &mut busy] {
            assert_eq!(verdict(walk, BEHIND_LAG, start), Some(64_000));
        }
        let at = start + ADJUST_COOLDOWN;
        let (moved, at) = hearings(&mut idle, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, [70_000], "the first step up waits the clear span");
        let (moved, at_busy) = hearings(&mut busy, CLEAR_RUN, little, at - CLEAR_RUN * BATCH);
        assert_eq!(moved, [70_000]);
        let (moved, _) = hearings(&mut idle, CLIMB_RUN, Duration::ZERO, at);
        assert_eq!(moved, [82_000], "the next comes after a climb span");
        let (moved, at_busy) = hearings(&mut busy, CLIMB_RUN, little, at_busy);
        assert!(moved.is_empty(), "{little:?} queued is not nothing");
        let (moved, _) = hearings(&mut busy, CLEAR_RUN - CLIMB_RUN, little, at_busy);
        assert_eq!(moved, [82_000], "and the link keeping up climbs at the pace it did");
    }

    /// A clear pong behind quiet sound, under a [`QUIET_FRACTION`] of the
    /// rate's share, is no hearing with nothing queued: the walk climbs at the
    /// pace it did, after a clear span, as a run of quiet pongs is still clear.
    #[test]
    fn quiet_sound_is_no_evidence_of_room_to_climb_sooner() {
        let start = Instant::now();
        let mut walk = walk(96_000, start);
        assert_eq!(verdict(&mut walk, BEHIND_LAG, start), Some(64_000));
        let at = start + ADJUST_COOLDOWN;
        let (moved, mut at) = hearings(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, [70_000]);
        // Silence, three bytes a packet, on every batch from here.
        let quiet = |walk: &mut BitrateWalk, at: Instant| {
            walk.sent(Duration::ZERO, 9, 9 * 3);
            assert!(walk.ping(at));
            walk.pong(at + TRIP)
        };
        let mut moved = Vec::new();
        for _ in 0..CLEAR_RUN {
            at += BATCH;
            moved.extend(quiet(&mut walk, at));
        }
        assert_eq!(moved, [82_000], "after a clear span, not a climb span");
        // One quiet batch among loud ones is enough to hold the climb.
        let (moved, at) = hearings(&mut walk, CLIMB_RUN - 1, Duration::ZERO, at);
        assert!(moved.is_empty());
        assert_eq!(quiet(&mut walk, at + BATCH), None, "a batch of silence among them");
        let (moved, at) = hearings(&mut walk, CLIMB_RUN, Duration::ZERO, at + BATCH);
        assert!(moved.is_empty(), "quiet once since the step, the walk waits the clear span");
        let (moved, _) = hearings(&mut walk, CLEAR_RUN - 2 * CLIMB_RUN, Duration::ZERO, at);
        assert_eq!(moved, [96_000]);
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
            at += PING_SPACING;
            assert_eq!(heard(&mut quick, Duration::ZERO, at), None, "a run of clear pongs as close as they come spans too little");
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
        for _ in 0..4 {
            verdict(&mut walk, BEHIND_LAG, at);
            at += ADJUST_COOLDOWN;
        }
        let (moved, at) = hearings(&mut walk, CLEAR_RUN, Duration::ZERO, at);
        assert_eq!(moved, [30_000]);
        let (moved, at) = hearings(&mut walk, CLIMB_RUN, Duration::ZERO, at);
        assert_eq!(moved, [42_000]);
        // Refused: behind within the window of that step, and walked back
        // after the refusal's short cooldown rather than the full one.
        let at = at + REFUSAL_COOLDOWN;
        assert_eq!(heard(&mut walk, BEHIND_LAG, at), None);
        assert_eq!(heard(&mut walk, BEHIND_LAG, at + BATCH), Some(30_000), "back to the rate the link bore");
        // Clear again: up to the cap, halfway to the refused rate, and no further.
        let at = at + BATCH + ADJUST_COOLDOWN;
        assert_eq!(recovery(&mut walk, 2, at), [36_000]);
        // The hold over, the walk probes past it, from the smallest step.
        let at = at + REFUSAL_HOLD;
        assert_eq!(recovery(&mut walk, 6, at), [42_000, 54_000, 78_000, 96_000]);
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
        assert_eq!(moved, [70_000]);
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
        assert!(moved.is_empty());
        assert_eq!(walk.bitrate(), 64_000);
        // Clear hearings either side of one in the band are one run.
        let (moved, at) = hearings(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at);
        assert!(moved.is_empty());
        assert_eq!(heard(&mut walk, between, at + BATCH), None);
        let (moved, _) = hearings(&mut walk, CLEAR_RUN / 2, Duration::ZERO, at + BATCH);
        assert_eq!(moved, [70_000]);
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
        assert!(moved.is_empty());
        assert!(walk.behind(), "{:?} of clear pongs is not a second", 4 * BATCH);
        let (moved, _) = hearings(&mut walk, 2, Duration::ZERO, at);
        assert!(moved.is_empty(), "no rate came back");
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
        assert!(moved.is_empty());
        walk.dropped();
        assert!(walk.behind());
        assert_eq!(heard(&mut walk, Duration::ZERO, at + BATCH), None);
        assert!(walk.behind(), "the clear pongs before a drop do not span it");
        let (moved, _) = hearings(&mut walk, 5, Duration::ZERO, at + BATCH);
        assert!(moved.is_empty());
        assert!(!walk.behind());
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
