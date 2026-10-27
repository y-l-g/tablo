//! A [`Chart`] of one or more [`Series`] over shared labels, and [`chart`], which draws it as SVG
//! the server writes.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::{
    Color, Text,
    display::{Number, format_number},
};

mod scale;

use scale::Axis;

/// How a [`Chart`] draws its series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChartKind {
    /// A line through each series' values.
    Line,
    /// A bar per value, the series of one label side by side.
    Bar,
}

impl ChartKind {
    /// Its `data-chart` hook.
    const fn name(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Bar => "bar",
        }
    }
}

/// A chart for a [`Widget`](crate::Widget)'s body: a heading, labels along the bottom, such as
/// months, and one [`Series`] of values per thing it measures. [`chart`] draws it.
///
/// ```rust
/// # use tablo_core::{Chart, Color, Series};
/// Chart::bar(["Jan", "Feb", "Mar"])
///     .heading("Orders")
///     .description("Placed and refunded, per month")
///     .series(Series::new("Placed", [120.0, 98.0, 143.0]))
///     .series(Series::new("Refunded", [4.0, 7.0, 2.0]).color(Color::Danger));
/// ```
///
/// A chart over a resource's records reads them as a widget reads any: it asks
/// [`can_list`](crate::can_list) first and reads through [`scoped_query`](crate::scoped_query).
#[derive(Clone, Debug)]
pub struct Chart {
    kind: ChartKind,
    heading: Option<Text>,
    description: Option<Text>,
    labels: Vec<Text>,
    series: Vec<Series>,
}

impl Chart {
    /// A line chart over `labels`, oldest first when they are dates.
    pub fn line(labels: impl IntoIterator<Item = impl Into<Text>>) -> Self {
        Self::new(ChartKind::Line, labels)
    }

    /// A bar chart over `labels`.
    pub fn bar(labels: impl IntoIterator<Item = impl Into<Text>>) -> Self {
        Self::new(ChartKind::Bar, labels)
    }

    fn new(kind: ChartKind, labels: impl IntoIterator<Item = impl Into<Text>>) -> Self {
        Self {
            kind,
            heading: None,
            description: None,
            labels: labels.into_iter().map(Into::into).collect(),
            series: Vec::new(),
        }
    }

    /// The title above the chart, which also captions its data table.
    #[must_use]
    pub fn heading(mut self, heading: impl Into<Text>) -> Self {
        self.heading = Some(heading.into());
        self
    }

    /// The line under the heading, such as the period the chart covers.
    #[must_use]
    pub fn description(mut self, description: impl Into<Text>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds a series, drawn after the ones added before it.
    #[must_use]
    pub fn series(mut self, series: Series) -> Self {
        self.series.push(series);
        self
    }
}

/// One thing a [`Chart`] measures: a label for its legend and a value per label of the chart.
#[derive(Clone, Debug)]
pub struct Series {
    label: Text,
    values: Vec<f64>,
    color: Option<Color>,
}

impl Series {
    /// The series `label`, with one value per label of the chart, in the same order. A value
    /// past the chart's last label is dropped; a missing or non-finite value leaves a gap.
    pub fn new(label: impl Into<Text>, values: impl IntoIterator<Item = f64>) -> Self {
        Self {
            label: label.into(),
            values: values.into_iter().collect(),
            color: None,
        }
    }

