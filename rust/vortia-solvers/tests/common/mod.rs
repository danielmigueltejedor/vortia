//! What the solver tests share: writing case files, and the lid-driven
//! cavity that icoFoam and simpleFoam are both checked on.

// Each test binary uses its own part of this module.
#![allow(dead_code)]

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;

pub const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

pub fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The classic cavity's mesh and initial fields: 0.1 m square, one cell
/// deep, the lid moving at 1 m/s. Each test writes its own `system/` and
/// `constant/`.
pub fn cavity_mesh_and_fields(dir: &Path, cells: usize) {
    let bounds = Aabb {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(0.1, 0.1, 0.01),
    };
    let mut raw = structured_box_raw(bounds, [cells, cells, 1]).unwrap();
    // xMin, xMax, yMin | yMax | zMin, zMax
    let p = &raw.patches;
    let merge = |name: &str, kind: &str, from: &[RawPatch]| RawPatch {
        name: name.into(),
        kind: kind.into(),
        start_face: from[0].start_face,
        n_faces: from.iter().map(|p| p.n_faces).sum(),
        groups: vec![kind.into()],
        dict: Default::default(),
    };
    raw.patches = vec![
        merge("movingWall", "wall", &p[3..4]),
        merge("fixedWalls", "wall", &p[0..3]),
        merge("frontAndBack", "empty", &p[4..6]),
    ];
    // movingWall must come after fixedWalls on disk, as the faces do.
    raw.patches.swap(0, 1);
    raw.write_case(dir).unwrap();

    write(
        &dir.join("0/U"),
        "FoamFile { version 2.0; format ascii; class volVectorField; object U; }\n\
         dimensions [0 1 -1 0 0 0 0]; internalField uniform (0 0 0);\n\
         boundaryField {\n\
           movingWall { type fixedValue; value uniform (1 0 0); }\n\
           fixedWalls { type noSlip; }\n\
           frontAndBack { type empty; }\n\
         }\n",
    );
    write(
        &dir.join("0/p"),
        "FoamFile { version 2.0; format ascii; class volScalarField; object p; }\n\
         dimensions [0 2 -2 0 0 0 0]; internalField uniform 0;\n\
         boundaryField {\n\
           movingWall { type zeroGradient; }\n\
           fixedWalls { type zeroGradient; }\n\
           frontAndBack { type empty; }\n\
         }\n",
    );
}
