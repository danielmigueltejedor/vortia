//! The mesh viewer: the surface of a case drawn by egui, turned with the mouse. The geometry is
//! in [`crate::surface`]; this draws it, on the CPU, so it runs wherever egui does.

use eframe::egui::{
    self, Color32, CornerRadius, Mesh, PointerButton, Pos2, Sense, Stroke, Vec2, vec2,
};

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::fieldview::{self, CellValues};
use crate::flowlines::{self, FlowLines};
use crate::surface::{Camera, Surface};

/// Outlines are drawn for surfaces up to this many faces; beyond it they would cost more than
/// they show.
const MAX_OUTLINED_FACES: usize = 8_000;
/// The width of the list of patches beside the picture.
const LEGEND: f32 = 220.0;

/// The colours of the patches, in turn; the ones that close the domain in 2-D are grey.
const PALETTE: [Color32; 7] = [
    Color32::from_rgb(0x4c, 0x8b, 0xf5),
    Color32::from_rgb(0xf0, 0x8c, 0x3a),
    Color32::from_rgb(0x2f, 0xb3, 0x8c),
    Color32::from_rgb(0x9b, 0x6b, 0xe0),
    Color32::from_rgb(0xe0, 0x5b, 0x8a),
    Color32::from_rgb(0xd0, 0xb0, 0x2c),
    Color32::from_rgb(0x4f, 0xb8, 0xd8),
];

/// The colour of a patch of `kind`, the `index`-th of its mesh.
pub fn patch_color(kind: &str, index: usize) -> Color32 {
    if is_closing(kind) {
        Color32::from_rgb(0x8a, 0x92, 0x9c)
    } else {
        PALETTE
            .get(index % PALETTE.len())
            .copied()
            .unwrap_or(Color32::GRAY)
    }
}

/// Whether patches of this kind close the domain (the sides of a 2-D or wedge case) rather
/// than bound the flow.
fn is_closing(kind: &str) -> bool {
    matches!(
        kind,
        "empty" | "wedge" | "symmetry" | "symmetryPlane" | "cyclic"
    )
}

/// Where the viewer finds the fields of the case: its folder, the times written and the field
/// files of the latest of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub dir: PathBuf,
    pub times: Vec<String>,
    pub fields: Vec<String>,
}

pub struct Viewer {
    surface: Surface,
    camera: Camera,
    visible: Vec<bool>,
    outlines: bool,
    /// The field the surface is coloured with, or none for the colours of the patches.
    field: Option<String>,
    /// The time it is read at; the latest written when none is chosen.
    time: Option<String>,
    values: Option<Result<CellValues, String>>,
    values_rx: Option<Receiver<Result<CellValues, String>>>,
    /// What `values` (or the read in flight) is of.
    requested: Option<(String, String)>,
    /// Whether streamlines and arrows are drawn over a vector field.
    lines: bool,
    arrows: bool,
    flow: Option<Result<FlowLines, String>>,
    flow_rx: Option<Receiver<Result<FlowLines, String>>>,
    flow_requested: Option<(String, String)>,
}

impl Viewer {
    pub fn new(surface: Surface) -> Self {
        let camera = Camera::fit(&surface);
        let visible = vec![true; surface.patches.len()];
        Self {
            surface,
            camera,
            visible,
            outlines: true,
            field: None,
            time: None,
            values: None,
            values_rx: None,
            requested: None,
            lines: false,
            arrows: false,
            flow: None,
            flow_rx: None,
            flow_requested: None,
        }
    }

    /// Colours the surface with a field of the case (`None`: the colours of the patches), at a
    /// time (`None`: the latest written).
    pub fn colour_by(&mut self, field: Option<&str>, time: Option<&str>) {
        self.field = field.map(str::to_owned);
        self.time = time.map(str::to_owned);
    }

    pub fn values(&self) -> Option<&Result<CellValues, String>> {
        self.values.as_ref()
    }

