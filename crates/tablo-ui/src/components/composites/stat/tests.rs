use proptest::prelude::*;

use super::{CHART_HEIGHT, CHART_PAD, CHART_WIDTH, sparkline_points};

fn parse(points: &str) -> Vec<(f64, f64)> {
    points
        .split(' ')
        .map(|point| {
            let (x, y) = point.split_once(',').expect("x,y");
            (x.parse().expect("x"), y.parse().expect("y"))
        })
        .collect()
}

#[test]
fn fewer_than_two_finite_values_draw_no_line() {
    assert_eq!(sparkline_points(&[]), None);
    assert_eq!(sparkline_points(&[3.0]), None);
    assert_eq!(sparkline_points(&[3.0, f64::NAN, f64::INFINITY]), None);
}

#[test]
fn a_flat_series_runs_across_the_middle() {
    let points = parse(&sparkline_points(&[5.0, 5.0, 5.0]).expect("a line"));
    assert!(points.iter().all(|&(_, y)| y == CHART_HEIGHT / 2.0));
}

proptest! {
    /// One point per finite value, spread from the left edge to the right, the largest value on
    /// the top line and the smallest on the bottom one, every point inside the drawing box.
    #[test]
    fn points_span_the_box_and_keep_the_order_of_values(
        values in prop::collection::vec(-1e12f64..1e12, 2..40),
    ) {
        let points = parse(&sparkline_points(&values).expect("a line"));
        prop_assert_eq!(points.len(), values.len());
        prop_assert_eq!(points[0].0, 0.0);
        prop_assert_eq!(points[points.len() - 1].0, CHART_WIDTH);
        prop_assert!(points.windows(2).all(|pair| pair[0].0 < pair[1].0));
        prop_assert!(points
            .iter()
            .all(|&(_, y)| (CHART_PAD..=CHART_HEIGHT - CHART_PAD).contains(&y)));
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        for (&value, &(_, y)) in values.iter().zip(&points) {
            if max > min && value == max {
                prop_assert_eq!(y, CHART_PAD);
            }
            if max > min && value == min {
                prop_assert_eq!(y, CHART_HEIGHT - CHART_PAD);
            }
        }
        for (pair, ys) in values.windows(2).zip(points.windows(2)) {
            // A larger value draws higher, up to the two decimals the points carry.
            if pair[0] > pair[1] + (max - min) * 1e-3 {
                prop_assert!(ys[0].1 < ys[1].1);
            }
        }
    }
}
