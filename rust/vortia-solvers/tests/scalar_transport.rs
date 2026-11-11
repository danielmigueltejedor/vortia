//! The engine's convection–diffusion solver against OpenFOAM's
//! `scalarTransportFoam` on the nozzle wedge, carried by the flow the nozzle
//! case computed (its `U` and `phi` at time 500).
//!
//! The reference numbers were produced by OpenFOAM v1912 on these exact cases
//! (`writePrecision 17`). The engine matches every solve's iteration count and
//! the cell values to round-off.

use std::path::{Path, PathBuf};

use vortia_fields::ScalarField;
use vortia_solvers::{Case, scalar_transport};

const HEADER: &str = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn nozzle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/nozzle-practice-2026")
}

/// The nozzle with `T` heated at the inlet, carried by the case's flow.
/// `with_phi` copies the flux (as a volume flux); without it the solver
/// builds one from `U`.
fn case(ddt: &str, div: &str, end: &str, dt: &str, with_phi: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let mesh_dst = d.join("constant/polyMesh");
    std::fs::create_dir_all(&mesh_dst).unwrap();
    for f in ["points", "faces", "owner", "neighbour", "boundary"] {
        std::fs::copy(nozzle().join("constant/polyMesh").join(f), mesh_dst.join(f)).unwrap();
    }
    write(
        &d.join("system/controlDict"),
        &format!(
            "{HEADER}application scalarTransportFoam; startFrom startTime; startTime 0; \
             endTime {end}; deltaT {dt}; writeControl timeStep; writeInterval 1;"
        ),
    );
    write(
        &d.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default {ddt}; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,T) {div}; }} \
             laplacianSchemes {{ default Gauss linear corrected; }}"
        ),
    );
    write(
        &d.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ T {{ solver PBiCGStab; preconditioner DILU; tolerance 1e-10; \
             relTol 0; }} }} SIMPLE {{ nNonOrthogonalCorrectors 1; }}"
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
    std::fs::copy(nozzle().join("500/U"), d.join("0/U")).unwrap();
    if with_phi {
        let phi = std::fs::read_to_string(nozzle().join("500/phi")).unwrap();
        let phi = phi.replace("[1 0 -1 0 0 0 0]", "[0 3 -1 0 0 0 0]");
        write(&d.join("0/phi"), &phi);
    }
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

fn iterations(steps: &[vortia_solvers::laplacian::Step]) -> Vec<usize> {
    steps
        .iter()
        .flat_map(|s| s.solves.iter().map(|p| p.iterations))
        .collect()
}

#[test]
fn steady_upwind_transport_matches_openfoam() {
    let dir = case("steadyState", "Gauss upwind", "3", "1", true);
    let steps = scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {}).unwrap();
    assert_eq!(iterations(&steps), [37, 34, 26, 14, 1, 0]);
    check(
        dir.path(),
        "3",
        [399.9999293197146, 300.0062189054042, 300.00002283538635],
        328.486061073332,
    );
}

/// The nozzle's flux is a mass flux read as a volume flux, so it does not
/// conserve: `bounded` is what keeps that from creating or destroying `T`.
#[test]
fn bounded_steady_transport_matches_openfoam() {
    let dir = case("steadyState", "bounded Gauss upwind", "3", "1", true);
    let steps = scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {}).unwrap();
    assert_eq!(iterations(&steps), [32, 34, 26, 13, 1, 0]);
    check(
        dir.path(),
        "3",
        [399.9999999987419, 300.0010536554006, 300.00000000000006],
        328.491639886155,
    );
}

#[test]
fn transient_linear_transport_with_the_flux_built_from_u_matches_openfoam() {
    let dir = case("Euler", "Gauss linear", "5e-06", "1e-06", false);
    let steps = scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {}).unwrap();
    assert_eq!(iterations(&steps), [15, 9, 12, 7, 12, 7, 11, 6, 10, 5]);
    check(
        dir.path(),
        "5e-06",
        [399.8959987519816, 298.947622701559, 300.1010000308401],
        305.0246618348859,
    );
}

#[test]
fn an_unsupported_convection_scheme_is_an_error() {
    let dir = case("Euler", "Gauss vanLeer", "1e-06", "1e-06", true);
    assert!(scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {}).is_err());
}

#[test]
fn a_run_restarts_from_the_last_time_it_wrote() {
    let dir = case("Euler", "Gauss linear", "2e-06", "1e-06", false);
    scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {}).unwrap();
    for f in ["T", "U", "phi"] {
        assert!(dir.path().join("2e-06").join(f).is_file(), "{f}");
    }
    let control = dir.path().join("system/controlDict");
    let text = std::fs::read_to_string(&control).unwrap();
    let text = text
        .replace("startFrom startTime", "startFrom latestTime")
        .replace("endTime 2e-06", "endTime 3e-06");
    std::fs::write(&control, text).unwrap();
    let steps = scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {}).unwrap();
    assert_eq!(steps.len(), 1);
    assert!(dir.path().join("3e-06/T").is_file());
}

#[test]
fn a_mass_flux_is_refused_as_a_volume_flux() {
    let dir = case("steadyState", "Gauss upwind", "1", "1", true);
    let phi = dir.path().join("0/phi");
    let text = std::fs::read_to_string(&phi).unwrap();
    std::fs::write(&phi, text.replace("[0 3 -1 0 0 0 0]", "[1 0 -1 0 0 0 0]")).unwrap();
    let err = scalar_transport::run(&Case::open(dir.path()).unwrap(), |_| {})
        .unwrap_err()
        .to_string();
    assert!(err.contains("phi has dimensions"), "{err}");
}
