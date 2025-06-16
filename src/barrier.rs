use std::{num::NonZeroU32, time::Duration};

use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
};

#[derive(Debug, thiserror::Error)]
pub enum BarrierError {
    #[error("max burst must be greater than zero")]
    InvalidMaxBurst,
    #[error("replenish must be greater than zero")]
    InvalidReplenish,
}

pub struct Barrier(RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>);

impl Barrier {
    pub fn build(replenish: Duration, max_burst: u32) -> Result<Self, BarrierError> {
        let quota = Quota::with_period(replenish)
            .ok_or(BarrierError::InvalidReplenish)?
            .allow_burst(NonZeroU32::new(max_burst).ok_or(BarrierError::InvalidMaxBurst)?);
        let rate_limiter = RateLimiter::direct(quota);

        Ok(Self(rate_limiter))
    }

    pub fn jammed(&self) -> Option<Duration> {
        match self.0.check() {
            Ok(()) => None,
            Err(not_until) => {
                let now = self.0.clock().now();

                let wait_time = not_until.wait_time_from(now);

                Some(wait_time)
            }
        }
    }
}
