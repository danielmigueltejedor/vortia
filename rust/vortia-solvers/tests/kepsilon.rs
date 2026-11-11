//! The engine's SIMPLE solver with the k-ε model against OpenFOAM's
//! `simpleFoam` on a turbulent channel and a turbulent cavity.
//!
//! The meshes are the engine's own boxes, written to disk so OpenFOAM v1912
//! ran on identical meshes. The reference numbers come from those runs
//! (`writePrecision 17`).

// The reference numbers are copied as OpenFOAM printed them.
#![allow(clippy::excessive_precision)]

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;

const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn field(dir: &Path, name: &str, class: &str, dims: &str, internal: &str, patches: &str) {
    write(
        &dir.join("0").join(name),
        &format!(
            "FoamFile {{ version 2.0; format ascii; class {class}; object {name}; }}\n\
             dimensions {dims}; internalField uniform {internal};\n\
             boundaryField {{ {patches} frontAndBack {{ type empty; }} }}\n"
        ),
    );
}

fn merge(name: &str, kind: &str, from: &[RawPatch]) -> RawPatch {
    RawPatch {
        name: name.into(),
        kind: kind.into(),
        start_face: from[0].start_face,
        n_faces: from.iter().map(|p| p.n_faces).sum(),
        groups: vec![kind.into()],
        dict: Default::default(),
    }
}

/// How a run is set up.
struct Setup {
    /// A channel with an inlet and an outlet, or a cavity with a lid.
    channel: bool,
    end: &'static str,
    div: &'static str,
    simple: &'static str,
    relax: &'static str,
}

/// A 1 m × 0.1 m channel, 40 × 10 cells, air at 10 m/s with 5 % intensity.
const CHANNEL: Setup = Setup {
    channel: true,
    end: "2000",
    div: "bounded Gauss upwind",
    simple: "nNonOrthogonalCorrectors 0; \
             residualControl { p 1e-3; U 1e-4; \"(k|epsilon)\" 1e-4; }",
    relax: "fields { p 0.3; } equations { U 0.7; \".*\" 0.7; }",
};

/// A 0.1 m cavity, 20 × 20 cells, its lid at 1 m/s, ν = 1e-5: the corners
/// where two walls meet, and wall cells below the log layer.
const CAVITY: Setup = Setup {
    channel: false,
    end: "150",
    div: "bounded Gauss linear",
    simple: "nNonOrthogonalCorrectors 0; pRefCell 0; pRefValue 0;",
    relax: "fields { p 0.3; } equations { U 0.7; k 0.6; epsilon 0.6; }",
};