    /// Takes a finished read, and starts the one the choices ask for if it is not the one held.
    fn sync_field(&mut self, ctx: &egui::Context, source: &Source) {
        if let Some(rx) = &self.values_rx
            && let Ok(read) = rx.try_recv()
        {
            self.values = Some(read);
            self.values_rx = None;
        }
        let Some(name) = self.field.clone() else {
            self.values = None;
            self.values_rx = None;
            self.requested = None;
            return;
        };
        let time = self
            .time
            .clone()
            .or_else(|| source.times.last().cloned())
            .unwrap_or_else(|| "0".to_owned());
        let wanted = (name.clone(), time.clone());
        if self.requested.as_ref() != Some(&wanted) {
            let repaint = ctx.clone();
            self.values = None;
            self.values_rx = Some(fieldview::spawn(
                source.dir.clone(),
                time,
                name,
                self.surface.cells,
                move || repaint.request_repaint(),
            ));
            self.requested = Some(wanted);
        }
        self.sync_flow(ctx, source);
    }

    /// Takes a finished flow, and starts the one the choices ask for: lines or arrows wanted
    /// and a vector field on the surface.
    fn sync_flow(&mut self, ctx: &egui::Context, source: &Source) {
        if let Some(rx) = &self.flow_rx
            && let Ok(done) = rx.try_recv()
        {
            self.flow = Some(done);
            self.flow_rx = None;
        }
        let vector = matches!(&self.values, Some(Ok(v)) if v.kind == "vector");
        let wanted = match (&self.field, vector && (self.lines || self.arrows)) {
            (Some(name), true) => {
                let time = self
                    .time
                    .clone()
                    .or_else(|| source.times.last().cloned())
                    .unwrap_or_else(|| "0".to_owned());
                Some((name.clone(), time))
            }
            _ => None,
        };
        let Some(wanted) = wanted else {
            self.flow = None;
            self.flow_rx = None;
            self.flow_requested = None;
            return;
        };
        if self.flow_requested.as_ref() != Some(&wanted) {
            let repaint = ctx.clone();
            self.flow = None;
            self.flow_rx = Some(flowlines::spawn(
                source.dir.clone(),
                wanted.1.clone(),
                wanted.0.clone(),
                move || repaint.request_repaint(),
            ));
            self.flow_requested = Some(wanted);
        }
    }

    /// Streamlines and arrows over the picture.
    fn draw_flow(&self, painter: &egui::Painter, rect: egui::Rect, screen: [f32; 2]) {
        let Some(Ok(flow)) = &self.flow else {
            return;
        };
        let to_pixel = self.camera.pixel_projection(&self.surface, screen);
        let at = |p: [f32; 2]| Pos2::new(rect.min.x + p[0], rect.min.y + p[1]);
        if self.lines {
            let halo = Stroke::new(2.6, Color32::from_black_alpha(90));
            let ink = Stroke::new(1.2, Color32::from_white_alpha(235));
            for line in &flow.lines {
                let points: Vec<Pos2> = line.points.iter().map(|p| at(to_pixel(p))).collect();
                painter.add(egui::Shape::line(points.clone(), halo));
                painter.add(egui::Shape::line(points, ink));
            }
        }
        if self.arrows {
            let reach = self.surface.radius * 0.01;
            // About one seed spacing, so neighbours do not run into each other.
            let arrow_size =
                (screen[0].min(screen[1]) * 0.9 * self.camera.zoom / 18.0 * 0.85).clamp(5.0, 24.0);
            for arrow in &flow.arrows {
                let tip = [
                    arrow.at[0] + arrow.direction[0] * reach,
                    arrow.at[1] + arrow.direction[1] * reach,
                    arrow.at[2] + arrow.direction[2] * reach,
                ];
                let (from, to) = (to_pixel(&arrow.at), to_pixel(&tip));
                let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                let length = (dx * dx + dy * dy).sqrt();
                // An arrow pointing at the viewer is a dot, not a direction.
                if !length.is_finite() || length < 1e-6 {
                    continue;
                }
                let (ux, uy) = (dx / length, dy / length);
                let size = arrow_size;
                let head = [from[0] + ux * size, from[1] + uy * size];
                let [r, g, b] = fieldview::scale(flow.fraction(arrow.speed));
                let colour = Color32::from_rgb(r, g, b);
                let halo = Stroke::new(3.4, Color32::from_black_alpha(140));
                let ink = Stroke::new(1.8, colour);
                let wing = |sign: f32| {
                    at([
                        head[0] - ux * arrow_size * 0.35 - sign * uy * arrow_size * 0.22,
                        head[1] - uy * arrow_size * 0.35 + sign * ux * arrow_size * 0.22,
                    ])
                };
                for stroke in [halo, ink] {
                    painter.line_segment([at(from), at(head)], stroke);
                    painter.line_segment([at(head), wing(1.0)], stroke);
                    painter.line_segment([at(head), wing(-1.0)], stroke);
                }
            }
        }
    }

    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    /// Hides or shows a patch of the surface.
    pub fn set_visible(&mut self, patch: usize, visible: bool) {
        if let Some(slot) = self.visible.get_mut(patch) {
            *slot = visible;
        }
    }

