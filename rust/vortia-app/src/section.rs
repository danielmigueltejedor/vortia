//! A section through the volume for the viewer: the polygons where a plane across one axis cuts
//! the cells, from the post-processing library, computed off the UI thread.
//!
//! The case is read once and kept (`load`), because moving the plane asks for a new section
//! at every step.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use vortia_core::Vec3;
use vortia_post::Case;

/// The cut of one cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    pub cell: usize,
    pub points: Vec<[f32; 3]>,
}

/// The plane across `axis` at `offset`, and the cells it cuts.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub axis: usize,
    pub offset: f32,
    pub polygons: Vec<Polygon>,
}

/// The section across `axis` (0, 1 or 2 for x, y, z) at `fraction` (0 to 1) of the way through
/// the box of the mesh.
pub fn compute(case: &Case, axis: usize, fraction: f64) -> Result<Section, String> {
    let bounds = case.bounds();
    let along = |v: Vec3| match axis {
        0 => Some(v.x),
        1 => Some(v.y),
        2 => Some(v.z),
        _ => None,
    };
    let (Some(low), Some(high)) = (along(bounds.min), along(bounds.max)) else {
        return Err(format!("axis {axis} is not x, y or z"));
    };
    if !fraction.is_finite() {
        return Err("the position of the section is not a number".to_owned());
    }
    let offset = low + (high - low) * fraction.clamp(0.0, 1.0);
    let (origin, normal) = match axis {
        0 => (Vec3::new(offset, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        1 => (Vec3::new(0.0, offset, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        _ => (Vec3::new(0.0, 0.0, offset), Vec3::new(0.0, 0.0, 1.0)),
    };
    let polygons = case
        .slice(origin, normal)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| Polygon {
            cell: p.cell,
            points: p
                .points
                .iter()
                .map(|q| [q.x as f32, q.y as f32, q.z as f32])
                .collect(),
        })
        .collect();
    Ok(Section {
        axis,
        offset: offset as f32,
        polygons,
    })
}

/// Reads the case on a thread; `wake` is called when it is ready.
pub fn load(dir: PathBuf, wake: impl Fn() + Send + 'static) -> Receiver<Result<Arc<Case>, String>> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("vortia-case".into())
        .spawn(move || {
            let read = std::panic::catch_unwind(|| {
                Case::open(&dir).map(Arc::new).map_err(|e| e.to_string())
            })
            .unwrap_or_else(|_| Err("The case could not be read.".to_owned()));
            let _ = tx.send(read);
            wake();
        });
    drop(spawned);
    rx
}

/// Computes a section on a thread; `wake` is called when it is ready.
pub fn spawn(
    case: Arc<Case>,
    axis: usize,
    fraction: f64,
    wake: impl Fn() + Send + 'static,
) -> Receiver<Result<Section, String>> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("vortia-section".into())
        .spawn(move || {
            let done = std::panic::catch_unwind(|| compute(&case, axis, fraction))
                .unwrap_or_else(|_| Err("The section could not be computed.".to_owned()));
            let _ = tx.send(done);
            wake();
        });
    drop(spawned);
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box_raw;

    fn case() -> (tempfile::TempDir, Case) {
        let dir = tempfile::tempdir().unwrap();
        let bounds = Aabb {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(4.0, 2.0, 1.0),
        };
        structured_box_raw(bounds, [4, 2, 1])
            .unwrap()
            .write_case(dir.path())
            .unwrap();
        let case = Case::open(dir.path()).unwrap();
        (dir, case)
    }

    #[test]
    fn a_section_is_the_cells_the_plane_crosses() {
        let (_dir, case) = case();
        let section = compute(&case, 0, 0.4).unwrap();
        assert_eq!(section.axis, 0);
        assert!((section.offset - 1.6).abs() < 1e-6);
        // One column of two cells, each cut in a square.
        assert_eq!(section.polygons.len(), 2);
        assert!(
            section
                .polygons
                .iter()
                .all(|p| p.points.len() == 4 && p.points.iter().all(|q| (q[0] - 1.6).abs() < 1e-6))
        );
        // Across y at the far end.
        let top = compute(&case, 1, 1.0).unwrap();
        assert!((top.offset - 2.0).abs() < 1e-6);
        assert!(!top.polygons.is_empty());
    }

    #[test]
    fn a_section_refuses_what_is_not_an_axis_or_a_place() {
        let (_dir, case) = case();
        assert!(compute(&case, 3, 0.5).is_err());
        assert!(compute(&case, 0, f64::NAN).is_err());
        // Past the ends is the end.
        let past = compute(&case, 0, 7.0).unwrap();
        assert!((past.offset - 4.0).abs() < 1e-6);
    }

    #[test]
    fn the_case_loads_on_a_thread_and_a_missing_one_says_so() {
        let (dir, _) = case();
        let rx = load(dir.path().to_owned(), || {});
        assert!(rx.recv().unwrap().is_ok());
        let empty = tempfile::tempdir().unwrap();
        let rx = load(empty.path().to_owned(), || {});
        assert!(rx.recv().unwrap().is_err());
    }
}
