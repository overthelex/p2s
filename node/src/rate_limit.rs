use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_CARDS_PER_DOMAIN: usize = 10;
const PUBLISH_COOLDOWN: Duration = Duration::from_secs(60);

/// Per-domain rate limiter for card publication anti-spam (§3.5).
///
/// Enforces:
/// - Max 10 cards per verified domain
/// - 1 publish per minute cooldown per domain
pub struct RateLimiter {
    domain_card_count: HashMap<String, usize>,
    domain_last_publish: HashMap<String, Instant>,
    max_cards_per_domain: usize,
    cooldown: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitResult {
    Allowed,
    TooManyCards { domain: String, count: usize, max: usize },
    CooldownActive { domain: String, remaining_secs: u64 },
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            domain_card_count: HashMap::new(),
            domain_last_publish: HashMap::new(),
            max_cards_per_domain: MAX_CARDS_PER_DOMAIN,
            cooldown: PUBLISH_COOLDOWN,
        }
    }

    #[cfg(test)]
    pub fn with_params(max_cards: usize, cooldown: Duration) -> Self {
        Self {
            domain_card_count: HashMap::new(),
            domain_last_publish: HashMap::new(),
            max_cards_per_domain: max_cards,
            cooldown,
        }
    }

    pub fn check(&self, domain: &str) -> RateLimitResult {
        if let Some(&count) = self.domain_card_count.get(domain) {
            if count >= self.max_cards_per_domain {
                return RateLimitResult::TooManyCards {
                    domain: domain.into(),
                    count,
                    max: self.max_cards_per_domain,
                };
            }
        }

        if let Some(&last) = self.domain_last_publish.get(domain) {
            let elapsed = last.elapsed();
            if elapsed < self.cooldown {
                let remaining = self.cooldown - elapsed;
                return RateLimitResult::CooldownActive {
                    domain: domain.into(),
                    remaining_secs: remaining.as_secs() + 1,
                };
            }
        }

        RateLimitResult::Allowed
    }

    pub fn record_publish(&mut self, domain: &str) {
        *self.domain_card_count.entry(domain.to_string()).or_insert(0) += 1;
        self.domain_last_publish.insert(domain.to_string(), Instant::now());
    }

    pub fn remove_card(&mut self, domain: &str) {
        if let Some(count) = self.domain_card_count.get_mut(domain) {
            *count = count.saturating_sub(1);
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_first_publish() {
        let limiter = RateLimiter::new();
        assert_eq!(limiter.check("example.com"), RateLimitResult::Allowed);
    }

    #[test]
    fn enforces_max_cards_per_domain() {
        let mut limiter = RateLimiter::with_params(2, Duration::ZERO);
        limiter.record_publish("example.com");
        limiter.record_publish("example.com");

        assert!(matches!(
            limiter.check("example.com"),
            RateLimitResult::TooManyCards { count: 2, max: 2, .. }
        ));
    }

    #[test]
    fn different_domains_independent() {
        let mut limiter = RateLimiter::with_params(1, Duration::ZERO);
        limiter.record_publish("a.com");

        assert!(matches!(limiter.check("a.com"), RateLimitResult::TooManyCards { .. }));
        assert_eq!(limiter.check("b.com"), RateLimitResult::Allowed);
    }

    #[test]
    fn enforces_cooldown() {
        let mut limiter = RateLimiter::with_params(10, Duration::from_secs(60));
        limiter.record_publish("example.com");

        assert!(matches!(
            limiter.check("example.com"),
            RateLimitResult::CooldownActive { .. }
        ));
    }

    #[test]
    fn cooldown_expires() {
        let mut limiter = RateLimiter::with_params(10, Duration::ZERO);
        limiter.record_publish("example.com");

        assert_eq!(limiter.check("example.com"), RateLimitResult::Allowed);
    }

    #[test]
    fn remove_card_decrements_count() {
        let mut limiter = RateLimiter::with_params(2, Duration::ZERO);
        limiter.record_publish("example.com");
        limiter.record_publish("example.com");

        assert!(matches!(limiter.check("example.com"), RateLimitResult::TooManyCards { .. }));

        limiter.remove_card("example.com");
        assert_eq!(limiter.check("example.com"), RateLimitResult::Allowed);
    }

    #[test]
    fn remove_card_doesnt_underflow() {
        let mut limiter = RateLimiter::new();
        limiter.remove_card("nonexistent.com");
        assert_eq!(limiter.check("nonexistent.com"), RateLimitResult::Allowed);
    }
}