    /// The series' color; defaults to the next of primary, info, success, warning and danger, in
    /// the order the series were added.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// The colors series take in turn when they name none.
const PALETTE: [Color; 5] = [
    Color::Primary,
    Color::Info,
    Color::Success,
    Color::Warning,
    Color::Danger,
];

/// The plot's drawing box: 100 units each way, stretched to the plot's size.
const PLOT: f64 = 100.0;
/// The share of a label's column its bars fill, the rest split either side.
const BAR_GROUP: f64 = 0.7;
/// The most labels the bottom axis spells; past it, it spells every `n`th.
const MAX_AXIS_LABELS: usize = 12;

/// Draws `chart` in a card: its heading and description, the plot with its axes, a legend when
/// it has more than one series, and a table of its values for screen readers. The plot reads
/// "No data" when no value is finite.
///
/// ```rust
/// # use tablo_core::{Chart, Series};
/// # fn body(cx: &topcoat::context::Cx) -> topcoat::view::BoxView<'_> {
/// tablo_core::chart(
///     cx,
///     Chart::line(["Mon", "Tue", "Wed"])
///         .heading("Sign-ups")
///         .series(Series::new("Sign-ups", [3.0, 8.0, 5.0])),
/// )
/// # }
/// ```
pub fn chart(cx: &Cx, chart: Chart) -> BoxView<'_> {
    let Chart {
        kind,
        heading,
        description,
        labels,
        series,
    } = chart;
    let heading = heading.map(|heading| heading.localize(cx));
    let description = description.map(|description| description.localize(cx));
    let labels: Vec<String> = labels.iter().map(|label| label.localize(cx)).collect();
    let series: Vec<Drawn> = series
        .into_iter()
        .enumerate()
        .map(|(index, series)| Drawn::new(cx, series, index, labels.len()))
        .collect();
    let axis = Axis::over(
        series
            .iter()
            .flat_map(|drawn| drawn.values.iter().flatten()),
    );
    let plot = axis.map(|axis| plot(cx, kind, &axis, &labels, &series));
    let empty = crate::t!(cx, "tablo-chart-empty");
    let legend: Vec<(String, &'static str)> = if series.len() > 1 {
        series
            .iter()
            .map(|drawn| (drawn.label.clone(), drawn.color.dot_class()))
            .collect()
    } else {
        Vec::new()
    };
    let table = data_table(cx, heading.clone(), &labels, &series);
    view! {
        cx =>
        <section
            data-chart=(kind.name())
            class="flex flex-col gap-4 rounded-xl border border-border bg-card p-5 text-card-foreground shadow-xs"
        >
            if heading.is_some() || description.is_some() {
                <div class="flex flex-col gap-1">
                    if let Some(heading) = heading {
                        <h2 class="text-base font-semibold">(heading)</h2>
                    }
                    if let Some(description) = description {
                        <p class="text-sm text-muted-foreground">(description)</p>
                    }
                </div>
            }
            if let Some(plot) = plot {
                (plot)
            } else {
                <p class="py-12 text-center text-sm text-muted-foreground">(empty)</p>
            }
            if !legend.is_empty() {
                <ul
                    class="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground"
                    aria-hidden="true"
                >
                    for (label, dot) in legend {
                        <li class="flex items-center gap-1.5">
                            <span class=(dot)></span>
                            (label)
                        </li>
                    }
                </ul>
            }
            (table)
        </section>
    }
    .boxed()
}

/// A [`Series`] ready to draw: its label localized, one value per label of the chart, its color
/// settled.
struct Drawn {
    label: String,
    values: Vec<Option<f64>>,
    color: Color,
}

impl Drawn {
    fn new(cx: &Cx, series: Series, index: usize, labels: usize) -> Self {
        let mut values: Vec<Option<f64>> = series
            .values
            .into_iter()
            .take(labels)
            .map(|value| value.is_finite().then_some(value))
            .collect();
        values.resize(labels, None);
        Self {
            label: series.label.localize(cx),
            values,
            color: series.color.unwrap_or(PALETTE[index % PALETTE.len()]),
        }
    }
}

/// The plot: tick labels up the left, the SVG, and the chart's labels along the bottom. The
/// labels are HTML so they keep their size while the SVG stretches to the widget's width.
fn plot<'a>(
    cx: &'a Cx,
    kind: ChartKind,
    axis: &Axis,
    labels: &[String],
    series: &[Drawn],
) -> BoxView<'a> {
    let ticks: Vec<(String, String)> = axis
        .ticks()
        .map(|tick| (axis.format(tick), coordinate(PLOT * axis.share(tick))))
        .collect();
    let width = PLOT / labels.len() as f64;
    let marks: Vec<BoxView<'a>> = series
        .iter()
        .enumerate()
        .map(|(index, drawn)| match kind {
            ChartKind::Line => line(cx, axis, width, drawn),
            ChartKind::Bar => bars(cx, axis, width, index, series.len(), drawn),
        })
        .collect();
    // One hover target per label, naming every series' value there.
    let columns: Vec<(String, String)> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let values: Vec<String> = series
                .iter()
                .map(|drawn| {
                    let value = drawn.values[index].map_or_else(|| "–".to_string(), format_value);
                    format!("{}: {value}", drawn.label)
                })
                .collect();
            (
                coordinate(index as f64 * width),
                format!("{label} — {}", values.join(", ")),
            )
        })
        .collect();
    let step = labels.len().div_ceil(MAX_AXIS_LABELS);
    let bottom: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            if index % step == 0 {
                label.clone()
            } else {
                String::new()
            }
        })
        .collect();
    let bottom_columns = format!(
        "grid-template-columns: repeat({}, minmax(0, 1fr))",
        labels.len()
    );
    let column_width = coordinate(width);
    let plot = PLOT.to_string();
    let view_box = format!("0 0 {PLOT} {PLOT}");
    view! {
        cx =>
        <div
            class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2"
            aria-hidden="true"
        >
            <div
                class="flex h-48 flex-col justify-between text-xs text-muted-foreground tabular-nums"
            >
                for (tick, _) in ticks.iter().rev() {
                    <div class="flex h-0 items-center justify-end">(tick.clone())</div>
                }
            </div>
            <svg
                class="h-48 w-full overflow-visible"
                viewBox=(view_box)
                preserveAspectRatio="none"
            >
                <g class="text-border">
                    for (_, y) in ticks.iter() {
                        <line
                            x1="0"
                            x2=(plot.clone())
                            y1=(y.clone())
                            y2=(y.clone())
                            stroke="currentColor"
                            vector-effect="non-scaling-stroke"
                        />
                    }
                </g>
                for mark in marks {
                    (mark)
                }
                for (x, title) in columns {
                    <rect
                        x=(x)
                        y="0"
                        width=(column_width.clone())
                        height=(plot.clone())
                        fill="transparent"
                    >
                        <title>(title)</title>
                    </rect>
                }
            </svg>
            <div></div>
            <div class="grid text-xs text-muted-foreground" style=(bottom_columns)>
                for label in bottom {
                    <span class="truncate text-center">(label)</span>
                }
            </div>
        </div>
    }
    .boxed()
}

