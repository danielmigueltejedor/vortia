//! The Results tab: a calculated case looked into. The field's range over the domain and on each
//! patch with what flows through it, a probe at a point, and a line plotted and saved as CSV.
//!
//! Everything is calculated by `vortia-post`, on a thread, so a large case does not freeze the
//! window; this file holds the forms, the jobs and the drawing.

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{Arc, mpsc};

use eframe::egui::{
    self, Align2, Context, FontId, Frame, Margin, Pos2, RichText, Sense, Stroke, Vec2,
};
use nodalia_theme::{Palette, component, font_size, for_egui, space};
use vortia_core::{Aabb, Vec3};
use vortia_post::{Case, FieldReport, FieldValues, Sample};

use super::{accent_button, corner, muted};

/// The times offered: the last ones, which is where a run ends up.
const MAX_TIMES: usize = 200;
/// Points a line starts with.
const LINE_POINTS: usize = 100;
/// The height of the plot.
const PLOT_HEIGHT: f32 = 220.0;

/// A calculation on a thread: not asked for, running, or done.
#[derive(Default)]
enum Slot<T> {
    #[default]
    Idle,
    Working(mpsc::Receiver<Result<T, String>>),
    Done(Result<T, String>),
}

impl<T> Slot<T> {
    fn poll(&mut self) {
        let arrived = match self {
            Slot::Working(rx) => match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("The calculation stopped unexpectedly.".to_owned()))
                }
            },
            _ => None,
        };
        if let Some(result) = arrived {
            *self = Slot::Done(result);
        }
    }

    fn is_idle(&self) -> bool {
        matches!(self, Slot::Idle)
    }
}

fn spawn<T: Send + 'static>(
    ctx: &Context,
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Slot<T> {
    let (tx, rx) = mpsc::channel();
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(job))
            .unwrap_or_else(|_| Err("The calculation stopped unexpectedly.".to_owned()));
        let _ = tx.send(result);
        repaint.request_repaint();
    });
    Slot::Working(rx)
}

/// Three numbers as typed.
#[derive(Clone, Default)]
struct PointForm([String; 3]);

impl PointForm {
    fn of(p: Vec3) -> Self {
        Self([p.x, p.y, p.z].map(plain))
    }

    fn parse(&self, what: &str) -> Result<Vec3, String> {
        let mut n = self
            .0
            .iter()
            .map(|t| t.trim().parse::<f64>().ok().filter(|v| v.is_finite()));
        match (n.next().flatten(), n.next().flatten(), n.next().flatten()) {
            (Some(x), Some(y), Some(z)) => Ok(Vec3::new(x, y, z)),
            _ => Err(format!("The {what} needs three numbers.")),
        }
    }
}

/// A line as typed: where it starts, where it ends and how many points it takes.
#[derive(Clone, Default)]
struct LineForm {
    start: PointForm,
    end: PointForm,
    points: String,
}

impl LineForm {
    /// Along the longest side of the mesh, through its middle.
    fn along(bounds: &Aabb) -> Self {
        let (centre, size) = (bounds.center(), bounds.size());
        let axis = if size.x >= size.y && size.x >= size.z {
            0
        } else if size.y >= size.z {
            1
        } else {
            2
        };
        let (mut start, mut end) = (centre, centre);
        start[axis] = bounds.min[axis];
        end[axis] = bounds.max[axis];
        Self {
            start: PointForm::of(start),
            end: PointForm::of(end),
            points: LINE_POINTS.to_string(),
        }
    }

    fn parse(&self) -> Result<(Vec3, Vec3, usize), String> {
        let start = self.start.parse("line's start")?;
        let end = self.end.parse("line's end")?;
        let points = self
            .points
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=vortia_post::MAX_LINE_POINTS).contains(n))
            .ok_or_else(|| {
                format!(
                    "The number of points is between 1 and {}.",
                    vortia_post::MAX_LINE_POINTS
                )
            })?;
        Ok((start, end, points))
    }
}

/// A line sampled.
struct Profile {
    time: String,
    field: String,
    values: FieldValues,
    samples: Vec<Sample>,
}

