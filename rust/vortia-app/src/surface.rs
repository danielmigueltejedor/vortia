//! The outside of a mesh, for the viewer: the boundary faces of a case as triangles, grouped by
//! patch, and the camera that looks at them. Nothing here draws; [`Camera::project`] turns the
//! triangles into screen triangles, back to front, and the viewer paints those.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use vortia_foamfile::RawPolyMesh;

/// The most triangles a surface keeps; a mesh beyond it is cut off, and says so.
pub const MAX_TRIANGLES: usize = 2_000_000;

/// One patch of the mesh, as a run of triangles and of faces of the surface.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchSurface {
    pub name: String,
    /// `patch`, `wall`, `empty`, `wedge`, …
    pub kind: String,
    /// The faces of the patch that made it into the surface.
    pub faces: usize,
    triangles: std::ops::Range<usize>,
    face_range: std::ops::Range<usize>,
    /// The edges that only one face of the patch has: its border.
    border: std::ops::Range<usize>,
}

/// The boundary of a mesh.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Surface {
    positions: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
    /// The cell each triangle's face belongs to.
    triangle_cells: Vec<u32>,
    /// The first point of each face in `loops`, and one more at the end.
    face_starts: Vec<u32>,
    loops: Vec<u32>,
    /// The borders of the patches, one run per patch.
    borders: Vec<[u32; 2]>,
    pub patches: Vec<PatchSurface>,
    pub centre: [f32; 3],
    /// Half the longest side of the box around the surface.
    pub radius: f32,
    /// The box's extent along x, y and z.
    pub extent: [f32; 3],
    /// How many cells the mesh has.
    pub cells: usize,
    /// Set when the surface stopped at [`MAX_TRIANGLES`].
    pub truncated: bool,
}

