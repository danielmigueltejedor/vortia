//! `setFields` on a small tube. The same dictionary on the app's nozzle case
//! (8660 cells, 270 `boxToCell` regions) gives OpenFOAM v1912's fields to
//! the 9 digits it writes.

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_fields::{ScalarField, VectorField};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;
use vortia_solvers::set_fields;

mod common;

use common::{HEADER, write};

const CELLS: usize = 6;

/// A tube of six cells of length 1 (centres at 0.5 … 5.5) with a field of
/// each kind.
fn tube(dir: &Path, dict: &str) {
    let bounds = Aabb {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(6.0, 1.0, 1.0),
    };
    let mut raw = structured_box_raw(bounds, [CELLS, 1, 1]).unwrap();
    let p = raw.patches.clone();
    let merge = |name: &str, kind: &str, from: &[RawPatch]| RawPatch {
        name: name.into(),
        kind: kind.into(),
        start_face: from[0].start_face,
        n_faces: from.iter().map(|p| p.n_faces).sum(),
        groups: vec![kind.into()],
        dict: Default::default(),
    };
    raw.patches = vec![
        merge("ends", "patch", &p[0..2]),
        merge("sides", "empty", &p[2..6]),
    ];
    raw.write_case(dir).unwrap();
    let field = |name: &str, class: &str, dims: &str, internal: &str| {
        write(
            &dir.join("0").join(name),
            &format!(
                "FoamFile {{ version 2.0; format ascii; class {class}; object {name}; }}\n\
                 dimensions {dims}; internalField uniform {internal};\n\
                 boundaryField {{ ends {{ type zeroGradient; }} sides {{ type empty; }} }}\n"
            ),
        );
    };
    field("p", "volScalarField", "[1 -1 -2 0 0 0 0]", "1");
    field("T", "volScalarField", "[0 0 0 1 0 0 0]", "2");
    field("U", "volVectorField", "[0 1 -1 0 0 0 0]", "(7 7 7)");
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application rhoCentralFoam; startFrom startTime; startTime 0; \
             endTime 1; deltaT 1; writeControl timeStep; writeInterval 1;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!("{HEADER}ddtSchemes {{ default Euler; }}"),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!("{HEADER}solvers {{ }}"),
    );
    write(
        &dir.join("system/setFieldsDict"),
        &format!("{HEADER}{dict}"),
    );
}

fn scalar(dir: &Path, name: &str) -> Vec<f64> {
    ScalarField::read(dir.join("0").join(name))
        .unwrap()
        .internal
        .expand(CELLS)
        .unwrap()
}

#[test]
fn defaults_then_boxes_in_order_with_the_faces_of_a_box_inside_it() {
    let dir = tempfile::tempdir().unwrap();
    tube(
        dir.path(),
        "defaultFieldValues (volScalarFieldValue p 100 volScalarFieldValue T 300 \
         volVectorFieldValue U (0 0 0)); \
         regions ( \
           boxToCell { box (1 -1 -1) (3 2 2); fieldValues (volScalarFieldValue p 200); } \
           boxToCell { box (2.5 -1 -1) (5.5 2 2); fieldValues (volScalarFieldValue T 400 \
             volVectorFieldValue U (1 2 0) volScalarFieldValue p 50); } \
         );",
    );
    let mut log = String::new();
    set_fields::run(dir.path(), |l| log.push_str(l)).unwrap();
    // Centres 0.5 … 5.5: the first box holds cells 1 and 2, the second
    // cells 2 to 5 (2.5 and 5.5 are on its faces), and sets p after the first.
    assert_eq!(
        scalar(dir.path(), "p"),
        [100.0, 200.0, 50.0, 50.0, 50.0, 50.0]
    );
    assert_eq!(
        scalar(dir.path(), "T"),
        [300.0, 300.0, 400.0, 400.0, 400.0, 400.0]
    );
    let u = VectorField::read(dir.path().join("0/U"))
        .unwrap()
        .internal
        .expand(CELLS)
        .unwrap();
    let want = |i: usize| {
        if i < 2 {
            Vec3::default()
        } else {
            Vec3::new(1.0, 2.0, 0.0)
        }
    };
    for (i, v) in u.iter().enumerate() {
        assert_eq!(*v, want(i), "cell {i}");
    }
    assert!(log.contains("Set 3 fields in 2 regions"), "{log}");
    // What it read back is a field the solvers read.
    assert!(
        std::fs::read_to_string(dir.path().join("0/p"))
            .unwrap()
            .contains("zeroGradient")
    );
}

#[test]
fn what_it_cannot_set_is_refused_and_leaves_the_case_alone() {
    let refused = |dict: &str, needle: &str| {
        let dir = tempfile::tempdir().unwrap();
        tube(dir.path(), dict);
        let before = std::fs::read_to_string(dir.path().join("0/p")).unwrap();
        let e = set_fields::run(dir.path(), |_| {}).unwrap_err().to_string();
        assert!(e.contains(needle), "{dict}: {e}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("0/p")).unwrap(),
            before
        );
    };
    refused(
        "regions ( sphereToCell { centre (0 0 0); radius 1; fieldValues (volScalarFieldValue p 1); } );",
        "sphereToCell",
    );
    refused(
        "defaultFieldValues (volScalarFieldValue p 1 volVectorFieldValue p (1 0 0));",
        "scalar and as a vector",
    );
    refused(
        "defaultFieldValues (volTensorFieldValue p 1);",
        "volTensorFieldValue",
    );
    refused("defaultFieldValues (volScalarFieldValue p nan);", "finite");
    refused("defaultFieldValues (volScalarFieldValue rho 1);", "rho");
    refused(
        "regions ( boxToCell { box (0 0 0); fieldValues (volScalarFieldValue p 1); } );",
        "box",
    );
}
