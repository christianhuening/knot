//! Per-key login throttling — leaky bucket with capacity 5, drains 1/min.
//!
//! Tracks two independent keyspaces from the caller: IP and email. After 5
//! failures within 5 min the throttle returns `Allow::No`. Successful
//! logins do NOT touch the throttle (Plan 3 keeps it dumb on purpose).
//!
//! Time is injected via the `Clock` trait so tests don't need `tokio::time`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allow {
    /// Caller may proceed with the authentication attempt.
    Yes,
    /// Caller is over budget; return the generic "invalid credentials"
    /// response. The handler should also sleep 1 s before responding.
    No,
}

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Instant;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

const CAPACITY: u32 = 5;
const DRAIN_PER: Duration = Duration::from_secs(60); // 1 token / minute

struct Bucket {
    tokens: u32,
    last_drained: Instant,
}

impl Bucket {
    /// Returns the post-drain token count and the timestamp that should be
    /// stored as `last_drained` (only the consumed whole intervals are
    /// credited — sub-interval remainder is preserved across calls).
    fn drained(&self, now: Instant) -> (u32, Instant) {
        let elapsed = now.saturating_duration_since(self.last_drained);
        let intervals = (elapsed.as_secs() / DRAIN_PER.as_secs()) as u32;
        let remaining = self.tokens.saturating_sub(intervals);
        // Advance `last_drained` only by the consumed whole intervals.
        let advance = DRAIN_PER * intervals;
        (remaining, self.last_drained + advance)
    }
}

pub struct Throttle<C: Clock = SystemClock> {
    clock: Arc<C>,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl Throttle<SystemClock> {
    pub fn new() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            buckets: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for Throttle<SystemClock> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Clock> Throttle<C> {
    pub fn with_clock(clock: Arc<C>) -> Self {
        Self {
            clock,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Check whether `key` may attempt a login. Does NOT record a failure;
    /// the caller invokes `record_failure` after a failed credential check.
    pub fn check(&self, key: &str) -> Allow {
        let map = self.buckets.lock().expect("throttle mutex");
        let now = self.clock.now();
        let remaining = map.get(key).map_or(0, |b| b.drained(now).0);
        if remaining >= CAPACITY {
            Allow::No
        } else {
            Allow::Yes
        }
    }

    /// Record a failed login for `key`. Returns the new failure count
    /// (1..=CAPACITY, capped).
    pub fn record_failure(&self, key: &str) -> u32 {
        let mut map = self.buckets.lock().expect("throttle mutex");
        let now = self.clock.now();
        let (remaining, new_last_drained) = match map.get(key) {
            Some(b) => b.drained(now),
            None => (0, now),
        };
        let new_count = (remaining + 1).min(CAPACITY);
        map.insert(
            key.to_string(),
            Bucket {
                tokens: new_count,
                last_drained: new_last_drained,
            },
        );
        new_count
    }

    /// Reset a key (called on successful login).
    pub fn reset(&self, key: &str) {
        let mut map = self.buckets.lock().expect("throttle mutex");
        map.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClock {
        base: Instant,
        offset_nanos: std::sync::atomic::AtomicU64,
    }

    impl FakeClock {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                base: Instant::now(),
                offset_nanos: std::sync::atomic::AtomicU64::new(0),
            })
        }
        fn advance(&self, by: Duration) {
            self.offset_nanos
                .fetch_add(by.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        }
    }
    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            let off = self.offset_nanos.load(std::sync::atomic::Ordering::Relaxed);
            self.base + Duration::from_nanos(off)
        }
    }

    #[test]
    fn fresh_key_is_allowed() {
        let t = Throttle::new();
        assert_eq!(t.check("ip:1.2.3.4"), Allow::Yes);
    }

    #[test]
    fn five_failures_blocks_sixth() {
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock);
        for _ in 0..5 {
            t.record_failure("k");
        }
        assert_eq!(t.check("k"), Allow::No);
    }

    #[test]
    fn under_threshold_is_allowed() {
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock);
        for _ in 0..4 {
            t.record_failure("k");
        }
        assert_eq!(t.check("k"), Allow::Yes);
    }

    #[test]
    fn drains_one_token_per_minute() {
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock.clone());
        for _ in 0..5 {
            t.record_failure("k");
        }
        assert_eq!(t.check("k"), Allow::No);
        clock.advance(Duration::from_secs(60));
        assert_eq!(t.check("k"), Allow::Yes, "one token should have drained");
    }

    #[test]
    fn reset_clears_bucket() {
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock);
        for _ in 0..5 {
            t.record_failure("k");
        }
        t.reset("k");
        assert_eq!(t.check("k"), Allow::Yes);
    }

    #[test]
    fn slow_trickle_eventually_blocks() {
        // An attacker submitting failed logins slightly faster than the
        // drain rate (every 59 s vs. the 60 s drain interval) must
        // eventually hit the cap — the sub-minute remainders accumulate
        // across calls instead of resetting `last_drained = now` each
        // time. Under the bug, the bucket would oscillate at ~2 tokens
        // forever; under the fix it grows by 1 token every ~60 attempts.
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock.clone());
        let mut blocked = false;
        for _ in 0..200 {
            t.record_failure("k");
            if t.check("k") == Allow::No {
                blocked = true;
                break;
            }
            clock.advance(Duration::from_secs(59));
        }
        assert!(
            blocked,
            "slow-trickle attacker at 59 s spacing must eventually be blocked"
        );
    }

    #[test]
    fn drain_preserves_sub_interval_remainder() {
        // Two calls 30 s apart, then a third 30 s later (total 60 s).
        // With the bug, each call snaps `last_drained = now`, so the
        // third call sees only 30 s elapsed → no drain → tokens=3.
        // With the fix, `last_drained` stays at the original timestamp
        // until a full interval is consumed → third call sees 60 s
        // elapsed → 1 token drained → tokens=2.
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock.clone());
        assert_eq!(t.record_failure("k"), 1);
        clock.advance(Duration::from_secs(30));
        assert_eq!(t.record_failure("k"), 2);
        clock.advance(Duration::from_secs(30));
        assert_eq!(
            t.record_failure("k"),
            2,
            "sub-interval remainder must accumulate so a full minute drains 1 token"
        );
    }

    #[test]
    fn record_failure_returns_capped_count() {
        let clock = FakeClock::new();
        let t = Throttle::with_clock(clock);
        for n in 1..=5 {
            assert_eq!(t.record_failure("k"), n);
        }
        // Sixth and beyond cap at 5.
        assert_eq!(t.record_failure("k"), 5);
    }
}