impl Surface {
    /// The boundary faces of the mesh; faces that point outside the point list are left out.
    pub fn from_mesh(mesh: &RawPolyMesh) -> Self {
        let mut surface = Surface::default();
        let points: Vec<[f32; 3]> = mesh
            .points
            .iter()
            .map(|p| [p.x as f32, p.y as f32, p.z as f32])
            .collect();
        surface.face_starts.push(0);
        surface.cells = mesh
            .owner
            .iter()
            .chain(&mesh.neighbour)
            .max()
            .map_or(0, |&highest| highest.saturating_add(1));
        'patches: for patch in &mesh.patches {
            let first_triangle = surface.triangles.len();
            let first_face = surface.face_starts.len().saturating_sub(1);
            let end = patch.start_face.saturating_add(patch.n_faces);
            for face_index in patch.start_face..end.min(mesh.faces.len()) {
                let face = mesh.faces.face(face_index);
                if face.len() < 3 || face.iter().any(|&i| i >= points.len()) {
                    continue;
                }
                let triangles = face.len() - 2;
                if surface.triangles.len() + triangles > MAX_TRIANGLES {
                    surface.truncated = true;
                    surface.push_patch(patch, first_triangle, first_face);
                    break 'patches;
                }
                // A fan from the first point: boundary faces are convex in a mesh worth running.
                let Some(&a) = face.first() else { continue };
                for pair in face.windows(2).skip(1) {
                    if let [b, c] = pair {
                        surface.triangles.push([a as u32, *b as u32, *c as u32]);
                    }
                }
                let cell = mesh
                    .owner
                    .get(face_index)
                    .map_or(0, |&c| u32::try_from(c).unwrap_or(u32::MAX));
                surface
                    .triangle_cells
                    .extend(std::iter::repeat_n(cell, triangles));
                surface.loops.extend(face.iter().map(|&i| i as u32));
                surface.face_starts.push(surface.loops.len() as u32);
            }
            surface.push_patch(patch, first_triangle, first_face);
        }
        surface.fit(points);
        surface
    }

    fn push_patch(
        &mut self,
        patch: &vortia_foamfile::RawPatch,
        first_triangle: usize,
        first_face: usize,
    ) {
        let last_face = self.face_starts.len().saturating_sub(1);
        self.patches.push(PatchSurface {
            name: patch.name.clone(),
            kind: patch.kind.clone(),
            faces: last_face.saturating_sub(first_face),
            triangles: first_triangle..self.triangles.len(),
            face_range: first_face..last_face,
            border: 0..0,
        });
    }

    /// Keeps the points the faces use, renumbered, and measures the box around them.
    fn fit(&mut self, points: Vec<[f32; 3]>) {
        let mut renumber = vec![u32::MAX; points.len()];
        let mut kept = Vec::new();
        let mut take = |i: u32| -> u32 {
            let slot = renumber.get_mut(i as usize);
            match slot {
                Some(slot) if *slot != u32::MAX => *slot,
                Some(slot) => {
                    let new = kept.len() as u32;
                    *slot = new;
                    kept.push(points.get(i as usize).copied().unwrap_or_default());
                    new
                }
                None => 0,
            }
        };
        for triangle in &mut self.triangles {
            for i in triangle.iter_mut() {
                *i = take(*i);
            }
        }
        for i in &mut self.loops {
            *i = take(*i);
        }
        self.positions = kept;
        self.find_borders();

        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for p in &self.positions {
            for axis in 0..3 {
                if let (Some(v), Some(lo), Some(hi)) =
                    (p.get(axis), low.get_mut(axis), high.get_mut(axis))
                {
                    *lo = lo.min(*v);
                    *hi = hi.max(*v);
                }
            }
        }
        if self.positions.is_empty() || low.iter().chain(&high).any(|v| !v.is_finite()) {
            self.centre = [0.0; 3];
            self.extent = [0.0; 3];
            self.radius = 1.0;
            return;
        }
        for axis in 0..3 {
            if let (Some(lo), Some(hi), Some(c), Some(e)) = (
                low.get(axis),
                high.get(axis),
                self.centre.get_mut(axis),
                self.extent.get_mut(axis),
            ) {
                *c = (lo + hi) / 2.0;
                *e = hi - lo;
            }
        }
        let longest = self.extent.iter().copied().fold(0.0_f32, f32::max);
        self.radius = if longest > 0.0 { longest / 2.0 } else { 1.0 };
    }

    /// The border of each patch: the edges that belong to one of its faces only.
    fn find_borders(&mut self) {
        let mut borders = Vec::new();
        for patch in &mut self.patches {
            let mut uses: std::collections::HashMap<(u32, u32), u32> =
                std::collections::HashMap::new();
            for face in patch.face_range.clone() {
                let (Some(&a), Some(&b)) =
                    (self.face_starts.get(face), self.face_starts.get(face + 1))
                else {
                    continue;
                };
                let Some(ring) = self.loops.get(a as usize..b as usize) else {
                    continue;
                };
                for (i, &from) in ring.iter().enumerate() {
                    let to = ring.get((i + 1) % ring.len()).copied().unwrap_or(from);
                    if from != to {
                        *uses.entry((from.min(to), from.max(to))).or_insert(0) += 1;
                    }
                }
            }
            let first = borders.len();
            let mut edges: Vec<[u32; 2]> = uses
                .into_iter()
                .filter(|(_, count)| *count == 1)
                .map(|((a, b), _)| [a, b])
                .collect();
            edges.sort_unstable();
            borders.extend(edges);
            patch.border = first..borders.len();
        }
        self.borders = borders;
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn face_count(&self) -> usize {
        self.face_starts.len().saturating_sub(1)
    }

    /// Reads `<case>/constant/polyMesh` and builds its surface.
    pub fn read(case: &Path) -> Result<Self, String> {
        let mesh = RawPolyMesh::read_case(case).map_err(|e| e.to_string())?;
        if mesh.patches.is_empty() {
            return Err("The mesh has no boundary patches.".to_owned());
        }
        Ok(Self::from_mesh(&mesh))
    }
}

/// Reads the surface of a case on a thread; `wake` is called when it is ready.
pub fn spawn(case: PathBuf, wake: impl Fn() + Send + 'static) -> Receiver<Result<Surface, String>> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("vortia-surface".into())
        .spawn(move || {
            let surface = std::panic::catch_unwind(|| Surface::read(&case))
                .unwrap_or_else(|_| Err("The mesh could not be read.".to_owned()));
            let _ = tx.send(surface);
            wake();
        });
    // No thread, no surface: the viewer says it could not load.
    drop(spawned);
    rx
}

/// An orthographic camera that turns around the middle of the surface, with y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Turn around the vertical axis, in radians; 0 looks along -z.
    pub yaw: f32,
    /// Height above the horizon, in radians, kept short of the poles.
    pub pitch: f32,
    /// How much of the surface fills the view: 1 fits it, 2 shows half as much.
    pub zoom: f32,
    /// The slide of the picture, in units of the surface's radius.
    pub pan: [f32; 2],
}

const MAX_PITCH: f32 = 1.55;

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.4,
            zoom: 1.0,
            pan: [0.0; 2],
        }
    }
}