fn case(dir: &Path, s: &Setup) {
    let (bounds, cells) = if s.channel {
        (Vec3::new(1.0, 0.1, 0.01), [40, 10, 1])
    } else {
        (Vec3::new(0.1, 0.1, 0.01), [20, 20, 1])
    };
    let mut raw = structured_box_raw(
        Aabb {
            min: Vec3::default(),
            max: bounds,
        },
        cells,
    )
    .unwrap();
    // xMin, xMax, yMin, yMax, zMin, zMax
    let p = raw.patches.clone();
    raw.patches = if s.channel {
        vec![
            merge("inlet", "patch", &p[0..1]),
            merge("outlet", "patch", &p[1..2]),
            merge("walls", "wall", &p[2..4]),
            merge("frontAndBack", "empty", &p[4..6]),
        ]
    } else {
        vec![
            merge("fixedWalls", "wall", &p[0..3]),
            merge("movingWall", "wall", &p[3..4]),
            merge("frontAndBack", "empty", &p[4..6]),
        ]
    };
    raw.write_case(dir).unwrap();

    let div = s.div;
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application simpleFoam; startFrom startTime; startTime 0; stopAt endTime; \
             endTime {}; deltaT 1; writeControl timeStep; writeInterval 50; \
             writeFormat ascii; writePrecision 17; timePrecision 6;",
            s.end
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default steadyState; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,U) {div}; div(phi,k) bounded Gauss upwind; \
             div(phi,epsilon) bounded Gauss upwind; \
             div((nuEff*dev2(T(grad(U))))) Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear orthogonal; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default orthogonal; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p {{ solver PCG; preconditioner DIC; tolerance 1e-07; relTol 0.05; }} \
               \"(U|k|epsilon)\" {{ solver smoothSolver; smoother symGaussSeidel; \
                 tolerance 1e-07; relTol 0.1; }} \
             }} \
             SIMPLE {{ {} }} relaxationFactors {{ {} }}",
            s.simple, s.relax
        ),
    );
    write(
        &dir.join("constant/transportProperties"),
        &format!("{HEADER}transportModel Newtonian; nu 1e-05;"),
    );
    write(
        &dir.join("constant/turbulenceProperties"),
        &format!(
            "{HEADER}simulationType RAS; RAS {{ RASModel kEpsilon; turbulence on; printCoeffs on; }}"
        ),
    );

    let vector = "volVectorField";
    let scalar = "volScalarField";
    if s.channel {
        let (k, eps) = ("0.375", "14.855");
        field(
            dir,
            "U",
            vector,
            "[0 1 -1 0 0 0 0]",
            "(10 0 0)",
            "inlet { type fixedValue; value uniform (10 0 0); } \
             outlet { type zeroGradient; } walls { type noSlip; }",
        );
        field(
            dir,
            "p",
            scalar,
            "[0 2 -2 0 0 0 0]",
            "0",
            "inlet { type zeroGradient; } outlet { type fixedValue; value uniform 0; } \
             walls { type zeroGradient; }",
        );
        let kind = |name: &str, wall: &str, v: &str| {
            format!(
                "inlet {{ type fixedValue; value uniform {v}; }} \
                 outlet {{ type zeroGradient; }} walls {{ type {wall}; value uniform {v}; }}{name:.0}"
            )
        };
        field(
            dir,
            "k",
            scalar,
            "[0 2 -2 0 0 0 0]",
            k,
            &kind("k", "kqRWallFunction", k),
        );
        field(
            dir,
            "epsilon",
            scalar,
            "[0 2 -3 0 0 0 0]",
            eps,
            &kind("epsilon", "epsilonWallFunction", eps),
        );
        field(
            dir,
            "nut",
            scalar,
            "[0 2 -1 0 0 0 0]",
            "0",
            "inlet { type calculated; value uniform 0; } \
             outlet { type calculated; value uniform 0; } \
             walls { type nutkWallFunction; value uniform 0; }",
        );
    } else {
        let (k, eps) = ("0.00375", "0.0012");
        field(
            dir,
            "U",
            vector,
            "[0 1 -1 0 0 0 0]",
            "(0 0 0)",
            "fixedWalls { type noSlip; } movingWall { type fixedValue; value uniform (1 0 0); }",
        );
        field(
            dir,
            "p",
            scalar,
            "[0 2 -2 0 0 0 0]",
            "0",
            "fixedWalls { type zeroGradient; } movingWall { type zeroGradient; }",
        );
        field(
            dir,
            "k",
            scalar,
            "[0 2 -2 0 0 0 0]",
            k,
            &format!(
                "fixedWalls {{ type kqRWallFunction; value uniform {k}; }} \
                 movingWall {{ type kqRWallFunction; value uniform {k}; }}"
            ),
        );
        field(
            dir,
            "epsilon",
            scalar,
            "[0 2 -3 0 0 0 0]",
            eps,
            &format!(
                "fixedWalls {{ type epsilonWallFunction; value uniform {eps}; }} \
                 movingWall {{ type epsilonWallFunction; value uniform {eps}; }}"
            ),
        );
        field(
            dir,
            "nut",
            scalar,
            "[0 2 -1 0 0 0 0]",
            "0",
            "fixedWalls { type nutkWallFunction; value uniform 0; } \
             movingWall { type nutkWallFunction; value uniform 0; }",
        );
    }
}

/// Writes a case for OpenFOAM:
/// `VORTIA_CASE=dir cargo test --test kepsilon write_kepsilon_case_for_openfoam -- --ignored`,
/// with `VORTIA_CAVITY=1` for the cavity.
#[test]
#[ignore]
fn write_kepsilon_case_for_openfoam() {
    let dir = std::env::var("VORTIA_CASE").unwrap();
    let setup = if std::env::var("VORTIA_CAVITY").is_ok() {
        &CAVITY
    } else {
        &CHANNEL
    };
    case(Path::new(&dir), setup);
}