    fn visible_faces(&self) -> usize {
        self.surface
            .patches
            .iter()
            .zip(&self.visible)
            .filter(|(_, shown)| **shown)
            .map(|(patch, _)| patch.faces)
            .sum()
    }

    /// The viewer and its list of patches, filling the room the ui has left.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        background: Color32,
        muted: Color32,
        radius: u8,
        source: &Source,
    ) {
        self.sync_field(&ui.ctx().clone(), source);
        let room = ui.available_size();
        ui.horizontal_top(|ui| {
            let picture = Vec2::new(
                (room.x - LEGEND - ui.spacing().item_spacing.x).max(100.0),
                room.y,
            );
            self.picture(ui, picture, background, muted, radius);
            ui.allocate_ui_with_layout(
                vec2(LEGEND, room.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.legend(ui, muted, source),
            );
        });
    }

    fn picture(
        &mut self,
        ui: &mut egui::Ui,
        size: Vec2,
        background: Color32,
        muted: Color32,
        radius: u8,
    ) {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(radius), background);

        // The mouse: the primary button turns the surface, the others slide it, the wheel zooms.
        let drag = response.drag_delta();
        let pixels_per_radius =
            (rect.width().min(rect.height()) / 2.0 * self.camera.zoom * 0.9).max(1.0);
        if response.dragged_by(PointerButton::Primary) {
            self.camera.orbit(drag.x * 0.01, drag.y * 0.01);
        } else if response.dragged_by(PointerButton::Secondary)
            || response.dragged_by(PointerButton::Middle)
        {
            self.camera.pan[0] += drag.x / pixels_per_radius;
            self.camera.pan[1] -= drag.y / pixels_per_radius;
        }
        if response.hovered() {
            let wheel = ui.input(|i| i.smooth_scroll_delta.y);
            if wheel != 0.0 {
                self.camera.zoom_by((wheel * 0.002).exp());
            }
        }
        if response.double_clicked() {
            self.camera = Camera::fit(&self.surface);
        }

        let screen = [rect.width(), rect.height()];
        let triangles = self.camera.project(&self.surface, screen, &self.visible);
        let mut mesh = Mesh::default();
        mesh.reserve_triangles(triangles.len());
        mesh.reserve_vertices(triangles.len().saturating_mul(3));
        for triangle in &triangles {
            let color = match &self.values {
                Some(Ok(values)) => match values.fraction(triangle.cell) {
                    Some(t) => {
                        let [r, g, b] = fieldview::scale(t);
                        // A light touch of shading: the colour is the information.
                        shade(Color32::from_rgb(r, g, b), 0.85 + 0.15 * triangle.light)
                    }
                    None => Color32::from_rgb(0x55, 0x5b, 0x63),
                },
                _ => {
                    let kind = self
                        .surface
                        .patches
                        .get(triangle.patch)
                        .map_or("", |p| p.kind.as_str());
                    shade(patch_color(kind, triangle.patch), triangle.light)
                }
            };
            let first = mesh.vertices.len() as u32;
            for [x, y] in triangle.points {
                mesh.colored_vertex(Pos2::new(rect.min.x + x, rect.min.y + y), color);
            }
            mesh.add_triangle(first, first + 1, first + 2);
        }
        painter.add(egui::Shape::mesh(mesh));

        // The borders of the patches, in the colour of the patch: a wall seen edge on is a line.
        // The patches that close the domain come first, so they never cover the others.
        let borders = self.camera.borders(&self.surface, screen, &self.visible);
        for closing in [true, false] {
            for ([a, b], patch) in &borders {
                let kind = self
                    .surface
                    .patches
                    .get(*patch)
                    .map_or("", |p| p.kind.as_str());
                if is_closing(kind) != closing {
                    continue;
                }
                let width = if closing { 1.0 } else { 2.5 };
                painter.line_segment(
                    [
                        Pos2::new(rect.min.x + a[0], rect.min.y + a[1]),
                        Pos2::new(rect.min.x + b[0], rect.min.y + b[1]),
                    ],
                    Stroke::new(width, patch_color(kind, *patch)),
                );
            }
        }
        if self.outlines && self.visible_faces() <= MAX_OUTLINED_FACES {
            let ink = Stroke::new(0.75, Color32::from_black_alpha(110));
            for [a, b] in self.camera.outlines(&self.surface, screen, &self.visible) {
                painter.line_segment(
                    [
                        Pos2::new(rect.min.x + a[0], rect.min.y + a[1]),
                        Pos2::new(rect.min.x + b[0], rect.min.y + b[1]),
                    ],
                    ink,
                );
            }
        }

        self.draw_flow(&painter, rect, screen);

        let mut note = format!(
            "{} faces  ·  drag to turn, right-drag to slide, wheel to zoom, double-click to reset",
            self.surface.face_count()
        );
        if self.surface.truncated {
            note.push_str("  ·  the mesh is larger than the viewer shows");
        }
        painter.text(
            Pos2::new(rect.min.x + 12.0, rect.max.y - 10.0),
            egui::Align2::LEFT_BOTTOM,
            note,
            egui::FontId::proportional(11.0),
            muted,
        );
    }

    /// The choice of field and time, and the colour scale of the field read.
    fn field_choices(&mut self, ui: &mut egui::Ui, muted: Color32, source: &Source) {
        ui.label(egui::RichText::new("Colour by").color(muted));
        let current = self.field.clone();
        egui::ComboBox::from_id_salt("viewer-field")
            .width(LEGEND - 12.0)
            .selected_text(current.as_deref().unwrap_or("Patches"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.field, None, "Patches");
                // A file with a space in its name (a copy of another) is not a field.
                for name in source.fields.iter().filter(|n| !n.contains(' ')) {
                    ui.selectable_value(&mut self.field, Some(name.clone()), name);
                }
            });
        if self.field.is_some() {
            let shown = self
                .time
                .clone()
                .or_else(|| source.times.last().cloned())
                .unwrap_or_else(|| "0".to_owned());
            ui.add_space(4.0);
            egui::ComboBox::from_id_salt("viewer-time")
                .width(LEGEND - 12.0)
                .selected_text(format!("Time {shown}"))
                .show_ui(ui, |ui| {
                    for time in source.times.iter().rev() {
                        ui.selectable_value(&mut self.time, Some(time.clone()), time);
                    }
                });
            ui.add_space(6.0);
            match &self.values {
                Some(Ok(values)) => {
                    scale_bar(ui, values, muted);
                    if values.kind == "vector" {
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.lines, "Streamlines");
                        ui.checkbox(&mut self.arrows, "Arrows");
                        match &self.flow {
                            Some(Err(message)) => {
                                ui.label(egui::RichText::new(message).size(11.0).color(muted));
                            }
                            None if self.lines || self.arrows => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(egui::RichText::new("Following…").color(muted));
                                });
                            }
                            _ => {}
                        }
                    }
                }
                Some(Err(message)) => {
                    ui.label(egui::RichText::new(message).size(11.0).color(muted));
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new("Reading…").color(muted));
                    });
                }
            }
        }
        ui.add_space(10.0);
    }

    fn legend(&mut self, ui: &mut egui::Ui, muted: Color32, source: &Source) {
        self.field_choices(ui, muted, source);
        if self.surface.face_count() <= MAX_OUTLINED_FACES {
            ui.checkbox(&mut self.outlines, "Cell edges");
            ui.add_space(6.0);
        }
        ui.label(egui::RichText::new("Patches").color(muted));
        egui::ScrollArea::vertical()
            .id_salt("viewer-patches")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, (patch, shown)) in self
                    .surface
                    .patches
                    .iter()
                    .zip(self.visible.iter_mut())
                    .enumerate()
                {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let (swatch, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                        ui.painter().rect_filled(
                            swatch,
                            CornerRadius::same(3),
                            patch_color(&patch.kind, index),
                        );
                        ui.checkbox(shown, &patch.name);
                    });
                    ui.add_space(-4.0);
                    ui.label(
                        egui::RichText::new(format!("{}  ·  {} faces", patch.kind, patch.faces))
                            .size(11.0)
                            .color(muted),
                    );
                }
            });
    }
}

