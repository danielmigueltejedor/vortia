//! The engine's `buoyantBoussinesqSimpleFoam` on the differentially heated
//! square cavity of de Vahl Davis, "Natural convection of air in a square
//! cavity: a bench mark numerical solution" (Int. J. Numer. Methods Fluids,
//! 3, 1983), air at `Pr = 0.71`.
//!
//! The cavity is one unit square, the left wall at `T = 1` and the right at
//! `T = 0`, the floor and the ceiling insulated, no slip everywhere. The
//! Rayleigh number sets the viscosity: `Ra = g β ΔT L³ / (ν α)`.

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_fields::{ScalarField, VectorField};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;
use vortia_solvers::{Case, buoyant};

mod common;

use common::{HEADER, write};

const PR: f64 = 0.71;
const G: f64 = 9.81;
const BETA: f64 = 3e-3;

/// The cavity at the Rayleigh number `ra`, `n` cells to a side, run for
/// `iterations` iterations.
fn cavity(dir: &Path, ra: f64, n: usize, iterations: usize) {
    let bounds = Aabb {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 0.1),
    };
    let mut raw = structured_box_raw(bounds, [n, n, 1]).unwrap();
    // xMin, xMax, yMin, yMax, zMin, zMax
    let p = &raw.patches;
    let patch = |name: &str, kind: &str, from: &[RawPatch]| RawPatch {
        name: name.into(),
        kind: kind.into(),
        start_face: from[0].start_face,
        n_faces: from.iter().map(|p| p.n_faces).sum(),
        groups: vec![kind.into()],
        dict: Default::default(),
    };
    raw.patches = vec![
        patch("hot", "wall", &p[0..1]),
        patch("cold", "wall", &p[1..2]),
        patch("floor", "wall", &p[2..3]),
        patch("ceiling", "wall", &p[3..4]),
        patch("frontAndBack", "empty", &p[4..6]),
    ];
    raw.write_case(dir).unwrap();

    let nu = (G * BETA * 1.0 * PR / ra).sqrt();
    let field = |class: &str, name: &str, dims: &str, internal: &str, walls: [&str; 4]| {
        let [hot, cold, floor, ceiling] = walls;
        write(
            &dir.join(format!("0/{name}")),
            &format!(
                "FoamFile {{ version 2.0; format ascii; class {class}; object {name}; }}\n\
                 dimensions {dims}; internalField {internal};\n\
                 boundaryField {{\n\
                   hot {{ {hot} }} cold {{ {cold} }}\n\
                   floor {{ {floor} }} ceiling {{ {ceiling} }}\n\
                   frontAndBack {{ type empty; }}\n\
                 }}\n"
            ),
        );
    };
    field(
        "volVectorField",
        "U",
        "[0 1 -1 0 0 0 0]",
        "uniform (0 0 0)",
        ["type noSlip;"; 4],
    );
    field(
        "volScalarField",
        "p_rgh",
        "[0 2 -2 0 0 0 0]",
        "uniform 0",
        ["type fixedFluxPressure; value uniform 0;"; 4],
    );
    field(
        "volScalarField",
        "p",
        "[0 2 -2 0 0 0 0]",
        "uniform 0",
        ["type calculated; value uniform 0;"; 4],
    );
    field(
        "volScalarField",
        "T",
        "[0 0 0 1 0 0 0]",
        "uniform 0.5",
        [
            "type fixedValue; value uniform 1;",
            "type fixedValue; value uniform 0;",
            "type zeroGradient;",
            "type zeroGradient;",
        ],
    );

    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application buoyantBoussinesqSimpleFoam; startFrom startTime; startTime 0; \
             stopAt endTime; endTime {iterations}; deltaT 1; writeControl timeStep; \
             writeInterval {iterations}; writeFormat ascii; writePrecision 12; timePrecision 6;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default steadyState; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,U) bounded Gauss linearUpwind grad(U); \
             div(phi,T) bounded Gauss linearUpwind grad(T); \
             div((nuEff*dev2(T(grad(U))))) Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear orthogonal; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default orthogonal; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p_rgh {{ solver PCG; preconditioner DIC; tolerance 1e-10; relTol 0.01; }} \
               U {{ solver smoothSolver; smoother symGaussSeidel; tolerance 1e-10; relTol 0.1; }} \
               T {{ solver smoothSolver; smoother symGaussSeidel; tolerance 1e-10; relTol 0.1; }} \
             }} \
             SIMPLE {{ nNonOrthogonalCorrectors 0; pRefCell 0; pRefValue 0; \
               residualControl {{ p_rgh 1e-7; U 1e-7; T 1e-7; }} }} \
             relaxationFactors {{ fields {{ p_rgh 0.7; }} equations {{ U 0.2; T 0.5; }} }}"
        ),
    );
    write(
        &dir.join("constant/transportProperties"),
        &format!(
            "{HEADER}transportModel Newtonian; nu {nu}; beta {BETA}; TRef 0.5; Pr {PR}; Prt 0.85;"
        ),
    );
    write(
        &dir.join("constant/turbulenceProperties"),
        &format!("{HEADER}simulationType laminar;"),
    );
    write(
        &dir.join("constant/g"),
        &format!("{HEADER}dimensions [0 1 -2 0 0 0 0]; value (0 -{G} 0);"),
    );
}

/// What a run of the cavity found.
struct Found {
    /// The mean Nusselt number on the hot wall.
    nusselt: f64,
    /// The largest horizontal velocity on the vertical midline, and the
    /// largest vertical velocity on the horizontal midline, over `α / L`.
    u_max: f64,
    v_max: f64,
    /// `max |T(x, y) + T(1 − x, 1 − y) − 1|`: the flow is centrosymmetric.
    asymmetry: f64,
    /// How far `p` is from `p_rgh + ρ_k gh` in the first cell.
    p_error: f64,
    iterations: usize,
    iteration_limit: usize,
}

