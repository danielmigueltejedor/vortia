//! Structured hexahedral meshes of a box, built in memory.
//!
//! The first piece of the engine's own `blockMesh`: one block, uniform
//! spacing. Faces come out in the order the solver needs (internal faces
//! upper-triangular, then one patch per side), so the result can be solved on
//! directly and compared with the same box meshed by OpenFOAM.

use vortia_core::{Aabb, Vec3};
use vortia_foamfile::{Dictionary, FaceList, RawPatch, RawPolyMesh};

use crate::error::{MeshError, Result};
use crate::polymesh::PolyMesh;

/// The most cells one box may have, so a typo cannot ask for terabytes.
pub const MAX_CELLS: usize = 1 << 27;

/// The six patches, in the order they are written.
pub const PATCH_NAMES: [&str; 6] = ["xMin", "xMax", "yMin", "yMax", "zMin", "zMax"];

/// A box split into `cells[0] × cells[1] × cells[2]` equal hexahedra.
pub fn structured_box(bounds: Aabb, cells: [usize; 3]) -> Result<PolyMesh> {
    let [nx, ny, nz] = cells;
    if nx == 0 || ny == 0 || nz == 0 {
        return Err(MeshError::Invalid(
            "a box needs at least one cell per direction".into(),
        ));
    }
    let n_cells = nx
        .checked_mul(ny)
        .and_then(|v| v.checked_mul(nz))
        .filter(|&n| n <= MAX_CELLS)
        .ok_or_else(|| {
            MeshError::Invalid(format!("{nx}×{ny}×{nz} cells is more than {MAX_CELLS}"))
        })?;
    let size = bounds.size();
    if !(size.x > 0.0 && size.y > 0.0 && size.z > 0.0) {
        return Err(MeshError::Invalid("the box has no volume".into()));
    }

    let (px, py) = (nx + 1, ny + 1);
    let pt = |i: usize, j: usize, k: usize| i + px * (j + py * k);
    let mut points = Vec::with_capacity(px * py * (nz + 1));
    for k in 0..=nz {
        for j in 0..=ny {
            for i in 0..=nx {
                let t = |n: usize, of: usize| n as f64 / of as f64;
                points.push(Vec3::new(
                    bounds.min.x + size.x * t(i, nx),
                    bounds.min.y + size.y * t(j, ny),
                    bounds.min.z + size.z * t(k, nz),
                ));
            }
        }
    }

    // The face on the high side of point (i,j,k) normal to each axis, ordered
    // so its normal points along +axis.
    let x_face = |i, j, k| {
        [
            pt(i, j, k),
            pt(i, j + 1, k),
            pt(i, j + 1, k + 1),
            pt(i, j, k + 1),
        ]
    };
    let y_face = |i, j, k| {
        [
            pt(i, j, k),
            pt(i, j, k + 1),
            pt(i + 1, j, k + 1),
            pt(i + 1, j, k),
        ]
    };
    let z_face = |i, j, k| {
        [
            pt(i, j, k),
            pt(i + 1, j, k),
            pt(i + 1, j + 1, k),
            pt(i, j + 1, k),
        ]
    };
    let cell = |i: usize, j: usize, k: usize| i + nx * (j + ny * k);

    let mut faces = FaceList {
        offsets: vec![0],
        points: Vec::new(),
    };
    // Each cell owns at most three internal faces and three boundary faces.
    let mut owner = Vec::with_capacity(n_cells.saturating_mul(3));
    let mut neighbour = Vec::with_capacity(n_cells.saturating_mul(3));
    let push = |faces: &mut FaceList, f: [usize; 4], reverse: bool| {
        if reverse {
            faces.points.extend(f.iter().rev());
        } else {
            faces.points.extend(f);
        }
        faces.offsets.push(faces.points.len());
    };

    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let c = cell(i, j, k);
                if i + 1 < nx {
                    push(&mut faces, x_face(i + 1, j, k), false);
                    owner.push(c);
                    neighbour.push(cell(i + 1, j, k));
                }
                if j + 1 < ny {
                    push(&mut faces, y_face(i, j + 1, k), false);
                    owner.push(c);
                    neighbour.push(cell(i, j + 1, k));
                }
                if k + 1 < nz {
                    push(&mut faces, z_face(i, j, k + 1), false);
                    owner.push(c);
                    neighbour.push(cell(i, j, k + 1));
                }
            }
        }
    }

    let mut patches = Vec::with_capacity(6);
    let mut add_patch = |name: &str,
                         faces: &mut FaceList,
                         owner: &mut Vec<usize>,
                         list: Vec<([usize; 4], usize)>,
                         reverse: bool| {
        let start = owner.len();
        for (f, c) in list {
            push(faces, f, reverse);
            owner.push(c);
        }
        patches.push(RawPatch {
            name: name.to_owned(),
            kind: "patch".to_owned(),
            start_face: start,
            n_faces: owner.len() - start,
            groups: Vec::new(),
            dict: Dictionary::new(),
        });
    };
    let side = |axis: usize, high: bool| -> Vec<([usize; 4], usize)> {
        let mut out = Vec::new();
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let (on, f) = match axis {
                        0 => (
                            if high { i + 1 == nx } else { i == 0 },
                            x_face(if high { nx } else { 0 }, j, k),
                        ),
                        1 => (
                            if high { j + 1 == ny } else { j == 0 },
                            y_face(i, if high { ny } else { 0 }, k),
                        ),
                        _ => (
                            if high { k + 1 == nz } else { k == 0 },
                            z_face(i, j, if high { nz } else { 0 }),
                        ),
                    };
                    if on {
                        out.push((f, cell(i, j, k)));
                    }
                }
            }
        }
        out
    };
    for (n, name) in PATCH_NAMES.iter().enumerate() {
        let (axis, high) = (n / 2, n % 2 == 1);
        add_patch(name, &mut faces, &mut owner, side(axis, high), !high);
    }

    PolyMesh::from_raw(RawPolyMesh {
        points,
        faces,
        owner,
        neighbour,
        patches,
    })
}