/// A series as a line through the middle of each label's column, broken where a value is
/// missing. A lone value between gaps draws as a dot: a zero-length line with round caps.
fn line<'a>(cx: &'a Cx, axis: &Axis, width: f64, drawn: &Drawn) -> BoxView<'a> {
    let runs = runs(&drawn.values, |index, value| {
        format!(
            "{},{}",
            coordinate((index as f64 + 0.5) * width),
            coordinate(PLOT * axis.share(value))
        )
    });
    let class = drawn.color.text_class();
    view! {
        cx =>
        <g class=(class)>
            for points in runs {
                <polyline
                    points=(points)
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    vector-effect="non-scaling-stroke"
                />
            }
        </g>
    }
    .boxed()
}

/// The `points` of each unbroken run of `values`, a lone value's point given twice.
fn runs(values: &[Option<f64>], point: impl Fn(usize, f64) -> String) -> Vec<String> {
    let mut runs: Vec<Vec<String>> = vec![Vec::new()];
    for (index, value) in values.iter().enumerate() {
        match value {
            Some(value) => runs
                .last_mut()
                .expect("one run at least")
                .push(point(index, *value)),
            None => runs.push(Vec::new()),
        }
    }
    runs.into_iter()
        .filter_map(|run| match run.as_slice() {
            [] => None,
            [point] => Some(format!("{point} {point}")),
            _ => Some(run.join(" ")),
        })
        .collect()
}

/// A series as one bar per value, from zero to the value, beside the other series' bars of the
/// same label.
fn bars<'a>(
    cx: &'a Cx,
    axis: &Axis,
    width: f64,
    index: usize,
    count: usize,
    drawn: &Drawn,
) -> BoxView<'a> {
    let bar = width * BAR_GROUP / count as f64;
    let zero = PLOT * axis.share(0.0);
    let rects: Vec<[String; 4]> = drawn
        .values
        .iter()
        .enumerate()
        .filter_map(|(label, value)| {
            let y = PLOT * axis.share((*value)?);
            let x = label as f64 * width + width * (1.0 - BAR_GROUP) / 2.0 + index as f64 * bar;
            // A tenth of the bar's slot keeps it apart from its neighbour.
            Some([
                coordinate(x + bar * 0.05),
                coordinate(y.min(zero)),
                coordinate(bar * 0.9),
                coordinate((y - zero).abs()),
            ])
        })
        .collect();
    let class = drawn.color.text_class();
    view! {
        cx =>
        <g class=(class)>
            for [x, y, w, h] in rects {
                <rect x=(x) y=(y) width=(w) height=(h) fill="currentColor" />
            }
        </g>
    }
    .boxed()
}

/// The chart's values as a table, for screen readers: a column per label, a row per series.
fn data_table<'a>(
    cx: &'a Cx,
    heading: Option<String>,
    labels: &[String],
    series: &[Drawn],
) -> BoxView<'a> {
    let labels = labels.to_vec();
    let rows: Vec<(String, Vec<String>)> = series
        .iter()
        .map(|drawn| {
            let values = drawn
                .values
                .iter()
                .map(|value| value.map(format_value).unwrap_or_default())
                .collect();
            (drawn.label.clone(), values)
        })
        .collect();
    view! {
        cx =>
        <table class="sr-only" data-chart-data="">
            if let Some(heading) = heading {
                <caption>(heading)</caption>
            }
            <thead>
                <tr>
                    <td></td>
                    for label in labels {
                        <th scope="col">(label)</th>
                    }
                </tr>
            </thead>
            <tbody>
                for (label, values) in rows {
                    <tr>
                        <th scope="row">(label)</th>
                        for value in values {
                            <td>(value)</td>
                        }
                    </tr>
                }
            </tbody>
        </table>
    }
    .boxed()
}

/// A coordinate in the drawing box, to three decimals.
fn coordinate(value: f64) -> String {
    format!("{value:.3}")
}

/// A value as the chart spells it: a whole number without decimals, any other with two.
fn format_value(value: f64) -> String {
    // `i128` holds every whole float below 1.7e38 exactly.
    if value.fract() == 0.0 && value.abs() < 1e38 {
        #[allow(clippy::cast_possible_truncation)]
        format_number(Number::Int(value as i128), 0)
    } else {
        format_number(Number::Float(value), 2)
    }
}

#[cfg(test)]
mod tests;