fn run(ra: f64, n: usize, iterations: usize) -> Found {
    let dir = tempfile::tempdir().unwrap();
    cavity(dir.path(), ra, n, iterations);
    let case = Case::open(dir.path()).unwrap();
    let steps = buoyant::run(&case, |_| {}).unwrap();
    let last = steps.last().unwrap();
    assert!(last.written);
    let end = dir.path().join(&last.time);
    let cells = n * n;
    let read = |name: &str| {
        ScalarField::read(end.join(name))
            .unwrap()
            .internal
            .expand(cells)
            .unwrap()
    };
    let (t, p_rgh, p) = (read("T"), read("p_rgh"), read("p"));
    let u = VectorField::read(end.join("U"))
        .unwrap()
        .internal
        .expand(cells)
        .unwrap();
    let at = |i: usize, j: usize| i + n * j;
    let h = 1.0 / n as f64;
    // dT/dx at the hot wall from a quadratic through the wall value and the
    // first two cells.
    let nusselt = (0..n)
        .map(|j| {
            let (t1, t2) = (t[at(0, j)], t[at(1, j)]);
            -(-8.0 * 1.0 + 9.0 * t1 - t2) / (3.0 * h)
        })
        .sum::<f64>()
        / n as f64;
    let alpha = (G * BETA * PR / ra).sqrt() / PR;
    let u_max = (0..n).map(|j| u[at(n / 2, j)].x).fold(f64::MIN, f64::max) / alpha;
    let v_max = (0..n).map(|i| u[at(i, n / 2)].y).fold(f64::MIN, f64::max) / alpha;
    let asymmetry = (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .map(|(i, j)| (t[at(i, j)] + t[at(n - 1 - i, n - 1 - j)] - 1.0).abs())
        .fold(0.0, f64::max);
    let gh = -G * (0.5 * h);
    let p_error = (p[0] - (p_rgh[0] + (1.0 - BETA * (t[0] - 0.5)) * gh)).abs();
    Found {
        nusselt,
        u_max,
        v_max,
        asymmetry,
        p_error,
        iterations: steps.len(),
        iteration_limit: iterations,
    }
}

/// Within `percent` per cent of the benchmark's `reference`.
fn near(what: &str, found: f64, reference: f64, percent: f64) {
    let off = 100.0 * (found - reference) / reference;
    assert!(
        off.abs() <= percent,
        "{what}: {found} is {off:.2} % from {reference}"
    );
}

fn check(ra: f64, n: usize, limit: usize, [nu, u, v]: [f64; 3], tolerance: [f64; 3]) {
    let r = run(ra, n, limit);
    assert!(
        r.iterations < r.iteration_limit,
        "Ra {ra} stopped at its iteration limit, not on its residuals"
    );
    near("Nusselt number", r.nusselt, nu, tolerance[0]);
    near("u_max", r.u_max, u, tolerance[1]);
    near("v_max", r.v_max, v, tolerance[2]);
    assert!(
        r.asymmetry < 1e-5,
        "T is not centrosymmetric: {}",
        r.asymmetry
    );
    assert!(r.p_error < 1e-9, "p is not p_rgh + rhok gh: {}", r.p_error);
}

/// The benchmark's `Nu`, `u_max` and `v_max` for Ra = 10³.
#[test]
fn the_cavity_at_rayleigh_1e3_matches_de_vahl_davis() {
    check(1e3, 20, 3000, [1.118, 3.649, 3.697], [1.0, 1.0, 1.5]);
}

#[test]
fn the_cavity_at_rayleigh_1e4_matches_de_vahl_davis() {
    check(1e4, 32, 6000, [2.243, 16.178, 19.617], [1.5, 1.0, 1.5]);
}

/// Slow in a debug build: `cargo test --release --test buoyant -- --ignored`.
/// The Nusselt number is off by more on a uniform mesh, where the boundary
/// layers are thin.
#[test]
#[ignore]
fn the_cavity_at_rayleigh_1e5_and_1e6_match_de_vahl_davis() {
    check(1e5, 64, 12000, [4.519, 34.73, 68.59], [1.5, 1.0, 1.0]);
    check(1e6, 80, 30000, [8.800, 64.63, 219.36], [3.5, 2.0, 1.0]);
}

fn written(setup: impl FnOnce(&Path)) -> String {
    let dir = tempfile::tempdir().unwrap();
    cavity(dir.path(), 1e3, 4, 10);
    setup(dir.path());
    let case = Case::open(dir.path()).unwrap();
    buoyant::run(&case, |_| {}).unwrap_err().to_string()
}

#[test]
fn it_refuses_what_it_does_not_have() {
    let message = written(|dir| {
        write(
            &dir.join("constant/turbulenceProperties"),
            &format!(
                "{HEADER}simulationType RAS; RAS {{ RASModel kEpsilon; turbulence on; printCoeffs on; }}"
            ),
        );
    });
    assert!(
        message.contains("kEpsilon") || message.contains("laminar"),
        "{message}"
    );

    let message = written(|dir| std::fs::remove_file(dir.join("constant/g")).unwrap());
    assert!(message.contains("g"), "{message}");

    let message = written(|dir| {
        write(
            &dir.join("constant/g"),
            &format!("{HEADER}value (0 nope 0);"),
        );
    });
    assert!(message.contains("three numbers"), "{message}");

    let message = written(|dir| {
        let path = dir.join("constant/transportProperties");
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("Pr 0.71", "Pr 0");
        write(&path, &text);
    });
    assert!(message.contains("Pr"), "{message}");
}
