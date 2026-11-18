//! Login rate limit: one token bucket per client address and one for the whole server.
//!
//! The global bucket exists because TrueNAS locks out a source address after 20 auth requests
//! in 60 s, and every login reaches the NAS from truelite's address: without it, one client
//! rotating addresses could get truelite locked out for everyone, reconnects included.
//!
//! Buckets are GCRA ("virtual scheduling"): each one is a single instant, the time at which it
//! is full again, so a bucket that is full needs no entry at all.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// `burst` attempts at once, then one more every `every`.
#[derive(Debug, Clone, Copy)]
pub struct Rate {
    pub burst: u32,
    pub every: Duration,
}

#[derive(Debug, Clone)]
pub struct LimitOptions {
    /// Per client address (IPv4 address or IPv6 /64).
    pub per_client: Rate,
    /// All clients together; keep it well below the NAS lockout rate (20 per 60 s).
    pub global: Rate,
    /// Most client addresses tracked at once. When full, a new address is refused rather than
    /// let through unlimited.
    pub max_clients: usize,
}

impl Default for LimitOptions {
    fn default() -> Self {
        Self {
            per_client: Rate {
                burst: 5,
                every: Duration::from_secs(60),
            },
            global: Rate {
                burst: 10,
                every: Duration::from_secs(6),
            },
            max_clients: 4096,
        }
    }
}

/// What a client is limited by: its IPv4 address, or the /64 prefix of its IPv6 address (one
/// host usually holds a whole /64). IPv4-mapped IPv6 addresses count as IPv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpKey {
    V4([u8; 4]),
    V6([u8; 8]),
}

impl From<IpAddr> for IpKey {
    fn from(ip: IpAddr) -> Self {
        match ip.to_canonical() {
            IpAddr::V4(v4) => IpKey::V4(v4.octets()),
            IpAddr::V6(v6) => {
                let mut prefix = [0; 8];
                prefix.copy_from_slice(&v6.octets()[..8]);
                IpKey::V6(prefix)
            }
        }
    }
}

/// The attempt was refused; the client may try again after this long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryAfter(pub Duration);

impl RetryAfter {
    /// Whole seconds, rounded up, at least 1: the `Retry-After` header value.
    pub fn secs(self) -> u64 {
        let d = self.0;
        (d.as_secs() + u64::from(d.subsec_nanos() > 0)).max(1)
    }
}

/// GCRA step: the bucket's new "full again" instant if one more attempt fits at `now`.
fn take(full_at: Instant, now: Instant, rate: Rate) -> Result<Instant, RetryAfter> {
    let start = full_at.max(now);
    let backlog = start.saturating_duration_since(now);
    let allowance = rate.every.saturating_mul(rate.burst.saturating_sub(1));
    if backlog > allowance {
        return Err(RetryAfter(backlog - allowance));
    }
    Ok(start + rate.every)
}

struct State {
    /// Client buckets that are not full; absent means full.
    clients: HashMap<IpKey, Instant>,
    global: Instant,
}

/// Shared limiter. Cheap to clone.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<(LimitOptions, Mutex<State>)>,
}

impl RateLimiter {
    pub fn new(opts: LimitOptions) -> Self {
        let state = State {
            clients: HashMap::new(),
            global: Instant::now(),
        };
        Self {
            inner: Arc::new((opts, Mutex::new(state))),
        }
    }

    /// Never held across an `.await`; every update is a single write, so poison is ignored.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner.1.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Counts one login attempt from `ip`.
    ///
    /// # Errors
    ///
    /// [`RetryAfter`] when the client's or the global bucket is empty; nothing is counted then.
    pub fn check(&self, ip: IpAddr) -> Result<(), RetryAfter> {
        self.check_at(ip, Instant::now())
    }

    /// [`check`](Self::check) at a given instant (tests).
    ///
    /// # Errors
    ///
    /// As [`check`](Self::check).
    pub fn check_at(&self, ip: IpAddr, now: Instant) -> Result<(), RetryAfter> {
        let opts = &self.inner.0;
        let key = IpKey::from(ip);
        let mut st = self.lock();
        if !st.clients.contains_key(&key) && st.clients.len() >= opts.max_clients {
            prune(&mut st.clients, now);
            if st.clients.len() >= opts.max_clients {
                tracing::warn!(max = opts.max_clients, "login rate limit table full");
                return Err(RetryAfter(opts.per_client.every));
            }
        }
        // The client's own bucket first: a client that is already blocked cannot drain the
        // global one.
        let client = take(
            st.clients.get(&key).copied().unwrap_or(now),
            now,
            opts.per_client,
        )?;
        let global = take(st.global, now, opts.global).inspect_err(|_| {
            tracing::warn!("global login rate limit reached");
        })?;
        st.clients.insert(key, client);
        st.global = global;
        Ok(())
    }