impl Camera {
    /// The first view of a surface: a flat one (a 2-D case) is seen face on, any other from
    /// a corner.
    pub fn fit(surface: &Surface) -> Self {
        let [x, y, z] = surface.extent;
        let longest = x.max(y).max(z);
        if longest > 0.0 && z < longest * 0.05 {
            Self {
                yaw: 0.0,
                pitch: 0.0,
                ..Self::default()
            }
        } else {
            Self::default()
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw - dx).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + dy).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.zoom = (self.zoom * factor).clamp(0.05, 500.0);
        }
    }

    /// The directions of the screen: right, up, and towards the viewer.
    fn basis(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let towards = [cp * sy, sp, cp * cy];
        let right = [cy, 0.0, -sy];
        let up = cross(towards, right);
        (right, up, towards)
    }

    /// The visible triangles, farthest first, on a screen of `size` pixels. `visible` says
    /// which patches are shown; a patch it does not list is shown.
    pub fn project(&self, surface: &Surface, size: [f32; 2], visible: &[bool]) -> Vec<Projected> {
        let (right, up, towards) = self.basis();
        let scale =
            size[0].min(size[1]) / 2.0 / surface.radius.max(f32::MIN_POSITIVE) * self.zoom * 0.9;
        let to_screen = |p: &[f32; 3]| -> [f32; 3] {
            let d = [
                p[0] - surface.centre[0],
                p[1] - surface.centre[1],
                p[2] - surface.centre[2],
            ];
            [dot(d, right), dot(d, up), dot(d, towards)]
        };
        let pixel = |c: [f32; 3]| -> [f32; 2] {
            [
                size[0] / 2.0 + (c[0] + self.pan[0] * surface.radius) * scale,
                size[1] / 2.0 - (c[1] + self.pan[1] * surface.radius) * scale,
            ]
        };
        let screen: Vec<[f32; 3]> = surface.positions.iter().map(to_screen).collect();

        let mut out = Vec::with_capacity(surface.triangles.len());
        for (patch_index, patch) in surface.patches.iter().enumerate() {
            if !visible.get(patch_index).copied().unwrap_or(true) {
                continue;
            }
            let first = patch.triangles.start;
            for (offset, triangle) in surface
                .triangles
                .get(patch.triangles.clone())
                .unwrap_or(&[])
                .iter()
                .enumerate()
            {
                let [Some(a), Some(b), Some(c)] = triangle.map(|i| screen.get(i as usize).copied())
                else {
                    continue;
                };
                // The normal in the camera's frame: its component towards the viewer is
                // the facing, and either side of a face is lit the same.
                let (ab, ac) = (sub(b, a), sub(c, a));
                let n = cross(ab, ac);
                let length = dot(n, n).sqrt();
                if !length.is_finite() {
                    continue;
                }
                let facing = if length > 0.0 {
                    (n[2] / length).abs()
                } else {
                    0.0
                };
                out.push(Projected {
                    points: [pixel(a), pixel(b), pixel(c)],
                    depth: (a[2] + b[2] + c[2]) / 3.0,
                    light: 0.35 + 0.65 * facing,
                    patch: patch_index,
                    cell: surface
                        .triangle_cells
                        .get(first + offset)
                        .map_or(0, |&c| c as usize),
                });
            }
        }
        out.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));
        out
    }

    /// The borders of the visible patches, with the patch each belongs to.
    pub fn borders(
        &self,
        surface: &Surface,
        size: [f32; 2],
        visible: &[bool],
    ) -> Vec<([[f32; 2]; 2], usize)> {
        let to_screen = self.pixel_projection(surface, size);
        let mut out = Vec::new();
        for (patch_index, patch) in surface.patches.iter().enumerate() {
            if !visible.get(patch_index).copied().unwrap_or(true) {
                continue;
            }
            for edge in surface.borders.get(patch.border.clone()).unwrap_or(&[]) {
                if let (Some(p), Some(q)) = (
                    surface.positions.get(edge[0] as usize),
                    surface.positions.get(edge[1] as usize),
                ) {
                    out.push(([to_screen(p), to_screen(q)], patch_index));
                }
            }
        }
        out
    }

    /// The screen position of a point of the surface, on a screen of `size` pixels.
    pub fn pixel_projection<'a>(
        &'a self,
        surface: &'a Surface,
        size: [f32; 2],
    ) -> impl Fn(&[f32; 3]) -> [f32; 2] + 'a {
        let (right, up, _) = self.basis();
        let scale =
            size[0].min(size[1]) / 2.0 / surface.radius.max(f32::MIN_POSITIVE) * self.zoom * 0.9;
        move |p| {
            let d = [
                p[0] - surface.centre[0],
                p[1] - surface.centre[1],
                p[2] - surface.centre[2],
            ];
            [
                size[0] / 2.0 + (dot(d, right) + self.pan[0] * surface.radius) * scale,
                size[1] / 2.0 - (dot(d, up) + self.pan[1] * surface.radius) * scale,
            ]
        }
    }

    /// The outlines of the faces of the visible patches, as pairs of screen points.
    pub fn outlines(
        &self,
        surface: &Surface,
        size: [f32; 2],
        visible: &[bool],
    ) -> Vec<[[f32; 2]; 2]> {
        let to_screen = self.pixel_projection(surface, size);
        let mut out = Vec::new();
        for (patch_index, patch) in surface.patches.iter().enumerate() {
            if !visible.get(patch_index).copied().unwrap_or(true) {
                continue;
            }
            for face in patch.face_range.clone() {
                let (Some(&a), Some(&b)) = (
                    surface.face_starts.get(face),
                    surface.face_starts.get(face + 1),
                ) else {
                    continue;
                };
                let Some(ring) = surface.loops.get(a as usize..b as usize) else {
                    continue;
                };
                for (i, &from) in ring.iter().enumerate() {
                    let to = ring.get((i + 1) % ring.len()).copied().unwrap_or(from);
                    if let (Some(p), Some(q)) = (
                        surface.positions.get(from as usize),
                        surface.positions.get(to as usize),
                    ) {
                        out.push([to_screen(p), to_screen(q)]);
                    }
                }
            }
        }
        out
    }
}

