//! The mesh viewer: the surface of a case drawn by egui, turned with the mouse. The geometry is
//! in [`crate::surface`]; this draws it, on the CPU, so it runs wherever egui does.

use eframe::egui::{
    self, Color32, CornerRadius, Mesh, PointerButton, Pos2, Sense, Stroke, Vec2, vec2,
};

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

pub struct Viewer {
    surface: Surface,
    camera: Camera,
    visible: Vec<bool>,
    outlines: bool,
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
    pub fn show(&mut self, ui: &mut egui::Ui, background: Color32, muted: Color32, radius: u8) {
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
                |ui| self.legend(ui, muted),
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
            let base = patch_color(
                self.surface
                    .patches
                    .get(triangle.patch)
                    .map_or("", |p| p.kind.as_str()),
                triangle.patch,
            );
            let color = shade(base, triangle.light);
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

    fn legend(&mut self, ui: &mut egui::Ui, muted: Color32) {
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
