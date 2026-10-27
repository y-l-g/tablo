use proptest::prelude::*;

use super::Axis;

fn ticks(values: &[f64]) -> Vec<f64> {
    Axis::over(values).expect("an axis").ticks().collect()
}

#[test]
fn no_value_has_no_axis() {
    assert_eq!(Axis::over(&[]), None);
}

#[test]
fn values_that_are_all_zero_span_zero_to_one() {
    assert_eq!(ticks(&[0.0, 0.0]), [0.0, 1.0]);
}

#[test]
fn the_axis_runs_from_zero_in_round_steps() {
    assert_eq!(ticks(&[3.0, 8.0, 5.0]), [0.0, 2.0, 4.0, 6.0, 8.0]);
    assert_eq!(ticks(&[-30.0, 10.0]), [-30.0, -20.0, -10.0, 0.0, 10.0]);
}

#[test]
fn a_tick_label_spells_the_decimals_its_step_needs() {
    let axis = Axis::over(&[0.3]).expect("an axis");
    let labels: Vec<String> = axis.ticks().map(|tick| axis.format(tick)).collect();
    assert_eq!(labels, ["0.0", "0.1", "0.2", "0.3"]);
    let axis = Axis::over(&[12_000.0]).expect("an axis");
    assert_eq!(axis.format(axis.ticks().last().expect("a tick")), "15,000");
}

#[test]
fn values_wider_than_a_float_still_draw() {
    let axis = Axis::over(&[f64::MAX, f64::MIN]).expect("an axis");
    assert_eq!(axis.share(f64::MAX), 0.0);
    assert_eq!(axis.share(f64::MIN), 1.0);
}

proptest! {
    /// The axis holds every value and zero, marks zero, rises in a few even steps, and places its
    /// top end at the top of the plot and its bottom end at the bottom.
    #[test]
    fn the_axis_holds_every_value_in_a_few_even_steps(
        values in prop::collection::vec(-1e12f64..1e12, 1..40),
    ) {
        let axis = Axis::over(&values).expect("an axis");
        let ticks: Vec<f64> = axis.ticks().collect();
        prop_assert!((2..=7).contains(&ticks.len()), "{ticks:?}");
        prop_assert!(ticks.contains(&0.0), "{ticks:?}");
        let (low, high) = (ticks[0], ticks[ticks.len() - 1]);
        prop_assert!(values.iter().all(|&value| (low..=high).contains(&value)));
        let step = ticks[1] - ticks[0];
        for pair in ticks.windows(2) {
            prop_assert!(((pair[1] - pair[0]) - step).abs() <= step * 1e-9, "{ticks:?}");
        }
        prop_assert_eq!(axis.share(high), 0.0);
        prop_assert_eq!(axis.share(low), 1.0);
        prop_assert!(values.iter().all(|&value| (0.0..=1.0).contains(&axis.share(value))));
    }
}