/// A triangle on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projected {
    pub points: [[f32; 2]; 3],
    /// Towards the viewer: larger is nearer.
    pub depth: f32,
    /// 0 to 1: how much light the face gets.
    pub light: f32,
    pub patch: usize,
    /// The cell the triangle's face belongs to.
    pub cell: usize,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir.join("constant/polyMesh")).unwrap();
        std::fs::write(
            dir.join("constant/polyMesh").join(name),
            format!("{HEADER}{body}"),
        )
        .unwrap();
    }

    /// A unit cube of one cell: two walls in one patch, the other four in another.
    fn cube(dir: &Path) {
        write(
            dir,
            "points",
            "8((0 0 0)(1 0 0)(1 1 0)(0 1 0)(0 0 1)(1 0 1)(1 1 1)(0 1 1))",
        );
        write(
            dir,
            "faces",
            "6(4(0 4 7 3)4(1 2 6 5)4(0 1 5 4)4(3 7 6 2)4(0 3 2 1)4(4 5 6 7))",
        );
        write(dir, "owner", "6(0 0 0 0 0 0)");
        write(dir, "neighbour", "0()");
        write(
            dir,
            "boundary",
            "2(sides{type wall;nFaces 2;startFace 0;}rest{type patch;nFaces 4;startFace 2;})",
        );
    }

    fn read_cube() -> Surface {
        let dir = tempfile::tempdir().unwrap();
        cube(dir.path());
        Surface::read(dir.path()).unwrap()
    }

    #[test]
    fn a_cube_is_twelve_triangles_in_two_patches() {
        let surface = read_cube();
        assert_eq!(surface.triangle_count(), 12);
        assert_eq!(surface.face_count(), 6);
        let faces: Vec<_> = surface
            .patches
            .iter()
            .map(|p| (p.name.as_str(), p.faces))
            .collect();
        assert_eq!(faces, [("sides", 2), ("rest", 4)]);
        assert_eq!(surface.centre, [0.5; 3]);
        assert_eq!(surface.extent, [1.0; 3]);
        assert_eq!(surface.radius, 0.5);
        assert!(!surface.truncated);
    }

    #[test]
    fn a_flat_surface_is_first_seen_face_on_and_a_solid_from_a_corner() {
        let cube = read_cube();
        assert_eq!(Camera::fit(&cube), Camera::default());
        let flat = Surface {
            extent: [10.0, 4.0, 0.1],
            ..Surface::default()
        };
        let camera = Camera::fit(&flat);
        assert_eq!((camera.yaw, camera.pitch), (0.0, 0.0));
    }

    #[test]
    fn the_nearest_triangles_come_last_and_a_hidden_patch_is_left_out() {
        let surface = read_cube();
        let camera = Camera {
            yaw: 0.0,
            pitch: 0.0,
            ..Camera::default()
        };
        let all = camera.project(&surface, [100.0, 100.0], &[true, true]);
        // The side walls are edge on, the front and back faces face the viewer.
        assert_eq!(all.len(), 12);
        assert!(all.windows(2).all(|w| w[0].depth <= w[1].depth));
        let facing: Vec<f32> = all.iter().map(|t| t.light).collect();
        assert!(facing.iter().any(|&l| (l - 1.0).abs() < 1e-6));
        let only_sides = camera.project(&surface, [100.0, 100.0], &[true, false]);
        assert_eq!(only_sides.len(), 4);
        assert!(only_sides.iter().all(|t| t.patch == 0));
    }

    #[test]
    fn the_picture_is_centred_and_fits_its_view() {
        let surface = read_cube();
        let camera = Camera {
            yaw: 0.0,
            pitch: 0.0,
            ..Camera::default()
        };
        let triangles = camera.project(&surface, [200.0, 100.0], &[]);
        let xs: Vec<f32> = triangles
            .iter()
            .flat_map(|t| t.points.map(|p| p[0]))
            .collect();
        let ys: Vec<f32> = triangles
            .iter()
            .flat_map(|t| t.points.map(|p| p[1]))
            .collect();
        let span = |v: &[f32]| {
            (
                v.iter().copied().fold(f32::INFINITY, f32::min),
                v.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            )
        };
        let (x0, x1) = span(&xs);
        let (y0, y1) = span(&ys);
        assert!(((x0 + x1) / 2.0 - 100.0).abs() < 1e-3);
        assert!(((y0 + y1) / 2.0 - 50.0).abs() < 1e-3);
        // The shorter side of the view holds the cube, with a margin.
        assert!(y1 - y0 < 100.0 && y1 - y0 > 80.0);
    }

    #[test]
    fn the_outlines_are_the_edges_of_the_faces() {
        let surface = read_cube();
        let lines = Camera::default().outlines(&surface, [100.0, 100.0], &[]);
        assert_eq!(lines.len(), 24);
        assert_eq!(
            Camera::default()
                .outlines(&surface, [100.0, 100.0], &[false, true])
                .len(),
            16
        );
    }

    #[test]
    fn the_border_of_a_patch_is_the_edges_only_one_of_its_faces_has() {
        let surface = read_cube();
        let camera = Camera::default();
        let borders = camera.borders(&surface, [100.0, 100.0], &[]);
        // Two opposite faces: eight edges, none shared. Four faces around: the ring's
        // eight outer edges, since the four edges between neighbours are shared.
        let of = |patch: usize| borders.iter().filter(|(_, p)| *p == patch).count();
        assert_eq!((of(0), of(1)), (8, 8));
        let hidden = camera.borders(&surface, [100.0, 100.0], &[false, true]);
        assert!(hidden.iter().all(|(_, p)| *p == 1));
    }

    #[test]
    fn the_camera_turns_and_zooms_within_its_limits() {
        let mut camera = Camera::default();
        camera.orbit(0.0, 100.0);
        assert_eq!(camera.pitch, MAX_PITCH);
        camera.orbit(-0.1, -300.0);
        assert_eq!(camera.pitch, -MAX_PITCH);
        assert!((0.0..std::f32::consts::TAU).contains(&camera.yaw));
        camera.zoom_by(f32::NAN);
        camera.zoom_by(-2.0);
        assert_eq!(camera.zoom, 1.0);
        camera.zoom_by(1e9);
        assert_eq!(camera.zoom, 500.0);
    }

    #[test]
    fn a_broken_mesh_gives_a_surface_without_the_broken_faces() {
        let dir = tempfile::tempdir().unwrap();
        cube(dir.path());
        // A face that names a point that is not there, and a patch that runs past the faces.
        write(
            dir.path(),
            "faces",
            "6(4(0 4 7 99)4(1 2 6 5)4(0 1 5 4)4(3 7 6 2)4(0 3 2 1)4(4 5 6 7))",
        );
        write(
            dir.path(),
            "boundary",
            "1(all{type patch;nFaces 400;startFace 0;})",
        );
        let surface = Surface::read(dir.path()).unwrap();
        assert_eq!(surface.face_count(), 5);
        assert_eq!(surface.patches.first().map(|p| p.faces), Some(5));
        let camera = Camera::default();
        assert_eq!(camera.project(&surface, [0.0, 0.0], &[]).len(), 10);
    }

    #[test]
    fn a_case_without_a_mesh_says_so() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Surface::read(dir.path()).is_err());
    }
}