impl Profile {
    fn take(
        case: &Case,
        time: &str,
        field: &str,
        line: (Vec3, Vec3),
        points: usize,
    ) -> Result<Self, String> {
        let (values, samples) = case
            .sample_line(time, field, line.0, line.1, points)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            time: time.to_owned(),
            field: field.to_owned(),
            values,
            samples,
        })
    }

    /// What can be plotted: a scalar, or each component of a vector and its length.
    fn choices(&self) -> Vec<String> {
        match self.values {
            FieldValues::Scalar(_) => vec![self.field.clone()],
            FieldValues::Vector(_) => ["x", "y", "z", "magnitude"]
                .iter()
                .map(|c| (*c).to_owned())
                .collect(),
        }
    }

    /// `(distance, value)` of every point inside the mesh, in runs: a point outside breaks the line.
    fn runs(&self, choice: usize) -> Vec<Vec<(f64, f64)>> {
        let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut open = false;
        for s in &self.samples {
            let value = match s.value.as_slice() {
                [] => None,
                [v] => Some(*v),
                [x, y, z] => match choice {
                    0 => Some(*x),
                    1 => Some(*y),
                    2 => Some(*z),
                    _ => Some((x * x + y * y + z * z).sqrt()),
                },
                _ => None,
            };
            match value {
                Some(v) => {
                    if !open {
                        runs.push(Vec::new());
                        open = true;
                    }
                    if let Some(run) = runs.last_mut() {
                        run.push((s.distance, v));
                    }
                }
                None => open = false,
            }
        }
        runs
    }
}

/// A probe read.
struct Probe {
    at: Vec3,
    cell: Option<usize>,
    value: Vec<f64>,
}

impl Probe {
    fn read(case: &Case, time: &str, field: &str, at: Vec3) -> Result<Self, String> {
        let s = case.probe(time, field, at).map_err(|e| e.to_string())?;
        Ok(Self {
            at,
            cell: s.cell,
            value: s.value,
        })
    }
}

/// What the Results tab holds for one case.
#[derive(Default)]
pub struct Results {
    case: Slot<Arc<Case>>,
    times: Vec<String>,
    fields: Vec<String>,
    time: Option<String>,
    field: Option<String>,
    report: Slot<FieldReport>,
    line: LineForm,
    profile: Slot<Profile>,
    choice: usize,
    point: PointForm,
    probe: Slot<Probe>,
    saved: Option<(bool, String)>,
    /// The case was read and the forms filled in once.
    started: bool,
}

impl Results {
    /// Takes what the threads finished and starts what is wanted.
    fn update(&mut self, dir: &Path, ctx: &Context) {
        self.case.poll();
        self.report.poll();
        self.profile.poll();
        self.probe.poll();
        if self.case.is_idle() {
            let dir = dir.to_owned();
            self.case = spawn(ctx, move || {
                Case::open(&dir).map(Arc::new).map_err(|e| e.to_string())
            });
        }
        let Slot::Done(Ok(case)) = &self.case else {
            return;
        };
        let case = Arc::clone(case);
        if !self.started {
            self.started = true;
            let mut names: Vec<String> = case
                .times()
                .map(|t| t.into_iter().map(|t| t.name).collect())
                .unwrap_or_default();
            let skip = names.len().saturating_sub(MAX_TIMES);
            names.drain(..skip);
            self.times = names;
            let bounds = case.bounds();
            self.line = LineForm::along(&bounds);
            self.point = PointForm::of(bounds.center());
            if let Some(last) = self.times.last().cloned() {
                self.set_time(&case, &last);
            }
        }
        if self.report.is_idle()
            && let (Some(time), Some(field)) = (self.time.clone(), self.field.clone())
        {
            let case = Arc::clone(&case);
            self.report = spawn(ctx, move || {
                case.report(&time, &field).map_err(|e| e.to_string())
            });
        }
    }

    fn set_time(&mut self, case: &Case, time: &str) {
        self.time = Some(time.to_owned());
        self.fields = case.field_names(time).unwrap_or_default();
        let keep = self
            .field
            .as_ref()
            .filter(|f| self.fields.contains(f))
            .cloned();
        let default = ["p", "T", "U"]
            .iter()
            .find(|f| self.fields.iter().any(|n| n == *f))
            .map(|f| (*f).to_owned())
            .or_else(|| self.fields.first().cloned());
        self.field = keep.or(default);
        self.forget();
    }

    /// What was calculated for another time or field is not what is asked for any more.
    fn forget(&mut self) {
        self.report = Slot::Idle;
        self.profile = Slot::Idle;
        self.probe = Slot::Idle;
        self.saved = None;
    }

