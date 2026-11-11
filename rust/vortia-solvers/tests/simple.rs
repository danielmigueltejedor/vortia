//! The engine's SIMPLE solver against OpenFOAM's `simpleFoam` (laminar) on
//! the lid-driven cavity run to steady state.
//!
//! The mesh is the engine's own box, written to disk so OpenFOAM v1912 ran on
//! the identical mesh. The reference numbers come from that run
//! (`writePrecision 17`).

// The reference numbers are copied as OpenFOAM printed them.
#![allow(clippy::excessive_precision)]

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_fields::{ScalarField, VectorField};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;
use vortia_solvers::{Case, simple};

const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// How a run of the cavity is set up.
struct Setup {
    cells: usize,
    end: &'static str,
    div: &'static str,
    laplacian: &'static str,
    u_solver: &'static str,
    simple: &'static str,
    relax: &'static str,
}

/// The cavity run to steady state, as the tutorial's settings would for
/// simpleFoam: bounded upwind, U relaxed by 0.7 and p by 0.3.
const STEADY: Setup = Setup {
    cells: 20,
    end: "2000",
    div: "bounded Gauss upwind",
    laplacian: "Gauss linear orthogonal",
    u_solver: "solver smoothSolver; smoother symGaussSeidel; tolerance 1e-05; relTol 0.1;",
    simple: "nNonOrthogonalCorrectors 0; pRefCell 0; pRefValue 0; \
             residualControl { p 1e-5; U 1e-5; }",
    relax: "fields { p 0.3; } equations { U 0.7; }",
};

/// The rest: linear convection, PBiCGStab for U, the old flat relaxation
/// form, a non-orthogonal corrector, the reference elsewhere, no residual
/// control and no momentum predictor.
const VARIANT: Setup = Setup {
    cells: 16,
    end: "60",
    div: "Gauss linear",
    laplacian: "Gauss linear uncorrected",
    u_solver: "solver PBiCGStab; preconditioner DILU; tolerance 1e-08; relTol 0.01;",
    simple: "nNonOrthogonalCorrectors 1; pRefCell 37; pRefValue 2.5; momentumPredictor no;",
    relax: "p 0.2; U 0.6;",
};

/// The classic cavity: 0.1 m square, lid at 1 m/s, ν = 0.01, Re = 10.
fn cavity(dir: &Path, s: &Setup) {
    let Setup {
        cells,
        end,
        div,
        laplacian,
        u_solver,
        simple,
        relax,
    } = *s;
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
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application simpleFoam; startFrom startTime; startTime 0; stopAt endTime; \
             endTime {end}; deltaT 1; writeControl timeStep; writeInterval 20; \
             writeFormat ascii; writePrecision 17; timePrecision 6;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default steadyState; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,U) {div}; \
             div((nuEff*dev2(T(grad(U))))) Gauss linear; }} \
             laplacianSchemes {{ default {laplacian}; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default orthogonal; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p {{ solver PCG; preconditioner DIC; tolerance 1e-06; relTol 0.05; }} \
               U {{ {u_solver} }} \
             }} \
             SIMPLE {{ {simple} }} relaxationFactors {{ {relax} }}"
        ),
    );
    write(
        &dir.join("constant/transportProperties"),
        &format!("{HEADER}transportModel Newtonian; nu 0.01;"),
    );
    write(
        &dir.join("constant/turbulenceProperties"),
        &format!("{HEADER}simulationType laminar;"),
    );
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

/// Writes the case for OpenFOAM:
/// `VORTIA_CAVITY=dir cargo test --test simple write_steady_cavity_for_openfoam -- --ignored`.
#[test]
#[ignore]
fn write_steady_cavity_for_openfoam() {
    let dir = std::env::var("VORTIA_CAVITY").unwrap();
    let setup = if std::env::var("VORTIA_VARIANT").is_ok() {
        &VARIANT
    } else {
        &STEADY
    };
    cavity(Path::new(&dir), setup);
}

