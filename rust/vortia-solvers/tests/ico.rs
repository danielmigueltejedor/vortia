//! The engine's PISO solver against OpenFOAM's `icoFoam` on the lid-driven
//! cavity.
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
use vortia_solvers::{Case, ico};

const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// How a run of the cavity is set up.
struct Setup {
    cells: usize,
    end: &'static str,
    dt: &'static str,
    div: &'static str,
    laplacian: &'static str,
    u_solver: &'static str,
    piso: &'static str,
}

/// The tutorial's settings: 20×20 cells to t = 0.5.
const TUTORIAL: Setup = Setup {
    cells: 20,
    end: "0.5",
    dt: "0.005",
    div: "Gauss linear",
    laplacian: "Gauss linear orthogonal",
    u_solver: "solver smoothSolver; smoother symGaussSeidel; tolerance 1e-05; relTol 0;",
    piso: "nCorrectors 2; nNonOrthogonalCorrectors 0; pRefCell 0; pRefValue 0;",
};

/// Everything else the solver can do: upwind, PBiCGStab for U, three
/// correctors with a non-orthogonal one, the reference in another cell.
const VARIANT: Setup = Setup {
    cells: 16,
    end: "0.04",
    dt: "0.004",
    div: "Gauss upwind",
    laplacian: "Gauss linear uncorrected",
    u_solver: "solver PBiCGStab; preconditioner DILU; tolerance 1e-08; relTol 0.01;",
    piso: "nCorrectors 3; nNonOrthogonalCorrectors 1; pRefCell 37; pRefValue 2.5; \
           momentumPredictor yes;",
};