    /// Reads the case again, for a run that wrote more times.
    fn refresh(&mut self) {
        let (time, field) = (self.time.take(), self.field.take());
        *self = Self::default();
        self.time = time;
        self.field = field;
    }
}

/// The Results tab.
pub(super) fn results_tab(ui: &mut egui::Ui, p: &Palette, dir: &Path, results: &mut Results) {
    results.update(dir, ui.ctx());
    let case = match &results.case {
        Slot::Working(_) | Slot::Idle => {
            wait(ui, "Reading the case…");
            return;
        }
        Slot::Done(Err(message)) => {
            ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
            return;
        }
        Slot::Done(Ok(case)) => Arc::clone(case),
    };
    if results.times.is_empty() {
        ui.label(RichText::new("The case has no results yet. Run it first.").color(muted(p)));
        if ui.button("Look again").clicked() {
            results.refresh();
        }
        return;
    }
    let mut pick_time: Option<String> = None;
    let mut pick_field: Option<String> = None;
    let mut refresh = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Time").color(muted(p)));
        let shown = results.time.clone().unwrap_or_default();
        egui::ComboBox::from_id_salt("results-time")
            .selected_text(shown.as_str())
            .show_ui(ui, |ui| {
                for t in results.times.iter().rev() {
                    if ui.selectable_label(*t == shown, t).clicked() && *t != shown {
                        pick_time = Some(t.clone());
                    }
                }
            });
        ui.add_space(space::MD);
        ui.label(RichText::new("Field").color(muted(p)));
        let shown = results.field.clone().unwrap_or_default();
        egui::ComboBox::from_id_salt("results-field")
            .selected_text(shown.as_str())
            .show_ui(ui, |ui| {
                for f in &results.fields {
                    if ui.selectable_label(*f == shown, f).clicked() && *f != shown {
                        pick_field = Some(f.clone());
                    }
                }
            });
        ui.add_space(space::MD);
        refresh = ui.button("Reload").clicked();
    });
    if let Some(t) = pick_time {
        results.set_time(&case, &t);
    } else if let Some(f) = pick_field {
        results.field = Some(f);
        results.forget();
    } else if refresh {
        results.refresh();
    }
    ui.add_space(space::SM);
    egui::ScrollArea::vertical()
        .id_salt("results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            overview(ui, p, results);
            ui.add_space(space::MD);
            probe_card(ui, p, &case, results);
            ui.add_space(space::MD);
            line_card(ui, p, &case, results);
        });
}

fn wait(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(text);
    });
}

fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::NONE
        .fill(for_egui::color(p.surface_sunken))
        .corner_radius(corner(component::CONTROL_RADIUS))
        .inner_margin(Margin::same(space::MD as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A number with a few digits, plain where it is of a size to read and scientific otherwise.
fn plain(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{v}");
    }
    if (1.0e-3..1.0e6).contains(&v.abs()) {
        let s = format!("{v:.5}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        format!("{v:.4e}")
    }
}

fn shown(values: &[f64]) -> String {
    match values {
        [one] => plain(*one),
        many => format!(
            "({})",
            many.iter().map(|v| plain(*v)).collect::<Vec<_>>().join(" ")
        ),
    }
}

fn overview(ui: &mut egui::Ui, p: &Palette, results: &Results) {
    card(ui, p, |ui| match &results.report {
        Slot::Idle | Slot::Working(_) => wait(ui, "Adding up the field…"),
        Slot::Done(Err(message)) => {
            ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
        }
        Slot::Done(Ok(report)) => {
            ui.label(RichText::new(format!("{} at {}", report.field, report.time)).strong());
            ui.add_space(space::SM);
            let v = &report.volume;
            ui.horizontal_wrapped(|ui| {
                for (name, value) in [
                    ("Cells", v.cells.to_string()),
                    ("Volume", plain(v.volume)),
                    ("Average", shown(&v.average)),
                    ("Minimum", shown(&v.min)),
                    ("Maximum", shown(&v.max)),
                    ("Integral", shown(&v.integral)),
                ] {
                    ui.label(RichText::new(name).color(muted(p)));
                    ui.label(value);
                    ui.add_space(space::MD);
                }
            });
            ui.add_space(space::MD);
            ui.label(RichText::new("Patches").color(muted(p)));
            egui::Grid::new("results-patches")
                .num_columns(7)
                .spacing([space::LG, space::XS])
                .striped(true)
                .show(ui, |ui| {
                    for head in [
                        "Patch", "Type", "Area", "Average", "Minimum", "Maximum", "Flux out",
                    ] {
                        ui.label(RichText::new(head).size(font_size::SMALL).color(muted(p)));
                    }
                    ui.end_row();
                    for patch in &report.patches {
                        let s = &patch.stats;
                        ui.label(&patch.name);
                        ui.label(RichText::new(&patch.kind).color(muted(p)));
                        ui.label(plain(s.area));
                        ui.label(shown(&s.average));
                        ui.label(shown(&s.min));
                        ui.label(shown(&s.max));
                        ui.label(patch.flux.map_or_else(|| "–".to_owned(), plain));
                        ui.end_row();
                    }
                });
            let fluxes: Vec<f64> = report.patches.iter().filter_map(|p| p.flux).collect();
            if !fluxes.is_empty() {
                let total: f64 = fluxes.iter().sum();
                ui.add_space(space::SM);
                ui.label(
                    RichText::new(format!(
                        "Flux out of all patches: {} (zero when what comes in goes out)",
                        plain(total)
                    ))
                    .size(font_size::SMALL)
                    .color(muted(p)),
                );
            }
        }
    });
}

fn point_row(ui: &mut egui::Ui, label: &str, p: &Palette, form: &mut PointForm) {
    ui.label(RichText::new(label).color(muted(p)));
    for (text, axis) in form.0.iter_mut().zip(["x", "y", "z"]) {
        ui.label(RichText::new(axis).size(font_size::SMALL).color(muted(p)));
        ui.add(egui::TextEdit::singleline(text).desired_width(86.0));
    }
}

fn probe_card(ui: &mut egui::Ui, p: &Palette, case: &Arc<Case>, results: &mut Results) {
    let mut ask = false;
    card(ui, p, |ui| {
        ui.label(RichText::new("Probe").strong());
        ui.add_space(space::SM);
        ui.horizontal(|ui| {
            point_row(ui, "At", p, &mut results.point);
            ask = accent_button(ui, p, "Read").clicked();
        });
        match &results.probe {
            Slot::Working(_) => wait(ui, "Reading…"),
            Slot::Done(Err(message)) => {
                ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
            }
            Slot::Done(Ok(probe)) => {
                let text = match probe.cell {
                    Some(cell) => format!("{} in cell {cell}", shown(&probe.value)),
                    None => format!(
                        "({} {} {}) is outside the mesh",
                        plain(probe.at.x),
                        plain(probe.at.y),
                        plain(probe.at.z)
                    ),
                };
                ui.label(text);
            }
            Slot::Idle => {}
        }
    });
    if ask {
        let (Some(time), Some(field)) = (results.time.clone(), results.field.clone()) else {
            return;
        };
        results.probe = match results.point.parse("point") {
            Err(message) => Slot::Done(Err(message)),
            Ok(at) => {
                let case = Arc::clone(case);
                spawn(ui.ctx(), move || Probe::read(&case, &time, &field, at))
            }
        };
    }
}

fn line_card(ui: &mut egui::Ui, p: &Palette, case: &Arc<Case>, results: &mut Results) {
    let (mut ask, mut save) = (false, false);
    card(ui, p, |ui| {
        ui.label(RichText::new("Line").strong());
        ui.add_space(space::SM);
        ui.horizontal(|ui| point_row(ui, "From", p, &mut results.line.start));
        ui.horizontal(|ui| {
            point_row(ui, "To", p, &mut results.line.end);
            ui.label(
                RichText::new("points")
                    .size(font_size::SMALL)
                    .color(muted(p)),
            );
            ui.add(egui::TextEdit::singleline(&mut results.line.points).desired_width(56.0));
            ask = accent_button(ui, p, "Sample").clicked();
        });
        match &results.profile {
            Slot::Idle => {}
            Slot::Working(_) => wait(ui, "Sampling…"),
            Slot::Done(Err(message)) => {
                ui.colored_label(for_egui::color(p.status_danger_fg), message.as_str());
            }
            Slot::Done(Ok(profile)) => {
                ui.add_space(space::SM);
                let choices = profile.choices();
                if choices.len() > 1 {
                    ui.horizontal(|ui| {
                        for (i, name) in choices.iter().enumerate() {
                            ui.selectable_value(&mut results.choice, i, name);
                        }
                    });
                }
                let choice = results.choice.min(choices.len().saturating_sub(1));
                plot(ui, p, &profile.runs(choice));
                ui.horizontal(|ui| {
                    save = ui.button("Save CSV").clicked();
                    if let Some((ok, message)) = &results.saved {
                        let tone = if *ok {
                            p.status_success_fg
                        } else {
                            p.status_danger_fg
                        };
                        ui.colored_label(for_egui::color(tone), message.as_str());
                    }
                });
            }
        }
    });
    if ask {
        let (Some(time), Some(field)) = (results.time.clone(), results.field.clone()) else {
            return;
        };
        results.saved = None;
        results.profile = match results.line.parse() {
            Err(message) => Slot::Done(Err(message)),
            Ok((start, end, points)) => {
                let case = Arc::clone(case);
                spawn(ui.ctx(), move || {
                    Profile::take(&case, &time, &field, (start, end), points)
                })
            }
        };
    }
    if save && let Slot::Done(Ok(profile)) = &results.profile {
        results.saved = Some(save_csv(case.path(), profile));
    }
}

/// Writes the line to `postProcessing/line/<time>/<field>.csv` in the case, where OpenFOAM keeps
/// what its sampling writes.
fn save_csv(dir: &Path, profile: &Profile) -> (bool, String) {
    let relative = Path::new("postProcessing")
        .join("line")
        .join(&profile.time)
        .join(format!("{}.csv", profile.field));
    let text = vortia_post::line_csv(&profile.field, &profile.values, &profile.samples);
    match vortia_foamfile::write::atomic(&dir.join(&relative), text.as_bytes()) {
        Ok(()) => (true, format!("Saved to {}.", relative.display())),
        Err(e) => (false, e.to_string()),
    }
}

/// The values against the distance along the line, with the range at the corners and the
/// reading under the pointer.
fn plot(ui: &mut egui::Ui, p: &Palette, runs: &[Vec<(f64, f64)>]) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), PLOT_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        corner(component::CONTROL_RADIUS),
        for_egui::color(p.surface_panel),
    );
    let points = || runs.iter().flatten().copied();
    let (Some(x0), Some(x1)) = (
        points().map(|(x, _)| x).reduce(f64::min),
        points().map(|(x, _)| x).reduce(f64::max),
    ) else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "The line is outside the mesh",
            FontId::proportional(font_size::SMALL),
            muted(p),
        );
        return;
    };
    let (mut y0, mut y1) = (
        points().map(|(_, y)| y).fold(f64::INFINITY, f64::min),
        points().map(|(_, y)| y).fold(f64::NEG_INFINITY, f64::max),
    );
    if y1 - y0 <= f64::EPSILON * y0.abs().max(1.0) {
        y0 -= 1.0;
        y1 += 1.0;
    }
    let (low, high) = (y0, y1);
    let pad = (high - low) * 0.06;
    let (y0, y1) = (low - pad, high + pad);
    let inner = rect.shrink2(Vec2::new(10.0, 22.0));
    let span_x = (x1 - x0).max(f64::EPSILON);
    let to_screen = |(x, y): (f64, f64)| {
        Pos2::new(
            inner.left() + ((x - x0) / span_x) as f32 * inner.width(),
            inner.bottom() - ((y - y0) / (y1 - y0)) as f32 * inner.height(),
        )
    };
    let grid = Stroke::new(1.0, for_egui::color(p.border_subtle));
    for y in [low, (low + high) / 2.0, high] {
        let at = to_screen((x0, y)).y;
        painter.line_segment(
            [Pos2::new(inner.left(), at), Pos2::new(inner.right(), at)],
            grid,
        );
    }
    let small = FontId::proportional(font_size::SMALL);
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.top() + 4.0),
        Align2::LEFT_TOP,
        plain(high),
        small.clone(),
        muted(p),
    );
    painter.text(
        Pos2::new(rect.left() + 8.0, inner.bottom() - 2.0),
        Align2::LEFT_BOTTOM,
        plain(low),
        small.clone(),
        muted(p),
    );
    painter.text(
        Pos2::new(inner.left(), rect.bottom() - 4.0),
        Align2::LEFT_BOTTOM,
        plain(x0),
        small.clone(),
        muted(p),
    );
    painter.text(
        Pos2::new(inner.right(), rect.bottom() - 4.0),
        Align2::RIGHT_BOTTOM,
        format!("distance {}", plain(x1)),
        small.clone(),
        muted(p),
    );
    let ink = for_egui::color(p.accent_default);
    for run in runs {
        let line: Vec<Pos2> = run.iter().copied().map(to_screen).collect();
        if let [only] = line.as_slice() {
            painter.circle_filled(*only, 2.5, ink);
        } else {
            painter.add(egui::Shape::line(line, Stroke::new(2.0, ink)));
        }
    }
    if let Some(at) = response
        .hover_pos()
        .filter(|at| inner.expand2(Vec2::new(6.0, 0.0)).contains(*at))
        && let Some((x, y)) = points().min_by(|a, b| {
            let (da, db) = (
                (to_screen(*a).x - at.x).abs(),
                (to_screen(*b).x - at.x).abs(),
            );
            da.total_cmp(&db)
        })
    {
        let spot = to_screen((x, y));
        painter.circle_filled(spot, 4.0, ink);
        let text = format!("{}  at {}", plain(y), plain(x));
        let anchor = if spot.x > rect.center().x {
            Align2::RIGHT_BOTTOM
        } else {
            Align2::LEFT_BOTTOM
        };
        painter.text(
            Pos2::new(spot.x, (spot.y - 8.0).max(rect.top() + 16.0)),
            anchor,
            text,
            FontId::proportional(font_size::SMALL),
            for_egui::color(p.text_primary),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(distance: f64, value: &[f64]) -> Sample {
        Sample {
            distance,
            point: Vec3::default(),
            cell: (!value.is_empty()).then_some(0),
            value: value.to_vec(),
        }
    }

    #[test]
    fn a_line_goes_along_the_longest_side_through_the_middle() {
        let bounds = Aabb {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(2.0, 8.0, 4.0),
        };
        let form = LineForm::along(&bounds);
        let (start, end, points) = form.parse().unwrap();
        assert_eq!(
            (start, end),
            (Vec3::new(1.0, 0.0, 2.0), Vec3::new(1.0, 8.0, 2.0))
        );
        assert_eq!(points, LINE_POINTS);
    }

    #[test]
    fn what_cannot_be_a_line_is_said() {
        let mut form = LineForm::along(&Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 1.0, 1.0),
        });
        form.points = "0".into();
        assert!(form.parse().unwrap_err().contains("between 1 and"));
        form.points = "10".into();
        form.end.0[1] = "nan".into();
        assert!(form.parse().unwrap_err().contains("line's end"));
        form.end.0[1] = "1".into();
        form.start.0[0] = "1e999".into();
        assert!(form.parse().unwrap_err().contains("line's start"));
    }

    #[test]
    fn a_point_outside_the_mesh_breaks_the_line() {
        let profile = Profile {
            time: "1".into(),
            field: "T".into(),
            values: FieldValues::Scalar(vec![]),
            samples: vec![
                sample(0.0, &[1.0]),
                sample(1.0, &[2.0]),
                sample(2.0, &[]),
                sample(3.0, &[4.0]),
            ],
        };
        assert_eq!(
            profile.runs(0),
            vec![vec![(0.0, 1.0), (1.0, 2.0)], vec![(3.0, 4.0)]]
        );
    }

    #[test]
    fn a_vector_plots_by_component_or_length() {
        let profile = Profile {
            time: "1".into(),
            field: "U".into(),
            values: FieldValues::Vector(vec![]),
            samples: vec![sample(0.0, &[3.0, 4.0, 0.0])],
        };
        assert_eq!(profile.choices().len(), 4);
        assert_eq!(profile.runs(1), vec![vec![(0.0, 4.0)]]);
        assert_eq!(profile.runs(3), vec![vec![(0.0, 5.0)]]);
    }

    #[test]
    fn numbers_keep_a_few_digits() {
        assert_eq!(plain(0.0), "0");
        assert_eq!(plain(2.5), "2.5");
        assert_eq!(plain(101325.0), "101325");
        assert_eq!(plain(1.0e-7), "1.0000e-7");
        assert_eq!(shown(&[1.0, 2.0, 3.0]), "(1 2 3)");
    }

    /// A 4 × 2 × 1 box with a temperature of `10 x` and a case around it.
    fn case_with_results() -> tempfile::TempDir {
        use vortia_mesh::blocks::structured_box_raw;
        let dir = tempfile::tempdir().unwrap();
        structured_box_raw(
            Aabb {
                min: Vec3::default(),
                max: Vec3::new(4.0, 2.0, 1.0),
            },
            [4, 2, 1],
        )
        .unwrap()
        .write_case(dir.path())
        .unwrap();
        let values: Vec<String> = (0..8).map(|i| format!("{}", 10 * (i % 4))).collect();
        let patches: String = ["xMin", "xMax", "yMin", "yMax", "zMin", "zMax"]
            .iter()
            .map(|p| format!("    {p} {{ type zeroGradient; }}\n"))
            .collect();
        std::fs::create_dir(dir.path().join("0")).unwrap();
        std::fs::write(
            dir.path().join("0/T"),
            format!(
                "FoamFile {{ version 2.0; format ascii; class volScalarField; object T; }}\n\
                 dimensions [0 0 0 1 0 0 0];\n\
                 internalField nonuniform List<scalar> 8({});\n\
                 boundaryField\n{{\n{patches}}}\n",
                values.join(" ")
            ),
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_case_is_read_and_its_latest_field_summed_up() {
        let dir = case_with_results();
        let ctx = Context::default();
        let mut results = Results::default();
        let mut done = false;
        for _ in 0..500 {
            results.update(dir.path(), &ctx);
            if matches!(results.report, Slot::Done(_)) {
                done = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(done, "the field was never summed up");
        assert_eq!(results.times, ["0"]);
        assert_eq!(results.fields, ["T"]);
        assert_eq!(results.field.as_deref(), Some("T"));
        let Slot::Done(Ok(report)) = &results.report else {
            panic!("the report failed");
        };
        // Cells are 0, 10, 20, 30 along x, twice.
        assert_eq!(report.volume.average, vec![15.0]);
        // The line starts on the longest side, through the middle.
        let (start, end, _) = results.line.parse().unwrap();
        assert_eq!(
            (start, end),
            (Vec3::new(0.0, 1.0, 0.5), Vec3::new(4.0, 1.0, 0.5))
        );
    }

    #[test]
    fn a_line_is_sampled_and_saved_where_the_case_keeps_such_things() {
        let dir = case_with_results();
        let case = Case::open(dir.path()).unwrap();
        let line = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(3.5, 1.0, 0.5));
        let profile = Profile::take(&case, "0", "T", line, 4).unwrap();
        assert_eq!(
            profile.runs(0),
            vec![vec![(0.0, 0.0), (1.0, 10.0), (2.0, 20.0), (3.0, 30.0)]]
        );
        let (ok, message) = save_csv(dir.path(), &profile);
        assert!(ok, "{message}");
        assert_eq!(message, "Saved to postProcessing/line/0/T.csv.");
        let text = std::fs::read_to_string(dir.path().join("postProcessing/line/0/T.csv")).unwrap();
        assert!(text.starts_with("distance,x,y,z,cell,T\n"), "{text}");
        assert_eq!(text.lines().count(), 5);
        assert!(Profile::take(&case, "0", "nothere", line, 4).is_err());
    }

    #[test]
    fn a_probe_outside_the_mesh_has_no_cell() {
        let dir = case_with_results();
        let case = Case::open(dir.path()).unwrap();
        let inside = Probe::read(&case, "0", "T", Vec3::new(2.5, 0.5, 0.5)).unwrap();
        assert_eq!(inside.value, vec![20.0]);
        let outside = Probe::read(&case, "0", "T", Vec3::new(9.0, 0.5, 0.5)).unwrap();
        assert!(outside.cell.is_none());
    }

    #[test]
    fn the_tab_draws_while_it_reads_and_when_it_cannot() {
        let ctx = Context::default();
        let draw = |dir: &Path, results: &mut Results| {
            let p = super::super::palette(&ctx);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                results_tab(ui, &p, dir, results);
            });
            out.textures_delta.clear();
        };
        // A folder with no mesh: the reason shows instead of a table.
        let empty = tempfile::tempdir().unwrap();
        let mut results = Results::default();
        for _ in 0..300 {
            draw(empty.path(), &mut results);
            if matches!(results.case, Slot::Done(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(results.case, Slot::Done(Err(_))));

        let dir = case_with_results();
        let mut results = Results::default();
        for _ in 0..300 {
            draw(dir.path(), &mut results);
            if matches!(results.report, Slot::Done(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(results.report, Slot::Done(Ok(_))));
        draw(dir.path(), &mut results);
    }
}
