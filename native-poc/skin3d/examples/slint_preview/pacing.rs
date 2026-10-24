use std::time::{Duration, Instant};

#[derive(Default)]
pub struct ActiveClock {
    elapsed: Duration,
    started: Option<Instant>,
}

impl ActiveClock {
    pub fn set_running(&mut self, running: bool, now: Instant) {
        match (self.started, running) {
            (None, true) => self.started = Some(now),
            (Some(start), false) => {
                self.elapsed += now.duration_since(start);
                self.started = None;
            }
            _ => {}
        }
    }

    pub fn seconds(&self, now: Instant) -> f64 {
        (self.elapsed
            + self
                .started
                .map_or(Duration::ZERO, |start| now.duration_since(start)))
        .as_secs_f64()
    }
}

// Slint timers truncate durations to milliseconds; round up against an absolute deadline.
pub fn timer_delay(deadline: Instant, now: Instant) -> Duration {
    Duration::from_millis(
        deadline
            .saturating_duration_since(now)
            .as_nanos()
            .div_ceil(1_000_000)
            .max(1) as u64,
    )
}

pub fn next_deadline(deadline: Instant, now: Instant, period: Duration) -> Instant {
    if now.duration_since(deadline) >= period {
        now + period
    } else {
        deadline + period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_round_up_and_drop_missed_frames_without_catch_up() {
        let now = Instant::now();
        let period = Duration::from_nanos(1_000_000_000 / 60);
        assert_eq!(timer_delay(now + period, now), Duration::from_millis(17));
        assert_eq!(
            timer_delay(now + Duration::from_micros(100), now),
            Duration::from_millis(1)
        );
        assert_eq!(
            next_deadline(now, now + Duration::from_millis(1), period),
            now + period
        );
        let late = now + Duration::from_secs(2);
        assert_eq!(next_deadline(now, late, period), late + period);
    }

    #[test]
    fn hidden_time_never_advances_animation_and_repeated_events_are_idempotent() {
        let now = Instant::now();
        let mut clock = ActiveClock::default();
        clock.set_running(true, now);
        clock.set_running(true, now + Duration::from_secs(1));
        clock.set_running(false, now + Duration::from_secs(2));
        clock.set_running(false, now + Duration::from_secs(3));
        assert_eq!(clock.seconds(now + Duration::from_secs(30)), 2.0);
        clock.set_running(true, now + Duration::from_secs(30));
        assert_eq!(clock.seconds(now + Duration::from_secs(31)), 3.0);
    }
}
