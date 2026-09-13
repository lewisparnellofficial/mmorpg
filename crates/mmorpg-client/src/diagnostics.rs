use super::*;

#[derive(Resource)]
pub(crate) struct AcceptanceSmoke {
    pub(crate) enabled: bool,
    pub(crate) step: usize,
    pub(crate) timer: Timer,
}

#[derive(Resource)]
pub(crate) struct FrameTimeStats {
    pub(crate) enabled: bool,
    pub(crate) collecting: bool,
    pub(crate) samples_ms: Vec<f64>,
    pub(crate) warmup_timer: Timer,
    pub(crate) report_timer: Timer,
}

impl FrameTimeStats {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            collecting: false,
            samples_ms: Vec::with_capacity(FRAME_TIME_SAMPLE_CAPACITY),
            report_timer: Timer::from_seconds(5.0, TimerMode::Once),
            warmup_timer: Timer::from_seconds(2.0, TimerMode::Once),
        }
    }

    pub(crate) fn report(&mut self) {
        if self.samples_ms.is_empty() {
            println!("frame_time_stats samples=0");
            return;
        }
        self.samples_ms.sort_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            let index = ((self.samples_ms.len() - 1) as f64 * fraction).round() as usize;
            self.samples_ms[index]
        };
        let max = *self.samples_ms.last().expect("non-empty frame samples");
        println!(
            "frame_time_stats samples={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3}",
            self.samples_ms.len(),
            percentile(0.50),
            percentile(0.95),
            percentile(0.99),
            max,
        );
    }
}

impl AcceptanceSmoke {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            step: 0,
            timer: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}