struct Reference {
    first_steps: [usize; 8],
    totals: [(&'static str, usize); 3],
    iterations: usize,
    end: &'static str,
    /// (cell, U, p) at the end.
    cells: [(usize, [f64; 2], f64); 3],
    face_100_flux: f64,
}

fn check(setup: &Setup, r: &Reference) {
    let dir = tempfile::tempdir().unwrap();
    cavity(dir.path(), setup);
    let case = Case::open(dir.path()).unwrap();
    let steps = simple::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), r.iterations);
    assert!(steps.last().unwrap().written);

    let counts: Vec<usize> = steps
        .iter()
        .flat_map(|s| s.solves.iter().map(|(_, p)| p.iterations))
        .collect();
    assert_eq!(counts[..8], r.first_steps);
    for (field, total) in r.totals {
        let sum: usize = steps
            .iter()
            .flat_map(|s| &s.solves)
            .filter(|(f, _)| f == field)
            .map(|(_, p)| p.iterations)
            .sum();
        assert_eq!(sum, total, "iterations solving {field}");
    }

    let end = dir.path().join(r.end);
    let n = setup.cells * setup.cells;
    let u = VectorField::read(end.join("U")).unwrap();
    let p = ScalarField::read(end.join("p")).unwrap();
    let u = u.internal.expand(n).unwrap();
    let p = p.internal.expand(n).unwrap();
    let close = |a: f64, b: f64, scale: f64| (a - b).abs() <= 1e-12 * scale;
    for (c, [ux, uy], pc) in r.cells {
        assert!(
            close(u[c].x, ux, 1.0) && close(u[c].y, uy, 1.0),
            "U in cell {c}: {:?}",
            u[c]
        );
        assert_eq!(u[c].z, 0.0);
        assert!(close(p[c], pc, 10.0), "p in cell {c}: {}", p[c]);
    }
    let phi = ScalarField::read(end.join("phi"))
        .unwrap()
        .internal
        .expand(2 * setup.cells * (setup.cells - 1))
        .unwrap()[100];
    assert!(close(phi, r.face_100_flux, 1e-4), "phi on face 100: {phi}");
}

/// Run to steady state: OpenFOAM v1912 stops after 114 iterations on its
/// residual control, and so does the engine, with every one of the 456
/// solves taking the same number of iterations.
#[test]
fn simplefoam_cavity_matches_openfoam() {
    check(
        &STEADY,
        &Reference {
            first_steps: [2, 1, 12, 2, 2, 12, 2, 2],
            totals: [("Ux", 191), ("Uy", 201), ("p", 1220)],
            iterations: 114,
            end: "114",
            cells: [
                (
                    0,
                    [0.00020254601960356428, -0.00020124103614676365],
                    -6.1930198392809097e-07,
                ),
                (
                    210,
                    [-0.20046154247900191, -0.015295994159358981],
                    0.01695287558876158,
                ),
                (
                    399,
                    [0.26124327280784976, -0.13075799605589533],
                    5.1906595977072945,
                ),
            ],
            face_100_flux: -3.2115315425639409e-06,
        },
    );
}

#[test]
fn simplefoam_variant_matches_openfoam() {
    check(
        &VARIANT,
        &Reference {
            first_steps: [14, 6, 15, 6, 16, 6, 15, 6],
            totals: [("Ux", 0), ("Uy", 0), ("p", 1282)],
            iterations: 60,
            end: "60",
            cells: [
                (
                    0,
                    [0.00016511618505008472, -0.00016296839441688282],
                    2.4906584222562991,
                ),
                (
                    137,
                    [-0.18987783423113935, -0.065134585211405832],
                    2.5645281724125399,
                ),
                (
                    255,
                    [0.25945551650052451, -0.13476422007071043],
                    6.6386885801666624,
                ),
            ],
            face_100_flux: 2.6111262702527433e-06,
        },
    );
}

