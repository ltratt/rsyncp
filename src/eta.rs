use std::{collections::VecDeque, time::Duration};

pub struct Eta {
    window: Duration,
    samples: VecDeque<Sample>,
    prev_rate: Option<f64>, // files/sec from previous run
}

impl Eta {
    pub fn from_prev_run(window: Duration, prev_paths: u64, prev_elapsed: Duration) -> Self {
        let prev_rate = if prev_paths > 0 && prev_elapsed.as_secs_f64() > 0.0 {
            Some(prev_paths as f64 / prev_elapsed.as_secs_f64())
        } else {
            None
        };
        Self {
            window,
            samples: VecDeque::with_capacity(128),
            prev_rate,
        }
    }

    pub fn update(
        &mut self,
        paths_done: u64,
        paths_known: u64,
        elapsed: Duration,
    ) -> Option<Duration> {
        // Keep at least one sample so that, when we push another, we have two, which is enough to
        // compute a rate.
        while self.samples.len() > 1
            && elapsed.saturating_sub(self.samples.front().unwrap().elapsed) > self.window
        {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample {
            paths_done,
            elapsed,
        });

        let rate = self.estimated_rate()?;
        if rate <= 0.0 || !rate.is_finite() {
            return None;
        }

        let remaining = paths_known.saturating_sub(paths_done);
        Some(Duration::from_secs_f64(remaining as f64 / rate))
    }

    fn estimated_rate(&self) -> Option<f64> {
        let front = self.samples.front()?;
        let back = self.samples.back()?;
        let win_elapsed = back.elapsed.saturating_sub(front.elapsed);
        if win_elapsed.is_zero() {
            return self.prev_rate;
        }
        let win_rate =
            back.paths_done.saturating_sub(front.paths_done) as f64 / win_elapsed.as_secs_f64();

        if let Some(prev_rate) = self.prev_rate {
            // Blend the previous and window rates: as the window becomes fuller, the current rate
            // will start to dominate, though we ensure at least a little bit of the previous rate
            // seeps through at all times.
            let through = win_elapsed.div_duration_f64(self.window).clamp(0.0, 0.9);
            Some((1.0 - through) * prev_rate + through * win_rate)
        } else {
            Some(win_rate)
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    paths_done: u64,
    /// The time since the start of execution this sample was taken.
    elapsed: Duration,
}

pub fn eta_string(d: Duration) -> String {
    let secs = d.as_secs().saturating_add(u64::from(d.subsec_nanos() > 0));

    if secs >= 24 * 60 * 60 {
        let days = secs as f64 / 86_400.0;
        format!("{days:.1} days")
    } else if secs >= 60 * 60 {
        let hours = secs as f64 / 3_600.0;
        format!("{hours:.1} hours")
    } else {
        let minutes = secs / 60;
        let seconds = secs % 60;

        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_eta_to_string() {
        assert_eq!(eta_string(Duration::ZERO), "00:00");

        // HH:MM
        assert_eq!(eta_string(Duration::from_millis(999)), "00:01");
        assert_eq!(eta_string(Duration::from_millis(1001)), "00:02");
        assert_eq!(eta_string(Duration::from_secs(61)), "01:01");
        assert_eq!(eta_string(Duration::from_secs(59 * 60 + 59)), "59:59");

        // Hours
        assert_eq!(eta_string(Duration::from_secs(60 * 60 - 1)), "59:59");
        assert_eq!(eta_string(Duration::from_secs(60 * 60)), "1.0 hours");
        assert_eq!(
            eta_string(Duration::from_secs(60 * 60 - 1) + Duration::from_nanos(1)),
            "1.0 hours"
        );
        assert_eq!(eta_string(Duration::from_secs(90 * 60)), "1.5 hours");
        assert_eq!(
            eta_string(Duration::from_secs(23 * 60 * 60 + 30 * 60)),
            "23.5 hours"
        );
        assert_eq!(
            eta_string(Duration::from_secs(24 * 60 * 60 - 1)),
            "24.0 hours"
        );

        // Days
        assert_eq!(eta_string(Duration::from_secs(24 * 60 * 60)), "1.0 days");
        assert_eq!(
            eta_string(Duration::from_secs(24 * 60 * 60 - 1) + Duration::from_nanos(1)),
            "1.0 days"
        );
        assert_eq!(eta_string(Duration::from_secs(30 * 60 * 60)), "1.2 days");
        assert_eq!(eta_string(Duration::from_secs(48 * 60 * 60)), "2.0 days");
    }
}
