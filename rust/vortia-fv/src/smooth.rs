//! Smoothing of a cell field so that no two neighbouring cells differ by more
//! than a ratio: OpenFOAM's `fvc::smooth`, which `rhoCentralFoam`'s local
//! time stepping applies to the reciprocal time step.
//!
//! Derived from `fvc::smooth`, `smoothData` and `FaceCellWave` (OpenFOAM,
//! Copyright (C) 2011-2017 OpenFOAM Foundation and OpenCFD Ltd.,
//! GPL-3.0-or-later). The result depends on the order the wave visits faces
//! and cells, so the same order is kept: changed faces in the order the owner
//! loop found them, each cell's faces as the owner faces then the neighbour
//! faces, ascending.

use vortia_mesh::PolyMesh;

/// OpenFOAM's `great` and `small` for double precision.
const GREAT: f64 = 1.0e15;
const SMALL: f64 = 1.0e-15;
const V_SMALL: f64 = 1.0e-300;
/// `FaceCellWave::propagationTol_`.
const PROPAGATION_TOL: f64 = 0.01;

/// `smoothData::update`: takes over `other` where `value` has none or is
/// smaller than `scale` times `other` by more than the tolerance.
fn update(value: &mut f64, other: f64, scale: f64) -> bool {
    // `!(value > -small)`, which holds for NaN too.
    if value.is_nan() || *value <= -SMALL || *value < V_SMALL {
        *value = other;
        true
    } else if other > (1.0 + PROPAGATION_TOL) * scale * *value {
        *value = other / scale;
        true
    } else {
        false
    }
}

/// Raises each cell of `field` to within `1 + coeff` times its neighbours'
/// values, as `fvc::smooth(field, coeff)` does.
pub fn smooth(mesh: &PolyMesh, field: &mut [f64], coeff: f64) {
    let max_ratio = 1.0 + coeff;
    let (own, nei) = (mesh.owner(), mesh.neighbour());
    let n_faces = mesh.n_faces();
    let get = |f: &[f64], c: usize| f.get(c).copied().unwrap_or(0.0);

    let mut face_info = vec![-GREAT; n_faces];
    let mut changed_face = vec![false; n_faces];
    let mut changed_faces: Vec<usize> = Vec::new();
    for (f, (&o, &n)) in own.iter().zip(nei).enumerate() {
        let (fo, fneigh) = (get(field, o), get(field, n));
        let seed = if fo > max_ratio * fneigh {
            fo
        } else if fneigh > max_ratio * fo {
            fneigh
        } else {
            continue;
        };
        if let Some(info) = face_info.get_mut(f)
            && *info != seed
        {
            *info = seed;
            if let Some(c) = changed_face.get_mut(f) {
                *c = true;
            }
            changed_faces.push(f);
        }
    }
    if changed_faces.is_empty() {
        return;
    }

    // `primitiveMesh::cells()`: the faces a cell owns, then those it neighbours.
    let mut cell_faces: Vec<Vec<usize>> = vec![Vec::new(); mesh.n_cells()];
    for f in 0..n_faces {
        if let Some(c) = own.get(f).and_then(|&o| cell_faces.get_mut(o)) {
            c.push(f);
        }
    }
    for (f, &n) in nei.iter().enumerate() {
        if let Some(c) = cell_faces.get_mut(n) {
            c.push(f);
        }
    }

    let mut changed_cell = vec![false; mesh.n_cells()];
    let mut changed_cells: Vec<usize> = Vec::new();
    for _ in 0..=mesh.n_cells() {
        // faceToCell
        for &f in &changed_faces {
            let info = face_info.get(f).copied().unwrap_or(-GREAT);
            let cells = [own.get(f).copied(), nei.get(f).copied()];
            for c in cells.into_iter().flatten() {
                let Some(value) = field.get_mut(c) else {
                    continue;
                };
                if *value != info
                    && update(value, info, max_ratio)
                    && let Some(flag) = changed_cell.get_mut(c)
                    && !*flag
                {
                    *flag = true;
                    changed_cells.push(c);
                }
            }
            if let Some(flag) = changed_face.get_mut(f) {
                *flag = false;
            }
        }
        changed_faces.clear();
        if changed_cells.is_empty() {
            break;
        }
        // cellToFace
        for &c in &changed_cells {
            let value = get(field, c);
            for &f in cell_faces.get(c).map_or(&[][..], Vec::as_slice) {
                let Some(info) = face_info.get_mut(f) else {
                    continue;
                };
                if *info != value
                    && update(info, value, 1.0)
                    && let Some(flag) = changed_face.get_mut(f)
                    && !*flag
                {
                    *flag = true;
                    changed_faces.push(f);
                }
            }
            if let Some(flag) = changed_cell.get_mut(c) {
                *flag = false;
            }
        }
        changed_cells.clear();
        if changed_faces.is_empty() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use vortia_core::{Aabb, Vec3};
    use vortia_mesh::blocks::structured_box;

    use super::*;

    fn line(n: usize) -> PolyMesh {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(n as f64, 1.0, 1.0),
        };
        structured_box(bounds, [n, 1, 1]).unwrap()
    }

    #[test]
    fn a_spike_spreads_into_a_slope_no_steeper_than_the_ratio() {
        let mesh = line(12);
        let mut field = vec![1.0; 12];
        field[5] = 100.0;
        smooth(&mesh, &mut field, 0.5);
        assert_eq!(field[5], 100.0);
        for w in field.windows(2) {
            let (hi, lo) = (w[0].max(w[1]), w[0].min(w[1]));
            assert!(hi <= 1.5 * 1.01 * lo + 1e-12, "{field:?}");
        }
        // Nothing is lowered.
        assert!(field.iter().all(|&x| x >= 1.0));
    }

    #[test]
    fn a_field_already_within_the_ratio_is_left_alone() {
        let mesh = line(6);
        let mut field = vec![1.0, 1.1, 1.2, 1.1, 1.0, 1.05];
        let before = field.clone();
        smooth(&mesh, &mut field, 0.5);
        assert_eq!(field, before);
    }

    #[test]
    fn hostile_fields_do_not_panic() {
        let mesh = line(5);
        for mut f in [
            vec![f64::NAN; 5],
            vec![f64::INFINITY, 0.0, -1.0, 1e300, 1e-300],
            vec![1.0; 2],
            vec![],
        ] {
            smooth(&mesh, &mut f, 0.02);
            smooth(&mesh, &mut f, -5.0);
        }
    }
}