/// What the engine cannot do yet is refused by name, not run differently.
#[test]
fn unsupported_settings_are_refused() {
    let refuse = |file: &str, from: &str, to: &str, says: &str| {
        let dir = tempfile::tempdir().unwrap();
        cavity(dir.path(), &STEADY);
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
        let err = Case::open(dir.path())
            .and_then(|case| simple::run(&case, |_| {}))
            .unwrap_err()
            .to_string();
        assert!(err.contains(says), "{err}");
    };
    refuse(
        "constant/turbulenceProperties",
        "laminar",
        "LES",
        "simulationType 'LES'",
    );
    refuse(
        "constant/transportProperties",
        "Newtonian",
        "CrossPowerLaw",
        "CrossPowerLaw",
    );
    refuse(
        "system/fvSolution",
        "pRefCell 0;",
        "pRefCell 0; consistent yes;",
        "SIMPLEC",
    );
    refuse(
        "system/fvSchemes",
        "default steadyState",
        "default Euler",
        "steadyState",
    );
    refuse(
        "system/fvSchemes",
        "div((nuEff*dev2(T(grad(U))))) Gauss linear",
        "div((nuEff*dev2(T(grad(U))))) Gauss upwind",
        "viscous stress",
    );
    refuse(
        "system/fvSchemes",
        "Gauss linear orthogonal",
        "Gauss linear corrected",
        "corrected",
    );
    refuse(
        "constant/turbulenceProperties",
        "simulationType laminar;",
        "simulationType laminar; laminar { laminarModel Maxwell; }",
        "Maxwell",
    );
    refuse(
        "system/fvSchemes",
        "interpolationSchemes { default linear; }",
        "interpolationSchemes { default linear; interpolate((1|A(U))) midPoint; }",
        "midPoint",
    );
    refuse(
        "system/fvSchemes",
        "interpolationSchemes { default linear; }",
        "interpolationSchemes { default linear (upwind); }",
        "linear (…)",
    );
    refuse(
        "0/U",
        "fixedWalls { type noSlip; }",
        "fixedWalls { type noSlip; }\n#include \"inlet\"\n",
        "#include",
    );
    refuse(
        "0/p",
        "class volScalarField",
        "class surfaceScalarField",
        "not a cell field",
    );
    for (file, says) in [
        ("constant/MRFProperties", "MRFProperties"),
        ("system/fvOptions", "fvOptions"),
        ("constant/fvOptions", "fvOptions"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        cavity(dir.path(), &STEADY);
        let path = dir.path().join(file);
        let zone = "zone1 { cellZone all; active yes; }";
        write(&path, &format!("{HEADER}{zone}"));
        let run = || Case::open(dir.path()).and_then(|case| simple::run(&case, |_| {}));
        let err = run().unwrap_err().to_string();
        assert!(err.contains(says), "{err}");
        // An entry switched off changes nothing, so it runs, however the
        // switch is spelled.
        for off in ["no", "f", "0"] {
            write(&path, &format!("{HEADER}{}", zone.replace("yes", off)));
            assert!(run().is_ok(), "{file} active {off}");
        }
    }
}

/// `residualControl` applies the first entry that matches a field, as
/// OpenFOAM does: a tight `U` ahead of a loose catch-all keeps iterating.
#[test]
fn the_first_matching_residual_control_applies() {
    let iterations = |control: &str| {
        let dir = tempfile::tempdir().unwrap();
        cavity(dir.path(), &STEADY);
        let path = dir.path().join("system/fvSolution");
        let text = std::fs::read_to_string(&path).unwrap();
        let text = text.replace("residualControl", &format!("residualControl {control} old"));
        std::fs::write(&path, text).unwrap();
        let case = Case::open(dir.path()).unwrap();
        simple::run(&case, |_| {}).unwrap().len()
    };
    let loose = iterations("{ \".*\" 1e-1; }");
    let tight_first = iterations("{ U 1e-6; \".*\" 1e-1; }");
    assert!(tight_first > loose, "{tight_first} vs {loose}");
}
