//! A chart's value axis: the range it spans and the round values it marks.

use crate::display::{Number, format_number};

/// About how many steps the axis divides its range into.
const STEPS: f64 = 4.0;
/// The most decimals a tick label spells.
const MAX_DECIMALS: u8 = 12;

/// The value axis over a chart's values: from a round value at or below the smallest, and at or
/// below zero, to one at or above the largest, and at or above zero, marked every round step.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Axis {
    ticks: Vec<f64>,
    decimals: u8,
}

impl Axis {
    /// The axis over `values`, `None` when there are none. Every value is finite.
    pub(super) fn over<'a>(values: impl IntoIterator<Item = &'a f64>) -> Option<Self> {
        let (min, max) = values
            .into_iter()
            .fold(None, |range: Option<(f64, f64)>, &value| {
                Some(range.map_or((value, value), |(min, max)| {
                    (min.min(value), max.max(value))
                }))
            })?;
        let (min, max) = (min.min(0.0), max.max(0.0));
        if min == max {
            // Every value is zero.
            return Some(Self::new(0.0, 1.0, 1.0));
        }
        // Halved first, so a range wider than `f64::MAX` stays finite.
        let raw = (max / 2.0 - min / 2.0) * 2.0 / STEPS;
        let magnitude = 10f64.powf(raw.log10().floor());
        let step = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|factor| factor * magnitude)
            .find(|&step| step >= raw)
            .unwrap_or(10.0 * magnitude);
        let (low, high) = ((min / step).floor(), (max / step).ceil());
        if step.is_finite() && step > 0.0 && (low * step).is_finite() && (high * step).is_finite() {
            Some(Self::new(low, high, step))
        } else {
            Some(Self {
                ticks: vec![min, max],
                decimals: 0,
            })
        }
    }

    /// Ticks at `low * step` through `high * step`, `low` and `high` whole.
    fn new(low: f64, high: f64, step: f64) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let decimals = if step >= 1.0 {
            0
        } else {
            (-step.log10().floor()).min(f64::from(MAX_DECIMALS)) as u8
        };
        // `high - low` is a handful: the range over a round step near a quarter of it.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = (high - low) as usize;
        // Whole multiples of `step`, so zero is exactly zero.
        let ticks = (0..=count)
            .map(|index| (low + index as f64) * step + 0.0)
            .collect();
        Self { ticks, decimals }
    }

    /// The values the axis marks, smallest first; the first and last are its ends.
    pub(super) fn ticks(&self) -> impl Iterator<Item = f64> + '_ {
        self.ticks.iter().copied()
    }

    /// How far down from the axis' top `value` sits, from 0 at the top end to 1 at the bottom.
    pub(super) fn share(&self, value: f64) -> f64 {
        let (low, high) = (self.ticks[0], self.ticks[self.ticks.len() - 1]);
        ((high / 2.0 - value / 2.0) / (high / 2.0 - low / 2.0)).clamp(0.0, 1.0)
    }

    /// A tick's label, with as many decimals as the step needs.
    pub(super) fn format(&self, tick: f64) -> String {
        format_number(Number::Float(tick), self.decimals)
    }
}

#[cfg(test)]
mod tests;
