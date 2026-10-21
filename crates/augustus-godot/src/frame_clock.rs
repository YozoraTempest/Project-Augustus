// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::cmp::Ordering;

const SAMPLE_COUNT: usize = 16;
const FILTER_START: usize = 2;
const FILTER_END: usize = 12;
const FILTER_COUNT: f32 = (FILTER_END - FILTER_START) as f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FrameTiming {
    pub raw_delta: f32,
    pub smoothed_delta: f32,
}

#[derive(Debug, Clone)]
pub(super) struct FrameTimeSmoother {
    previous_clock: f64,
    raw_delta: f32,
    samples: [f32; SAMPLE_COUNT],
    next_sample: usize,
}

impl Default for FrameTimeSmoother {
    fn default() -> Self {
        Self {
            previous_clock: 0.0,
            raw_delta: 0.0,
            samples: [0.0; SAMPLE_COUNT],
            next_sample: 0,
        }
    }
}

impl FrameTimeSmoother {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(&mut self, clock_value: f64) -> FrameTiming {
        let mut smoothed_delta = 0.0;
        if self.previous_clock > 0.0 {
            self.raw_delta = (clock_value - self.previous_clock) as f32;
            self.samples[self.next_sample] = self.raw_delta;
            self.next_sample = (self.next_sample + 1) % SAMPLE_COUNT;
            // Preserve the sorted-index overwrite behavior rather than turning this into a FIFO.
            self.samples
                .sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));

            let sum = self.samples[FILTER_START..FILTER_END]
                .iter()
                .fold(0.0_f32, |sum, sample| sum + sample);
            let average = sum / FILTER_COUNT;
            smoothed_delta = (self.raw_delta * 0.5 + average * 0.5).clamp(0.0, 0.5);
        }
        self.previous_clock = clock_value;

        FrameTiming {
            raw_delta: self.raw_delta,
            smoothed_delta,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FrameTimeSmoother;

    #[test]
    fn first_frame_sets_the_clock_baseline_without_changing_samples() {
        let mut clock = FrameTimeSmoother::default();

        let timing = clock.update(10.0);

        assert_eq!(timing.raw_delta, 0.0);
        assert_eq!(timing.smoothed_delta, 0.0);
        assert_eq!(clock.samples, [0.0; 16]);
        assert_eq!(clock.next_sample, 0);
    }

    #[test]
    fn sorted_window_uses_the_middle_ten_and_replaces_the_sorted_index() {
        let mut clock = FrameTimeSmoother::default();
        let mut now = 1.0;
        clock.update(now);

        let mut timing = clock.update(now + 0.001);
        now += 0.001;
        for delta_ms in 2..=16 {
            let delta = f64::from(delta_ms) / 1_000.0;
            timing = clock.update(now + delta);
            now += delta;
        }

        assert_eq!(clock.next_sample, 0);
        assert!(
            (timing.smoothed_delta - 0.009).abs() < 0.00001,
            "{timing:?}"
        );
        for (index, sample) in clock.samples.iter().enumerate() {
            let expected = if index < 8 {
                0.0
            } else {
                (index - 7) as f32 * 0.002
            };
            assert!((sample - expected).abs() < 0.00001, "{clock:?}");
        }

        timing = clock.update(now + 0.0005);
        assert_eq!(clock.next_sample, 1);
        assert!(
            (timing.smoothed_delta - 0.001275).abs() < 0.00001,
            "{timing:?}"
        );
        assert_eq!(clock.samples[0], 0.0);
        assert!((clock.samples[7] - 0.0005).abs() < 0.00001);
    }

    #[test]
    fn nonpositive_or_nan_previous_clock_resets_only_the_smoothed_output() {
        let mut clock = FrameTimeSmoother::default();
        clock.update(1.0);
        let negative = clock.update(0.0);
        assert_eq!(negative.raw_delta, -1.0);
        assert_eq!(negative.smoothed_delta, 0.0);
        let samples_after_negative = clock.samples;

        let nonpositive_previous = clock.update(5.0);
        assert_eq!(nonpositive_previous.raw_delta, -1.0);
        assert_eq!(nonpositive_previous.smoothed_delta, 0.0);
        assert_eq!(clock.samples, samples_after_negative);

        clock.update(f64::NAN);
        let samples_after_nan = clock.samples;
        let nan_previous = clock.update(6.0);
        assert!(nan_previous.raw_delta.is_nan());
        assert_eq!(nan_previous.smoothed_delta, 0.0);
        assert!(
            clock
                .samples
                .iter()
                .zip(samples_after_nan)
                .all(|(left, right)| left.to_bits() == right.to_bits())
        );
    }

    #[test]
    fn smoothed_delta_is_clamped_without_clamping_raw_delta() {
        let mut clock = FrameTimeSmoother::default();
        clock.update(1.0);

        let timing = clock.update(4.0);

        assert_eq!(timing.raw_delta, 3.0);
        assert_eq!(timing.smoothed_delta, 0.5);
    }
}
