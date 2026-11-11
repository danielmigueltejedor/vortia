//! The engine's diffusion solver against OpenFOAM's `laplacianFoam` on the
//! same case: the nozzle wedge, heated at the inlet, with a heat flux through
//! the walls, `Gauss linear corrected` and non-orthogonal correctors.
//!
//! The reference numbers were produced by OpenFOAM v1912 on this exact case
//! (`writePrecision 17`). The engine matches every solve's iteration count and
//! the cell values to round-off.

use std::path::{Path, PathBuf};

use vortia_fields::ScalarField;
use vortia_solvers::{Case, laplacian};

const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn nozzle_case(ddt: &str, end: &str, dt: &str, correctors: u32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mesh_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/nozzle-practice-2026/constant/polyMesh");
    let mesh_dst = dir.path().join("constant/polyMesh");
    std::fs::create_dir_all(&mesh_dst).unwrap();
    for f in ["points", "faces", "owner", "neighbour", "boundary"] {
        std::fs::copy(mesh_src.join(f), mesh_dst.join(f)).unwrap();
    }
    let d = dir.path();
    write(
        &d.join("system/controlDict"),
        &format!(
            "{HEADER}application laplacianFoam; startFrom startTime; startTime 0; \
             endTime {end}; deltaT {dt}; writeControl timeStep; writeInterval 1;"
        ),
    );
    write(
        &d.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default {ddt}; }} gradSchemes {{ default Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear corrected; }}"
        ),
    );
    write(
        &d.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ T {{ solver PCG; preconditioner DIC; tolerance 1e-10; relTol 0; }} }} \
             SIMPLE {{ nNonOrthogonalCorrectors {correctors}; }}"
        ),
    );
    write(
        &d.join("constant/transportProperties"),
        &format!("{HEADER}DT DT [0 2 -1 0 0 0 0] 4e-05;"),
    );
    write(
        &d.join("0/T"),
        "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
         dimensions [0 0 0 1 0 0 0]; internalField uniform 300;\n\
         boundaryField {\n\
           axis { type empty; }\n\
           inlet { type fixedValue; value uniform 400; }\n\
           walls { type fixedGradient; gradient uniform 1000; }\n\
           atmosphere { type fixedValue; value uniform 300; }\n\
           front { type wedge; } back { type wedge; }\n\
         }\n",
    );
    dir
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-11 * b.abs()
}

fn check(case: &Path, time: &str, expected: [f64; 3], mean: f64) {
    let t = ScalarField::read(case.join(time).join("T")).unwrap();
    let v = t.internal.expand(7500).unwrap();
    for (got, want) in [v[0], v[3749], v[7499]].into_iter().zip(expected) {
        assert!(close(got, want), "{time}: {got} vs {want}");
    }
    let m = v.iter().sum::<f64>() / v.len() as f64;
    assert!(close(m, mean), "{time}: mean {m} vs {mean}");
}

#[test]
fn steady_diffusion_matches_openfoam() {
    let dir = nozzle_case("steadyState", "3", "1", 2);
    let case = Case::open(dir.path()).unwrap();
    let steps = laplacian::run(&case, |_| {}).unwrap();
    let iterations: Vec<usize> = steps
        .iter()
        .flat_map(|s| s.solves.iter().map(|p| p.iterations))
        .collect();
    // laplacianFoam's log: "No Iterations" of each solve.
    assert_eq!(iterations, [125, 104, 94, 81, 72, 58, 39, 9, 1]);
    check(
        dir.path(),
        "3",
        [384.88693152685806, 300.00707999530204, 300.00012595029307],
        303.91502897736154,
    );
}

#[test]
fn transient_diffusion_matches_openfoam() {
    let dir = nozzle_case("Euler", "0.05", "0.01", 1);
    let case = Case::open(dir.path()).unwrap();
    let steps = laplacian::run(&case, |_| {}).unwrap();
    let names: Vec<&str> = steps.iter().map(|s| s.time.as_str()).collect();
    assert_eq!(names, ["0.01", "0.02", "0.03", "0.04", "0.05"]);
    let iterations: Vec<usize> = steps
        .iter()
        .flat_map(|s| s.solves.iter().map(|p| p.iterations))
        .collect();
    assert_eq!(iterations, [116, 96, 91, 85, 87, 78, 82, 73, 80, 72]);
    check(
        dir.path(),
        "0.05",
        [384.8662394800516, 300.00615528678765, 300.00010915658476],
        303.8524253925262,
    );
}

#[test]
fn an_unsupported_setup_is_an_error_not_a_crash() {
    let dir = nozzle_case("CrankNicolson 0.9", "1", "1", 0);
    assert!(laplacian::run(&Case::open(dir.path()).unwrap(), |_| {}).is_err());
    let dir = nozzle_case("Euler", "1", "-1", 0);
    assert!(laplacian::run(&Case::open(dir.path()).unwrap(), |_| {}).is_err());
    let dir = nozzle_case("Euler", "1e30", "1e-30", 0);
    assert!(laplacian::run(&Case::open(dir.path()).unwrap(), |_| {}).is_err());
}