/// The classic cavity: 0.1 m square, lid at 1 m/s, ν = 0.01, Re = 10.
fn cavity(dir: &Path, s: &Setup) {
    let Setup {
        cells,
        end,
        dt,
        div,
        laplacian,
        u_solver,
        piso,
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
            "{HEADER}application icoFoam; startFrom startTime; startTime 0; stopAt endTime; \
             endTime {end}; deltaT {dt}; writeControl timeStep; writeInterval 1; \
             writeFormat ascii; writePrecision 17; timePrecision 6;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default Euler; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,U) {div}; }} \
             laplacianSchemes {{ default {laplacian}; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default orthogonal; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p {{ solver PCG; preconditioner DIC; tolerance 1e-06; relTol 0.05; }} \
               pFinal {{ $p; relTol 0; }} \
               U {{ {u_solver} }} \
             }} \
             PISO {{ {piso} }}"
        ),
    );
    write(
        &dir.join("constant/transportProperties"),
        &format!("{HEADER}nu 0.01;"),
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

/// Writes the case for OpenFOAM: `VORTIA_CAVITY=dir cargo test -- --ignored`.
#[test]
#[ignore]
fn write_cavity_for_openfoam() {
    let dir = std::env::var("VORTIA_CAVITY").unwrap();
    let setup = if std::env::var("VORTIA_VARIANT").is_ok() {
        &VARIANT
    } else {
        &TUTORIAL
    };
    cavity(Path::new(&dir), setup);
}

/// Runs the engine on a case: `VORTIA_CAVITY=dir cargo test -- --ignored`.
#[test]
#[ignore]
fn run_cavity_case() {
    let dir = std::env::var("VORTIA_CAVITY").unwrap();
    let case = Case::open(&dir).unwrap();
    vortia_solvers::run(&case, |l| print!("{l}")).unwrap();
}

struct Reference {
    first_step: [usize; 8],
    totals: [(&'static str, usize); 3],
    end: &'static str,
    /// (cell, U, p) at the end time.
    cells: [(usize, [f64; 2], f64); 3],
    face_100_flux: f64,
}

fn check(setup: &Setup, r: &Reference) {
    let dir = tempfile::tempdir().unwrap();
    cavity(dir.path(), setup);
    let case = Case::open(dir.path()).unwrap();
    let steps = ico::run(&case, |_| {}).unwrap();

    let counts: Vec<usize> = steps
        .iter()
        .flat_map(|s| s.solves.iter().map(|(_, p)| p.iterations))
        .collect();
    assert_eq!(counts[..8], r.first_step);
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
    let u = VectorField::read(end.join("U")).unwrap();
    let p = ScalarField::read(end.join("p")).unwrap();
    let u = u.internal.expand(setup.cells * setup.cells).unwrap();
    let p = p.internal.expand(setup.cells * setup.cells).unwrap();
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

/// OpenFOAM's tutorial settings for 100 steps: every one of the 400 solves
/// takes the same number of iterations, and the fields agree to round-off.
#[test]
fn icofoam_cavity_matches_openfoam() {
    check(
        &TUTORIAL,
        &Reference {
            first_step: [19, 0, 12, 35, 19, 18, 22, 33],
            totals: [("Ux", 283), ("Uy", 319), ("p", 1606)],
            end: "0.5",
            cells: [
                (
                    0,
                    [0.00025340549641044162, -0.00025045582927944347],
                    4.2993117172359416e-06,
                ),
                (
                    210,
                    [-0.20385644943821979, -0.015659478617749988],
                    0.015608083297866063,
                ),
                (
                    399,
                    [0.308819407418294, -0.14946752173936129],
                    4.848535352867942,
                ),
            ],
            face_100_flux: -3.2124393302135079e-06,
        },
    );
}

#[test]
fn icofoam_variant_matches_openfoam() {
    check(
        &VARIANT,
        &Reference {
            first_step: [2, 0, 17, 6, 17, 4, 9, 20],
            totals: [("Ux", 17), ("Uy", 11), ("p", 681)],
            end: "0.04",
            cells: [
                (
                    0,
                    [0.00022259983298526592, -0.0002162994687752069],
                    2.4913015241469885,
                ),
                (
                    137,
                    [-0.18766632216343468, -0.066062378340553812],
                    2.5652249287544207,
                ),
                (
                    255,
                    [0.30437510842292076, -0.15139661677834548],
                    6.3685957943175104,
                ),
            ],
            face_100_flux: 2.7147633476260653e-06,
        },
    );
}

/// What the engine cannot do yet is refused by name, not run differently.
#[test]
fn unsupported_settings_are_refused() {
    let refuse = |file: &str, from: &str, to: &str, says: &str| {
        let dir = tempfile::tempdir().unwrap();
        cavity(dir.path(), &TUTORIAL);
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
        let err = Case::open(dir.path())
            .and_then(|case| ico::run(&case, |_| {}))
            .unwrap_err()
            .to_string();
        assert!(err.contains(says), "{err}");
    };
    refuse(
        "system/fvSchemes",
        "Gauss linear orthogonal",
        "Gauss linear corrected",
        "corrected",
    );
    refuse(
        "system/fvSchemes",
        "default Euler",
        "default backward",
        "backward",
    );
    refuse(
        "system/fvSolution",
        "pRefCell 0;",
        "pRefPoint (0 0 0);",
        "pRefPoint",
    );
    refuse(
        "system/fvSolution",
        "pRefCell 0;",
        "pRefCell 400;",
        "pRefCell 400",
    );
    refuse("0/U", "type noSlip;", "type slip;", "slip");
    refuse("system/fvSolution", "pFinal", "pLast", "pFinal");
    // What a directive would change is unknown until it is evaluated.
    refuse(
        "system/fvSolution",
        "PISO {",
        "#include \"more\"\nPISO {",
        "#include",
    );
    // Units are checked, not assumed.
    refuse(
        "constant/transportProperties",
        "nu 0.01;",
        "nu [0 0 0 0 0 0 0] 0.01;",
        "nu has dimensions",
    );
    refuse(
        "0/p",
        "[0 2 -2 0 0 0 0]",
        "[1 -1 -2 0 0 0 0]",
        "p has dimensions",
    );
    // A flipped face makes a mesh no solution can be trusted on.
    refuse(
        "constant/polyMesh/faces",
        "4(1 22 463 442)",
        "4(442 463 22 1)",
        "wrongly oriented",
    );
}