    /// Forgets buckets that are full again at `now`.
    pub fn prune_at(&self, now: Instant) {
        prune(&mut self.lock().clients, now);
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.lock().clients.len()
    }
}

fn prune(clients: &mut HashMap<IpKey, Instant>, now: Instant) {
    clients.retain(|_, full_at| *full_at > now);
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// Only the per-client limit matters.
    fn per_client_only() -> RateLimiter {
        RateLimiter::new(LimitOptions {
            global: Rate {
                burst: 1000,
                every: Duration::from_millis(1),
            },
            ..LimitOptions::default()
        })
    }

    #[test]
    fn five_attempts_then_one_per_minute() {
        let rl = per_client_only();
        let t = Instant::now();
        for _ in 0..5 {
            rl.check_at(ip("10.0.0.1"), t).unwrap();
        }
        let wait = rl.check_at(ip("10.0.0.1"), t).unwrap_err();
        assert_eq!(wait, RetryAfter(MIN));
        assert_eq!(wait.secs(), 60);
        // Another client is not affected.
        rl.check_at(ip("10.0.0.2"), t).unwrap();
        // One more attempt a minute later, not two.
        rl.check_at(ip("10.0.0.1"), t + MIN).unwrap();
        assert!(rl.check_at(ip("10.0.0.1"), t + MIN).is_err());
        // Full again after five quiet minutes.
        for _ in 0..5 {
            rl.check_at(ip("10.0.0.1"), t + 6 * MIN).unwrap();
        }
    }

    #[test]
    fn a_refused_attempt_is_not_counted() {
        let rl = per_client_only();
        let t = Instant::now();
        for _ in 0..5 {
            rl.check_at(ip("10.0.0.1"), t).unwrap();
        }
        for _ in 0..100 {
            assert!(rl.check_at(ip("10.0.0.1"), t).is_err());
        }
        rl.check_at(ip("10.0.0.1"), t + MIN).unwrap();
    }

    #[test]
    fn ipv6_is_limited_per_64_and_mapped_v4_is_v4() {
        let rl = per_client_only();
        let t = Instant::now();
        for i in 0..5 {
            rl.check_at(ip(&format!("2001:db8:1:2::{i}")), t).unwrap();
        }
        assert!(rl.check_at(ip("2001:db8:1:2:ffff::1"), t).is_err());
        rl.check_at(ip("2001:db8:1:3::1"), t).unwrap();

        for _ in 0..5 {
            rl.check_at(ip("192.0.2.7"), t).unwrap();
        }
        assert!(rl.check_at(ip("::ffff:192.0.2.7"), t).is_err());
    }

    #[test]
    fn the_global_bucket_limits_many_clients() {
        let rl = RateLimiter::new(LimitOptions::default());
        let t = Instant::now();
        for i in 0..10 {
            rl.check_at(ip(&format!("10.0.0.{i}")), t).unwrap();
        }
        let wait = rl.check_at(ip("10.0.1.1"), t).unwrap_err();
        assert_eq!(wait, RetryAfter(Duration::from_secs(6)));
        // A client refused by the global bucket keeps its own tokens.
        rl.check_at(ip("10.0.1.1"), t + Duration::from_secs(6))
            .unwrap();
    }

    #[test]
    fn a_full_table_refuses_new_clients_and_prune_frees_it() {
        let rl = RateLimiter::new(LimitOptions {
            max_clients: 2,
            ..LimitOptions::default()
        });
        let t = Instant::now();
        rl.check_at(ip("10.0.0.1"), t).unwrap();
        rl.check_at(ip("10.0.0.2"), t).unwrap();
        assert!(rl.check_at(ip("10.0.0.3"), t).is_err());
        // Known clients still count normally.
        rl.check_at(ip("10.0.0.1"), t).unwrap();
        // One attempt is forgotten after a minute, so room is made on demand.
        rl.check_at(ip("10.0.0.3"), t + 2 * MIN).unwrap();

        rl.prune_at(t + 10 * MIN);
        assert_eq!(rl.tracked(), 0);
    }

    #[test]
    fn retry_after_rounds_up() {
        assert_eq!(RetryAfter(Duration::from_millis(1)).secs(), 1);
        assert_eq!(RetryAfter(Duration::from_millis(5_001)).secs(), 6);
        assert_eq!(RetryAfter(Duration::ZERO).secs(), 1);
    }
}