/// The colour scale as a bar, with the range under it.
fn scale_bar(ui: &mut egui::Ui, values: &CellValues, muted: Color32) {
    let Some((low, high)) = values.range else {
        ui.label(
            egui::RichText::new("No finite values.")
                .size(11.0)
                .color(muted),
        );
        return;
    };
    let (rect, _) = ui.allocate_exact_size(vec2(LEGEND - 12.0, 12.0), Sense::hover());
    let mut mesh = Mesh::default();
    const STEPS: usize = 24;
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let x = rect.min.x + rect.width() * t;
        let [r, g, b] = fieldview::scale(t);
        let color = Color32::from_rgb(r, g, b);
        mesh.colored_vertex(Pos2::new(x, rect.min.y), color);
        mesh.colored_vertex(Pos2::new(x, rect.max.y), color);
    }
    for step in 0..STEPS as u32 {
        let i = step * 2;
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i + 1, i + 3, i + 2);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(fieldview::label(low))
                .monospace()
                .size(11.0)
                .color(muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(fieldview::label(high))
                    .monospace()
                    .size(11.0)
                    .color(muted),
            );
        });
    });
    let what = if values.kind == "vector" {
        format!("{}, length", values.name)
    } else {
        values.name.clone()
    };
    ui.label(egui::RichText::new(what).size(11.0).color(muted));
}

fn shade(color: Color32, light: f32) -> Color32 {
    let light = light.clamp(0.0, 1.0);
    let scale = |channel: u8| (f32::from(channel) * light).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(scale(color.r()), scale(color.g()), scale(color.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shading_darkens_and_never_overflows() {
        let c = Color32::from_rgb(200, 100, 50);
        assert_eq!(shade(c, 1.0), c);
        assert_eq!(shade(c, 0.5), Color32::from_rgb(100, 50, 25));
        assert_eq!(shade(c, 7.0), c);
        assert_eq!(shade(c, f32::NAN.max(-1.0)), Color32::BLACK);
    }

    #[test]
    fn closing_patches_are_grey_and_the_others_take_turns() {
        assert_eq!(patch_color("empty", 0), patch_color("wedge", 5));
        assert_ne!(patch_color("patch", 0), patch_color("patch", 1));
        assert_eq!(patch_color("wall", 0), patch_color("wall", PALETTE.len()));
    }
}
