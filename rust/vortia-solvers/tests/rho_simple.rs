//! The engine's compressible SIMPLE solver against OpenFOAM's
//! `rhoSimpleFoam` (laminar, perfect gas) on a heated channel.
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
use vortia_solvers::{Case, rho_simple};

mod common;

use common::{HEADER, write};

/// How a run of the channel is set up.
struct Setup {
    /// Flow made by pressure at the inlet instead of a velocity.
    driven: bool,
    end: &'static str,
    energy: &'static str,
    div: &'static str,
    simple: &'static str,
    relax: &'static str,
}

/// Internal energy, SIMPLEC, bounded upwind: what the nozzle uses, minus
/// the higher-order schemes.
const CONSISTENT: Setup = Setup {
    driven: false,
    end: "400",
    energy: "sensibleInternalEnergy",
    div: "bounded Gauss upwind",
    simple: "nNonOrthogonalCorrectors 0; consistent yes; \
             residualControl { p 1e-6; U 1e-7; e 1e-7; }",
    relax: "fields { p 0.3; rho 0.5; } equations { U 0.7; e 0.7; }",
};

/// The same, driven by the pressure, and cut after 100 iterations (the
/// residual control would stop the two programs a step apart).
const DRIVEN: Setup = Setup {
    driven: true,
    end: "100",
    energy: "sensibleInternalEnergy",
    div: "bounded Gauss upwind",
    simple: "nNonOrthogonalCorrectors 0; consistent yes;",
    relax: "fields { p 0.3; rho 0.5; } equations { U 0.7; e 0.7; }",
};

/// Enthalpy, plain SIMPLE, linear convection and no density relaxation.
const ENTHALPY: Setup = Setup {
    driven: false,
    end: "40",
    energy: "sensibleEnthalpy",
    div: "Gauss linear",
    simple: "nNonOrthogonalCorrectors 0;",
    relax: "fields { p 0.3; } equations { U 0.7; h 0.7; }",
};

/// A 2D channel 0.2 m long and 0.02 m high: air enters at 300 K and
/// 2 m/s, the lower wall is held at 350 K, the upper wall is adiabatic
/// and the outlet is at 1 bar.
///
/// With a driven setup the flow is made by pressure instead: the inlet holds a
/// total pressure of 1.0002 bar and a total temperature of 300 K, the outlet
/// lets whatever reverses carry 300 K back in.
fn channel(dir: &Path, s: &Setup) {
    let driven = s.driven;
    let bounds = Aabb {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(0.2, 0.02, 0.002),
    };
    let mut raw = structured_box_raw(bounds, [30, 10, 1]).unwrap();
    // xMin, xMax, yMin, yMax, zMin, zMax.
    let rename = |p: &mut RawPatch, name: &str, kind: &str| {
        p.name = name.into();
        p.kind = kind.into();
        p.groups = vec![kind.into()];
    };
    let p = &mut raw.patches;
    rename(&mut p[0], "inlet", "patch");
    rename(&mut p[1], "outlet", "patch");
    rename(&mut p[2], "hot", "wall");
    rename(&mut p[3], "top", "wall");
    let empty = RawPatch {
        name: "frontAndBack".into(),
        kind: "empty".into(),
        start_face: p[4].start_face,
        n_faces: p[4].n_faces + p[5].n_faces,
        groups: vec!["empty".into()],
        dict: Default::default(),
    };
    p.truncate(4);
    p.push(empty);
    raw.write_case(dir).unwrap();

    let (u_boundary, p_boundary, t_boundary) = if driven {
        (
            "inlet { type pressureInletVelocity; value uniform (4 0 0); } \
             outlet { type pressureInletOutletVelocity; value uniform (4 0 0); }",
            "inlet { type totalPressure; p0 uniform 100020; value uniform 100020; } \
             outlet { type fixedValue; value uniform 100000; }",
            "inlet { type totalTemperature; T0 uniform 300; gamma 1.4; value uniform 300; } \
             outlet { type inletOutlet; inletValue uniform 300; value uniform 300; }",
        )
    } else {
        (
            "inlet { type fixedValue; value uniform (2 0 0); } outlet { type zeroGradient; }",
            "inlet { type zeroGradient; } outlet { type fixedValue; value uniform 100000; }",
            "inlet { type fixedValue; value uniform 300; } outlet { type zeroGradient; }",
        )
    };
    let u0 = if driven { 4 } else { 1 };
    write(
        &dir.join("0/U"),
        &format!(
            "FoamFile {{ version 2.0; format ascii; class volVectorField; object U; }}\n\
             dimensions [0 1 -1 0 0 0 0]; internalField uniform ({u0} 0 0);\n\
             boundaryField {{\n\
               {u_boundary}\n\
               hot {{ type noSlip; }}\n\
               top {{ type noSlip; }}\n\
               frontAndBack {{ type empty; }}\n\
             }}\n"
        ),
    );
    write(
        &dir.join("0/p"),
        &format!(
            "FoamFile {{ version 2.0; format ascii; class volScalarField; object p; }}\n\
             dimensions [1 -1 -2 0 0 0 0]; internalField uniform 100000;\n\
             boundaryField {{\n\
               {p_boundary}\n\
               hot {{ type zeroGradient; }}\n\
               top {{ type zeroGradient; }}\n\
               frontAndBack {{ type empty; }}\n\
             }}\n"
        ),
    );
    write(
        &dir.join("0/T"),
        &format!(
            "FoamFile {{ version 2.0; format ascii; class volScalarField; object T; }}\n\
             dimensions [0 0 0 1 0 0 0]; internalField uniform 300;\n\
             boundaryField {{\n\
               {t_boundary}\n\
               hot {{ type fixedValue; value uniform 350; }}\n\
               top {{ type zeroGradient; }}\n\
               frontAndBack {{ type empty; }}\n\
             }}\n"
        ),
    );
    let Setup {
        driven: _,
        end,
        energy,
        div,
        simple,
        relax,
    } = *s;
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application rhoSimpleFoam; startFrom startTime; startTime 0; \
             stopAt endTime; endTime {end}; deltaT 1; writeControl timeStep; \
             writeInterval 20; writeFormat ascii; writePrecision 17; timePrecision 6;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default steadyState; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,U) {div}; div(phi,e) {div}; \
             div(phi,h) {div}; div(phi,Ekp) {div}; div(phi,K) {div}; \
             div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear orthogonal; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default orthogonal; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p {{ solver PCG; preconditioner DIC; tolerance 1e-8; relTol 0.01; }} \
               \"(U|e|h)\" {{ solver smoothSolver; smoother symGaussSeidel; \
                 tolerance 1e-10; relTol 0.1; }} \
             }} \
             SIMPLE {{ {simple} }} relaxationFactors {{ {relax} }}"
        ),
    );
    write(
        &dir.join("constant/thermophysicalProperties"),
        &format!(
            "{HEADER}thermoType {{ type hePsiThermo; mixture pureMixture; transport const; \
             thermo hConst; equationOfState perfectGas; specie specie; energy {energy}; }} \
             mixture {{ specie {{ molWeight 28.9; }} \
             thermodynamics {{ Cp 1005; Hf 0; }} transport {{ mu 1.82e-05; Pr 0.71; }} }}"
        ),
    );
    write(
        &dir.join("constant/turbulenceProperties"),
        &format!("{HEADER}simulationType laminar;"),
    );
}

/// Writes the case for OpenFOAM:
/// `VORTIA_CHANNEL=dir cargo test --test rho_simple write_channel_for_openfoam -- --ignored`
/// (`VORTIA_ENTHALPY=1` for the other setup, `VORTIA_DRIVEN=1` for the
/// pressure-driven one).
#[test]
#[ignore]
fn write_channel_for_openfoam() {
    let dir = std::env::var("VORTIA_CHANNEL").unwrap();
    let setup = if std::env::var("VORTIA_DRIVEN").is_ok() {
        &DRIVEN
    } else if std::env::var("VORTIA_ENTHALPY").is_ok() {
        &ENTHALPY
    } else {
        &CONSISTENT
    };
    channel(Path::new(&dir), setup);
}

/// The nozzle's wedge mesh (7500 cells, as the example case has it) run
/// laminar: the inlet driven by a total pressure a little above the outlet's
/// 1.01325 bar, air moving at 3 m/s to begin with. `uncorrected` stands in for
/// the non-orthogonal correction the engine does not have yet for vectors.
fn nozzle(dir: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/nozzle-practice-2026");
    let mesh = dir.join("constant/polyMesh");
    std::fs::create_dir_all(&mesh).unwrap();
    for f in ["points", "faces", "owner", "neighbour", "boundary"] {
        std::fs::copy(source.join("constant/polyMesh").join(f), mesh.join(f)).unwrap();
    }
    let field = |name: &str, class: &str, dims: &str, internal: &str, patches: &str| {
        write(
            &dir.join("0").join(name),
            &format!(
                "FoamFile {{ version 2.0; format ascii; class {class}; object {name}; }}\n\
                 dimensions {dims}; internalField uniform {internal};\n\
                 boundaryField {{ axis {{ type empty; }} walls {{ {patches} }} \
                 front {{ type wedge; }} back {{ type wedge; }} {} }}\n",
                if name == "U" {
                    "inlet { type pressureInletVelocity; value uniform (0 0 0); } \
                     atmosphere { type pressureInletOutletVelocity; value uniform (0 0 0); }"
                } else if name == "p" {
                    "inlet { type totalPressure; p0 uniform 101525; gamma 1.4; \
                     value uniform 101525; } \
                     atmosphere { type fixedValue; value uniform 101325; }"
                } else {
                    "inlet { type totalTemperature; T0 uniform 300; gamma 1.4; \
                     value uniform 300; } \
                     atmosphere { type inletOutlet; inletValue uniform 300; value uniform 300; }"
                }
            ),
        );
    };
    field(
        "U",
        "volVectorField",
        "[0 1 -1 0 0 0 0]",
        "(3 0 0)",
        "type noSlip;",
    );
    field(
        "p",
        "volScalarField",
        "[1 -1 -2 0 0 0 0]",
        "101325",
        "type zeroGradient;",
    );
    field(
        "T",
        "volScalarField",
        "[0 0 0 1 0 0 0]",
        "300",
        "type zeroGradient;",
    );
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application rhoSimpleFoam; startFrom startTime; startTime 0; \
             stopAt endTime; endTime 60; deltaT 1; writeControl timeStep; \
             writeInterval 60; writeFormat ascii; writePrecision 17; timePrecision 6;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}ddtSchemes {{ default steadyState; }} gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(phi,U) bounded Gauss upwind; \
             div(phi,e) bounded Gauss upwind; div(phi,Ekp) bounded Gauss upwind; \
             div(phi,K) bounded Gauss upwind; \
             div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear uncorrected; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default uncorrected; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p {{ solver PCG; preconditioner DIC; tolerance 1e-7; relTol 0.01; }} \
               \"(U|e)\" {{ solver smoothSolver; smoother symGaussSeidel; \
                 tolerance 1e-8; relTol 0; }} \
             }} \
             SIMPLE {{ nNonOrthogonalCorrectors 0; consistent yes; }} \
             relaxationFactors {{ fields {{ p 0.3; rho 0.5; }} equations {{ U 0.7; e 0.7; }} }}"
        ),
    );
    write(
        &dir.join("constant/thermophysicalProperties"),
        &format!(
            "{HEADER}thermoType {{ type hePsiThermo; mixture pureMixture; transport const; \
             thermo hConst; equationOfState perfectGas; specie specie; \
             energy sensibleInternalEnergy; }} \
             mixture {{ specie {{ molWeight 28.9; }} \
             thermodynamics {{ Cp 1005; Hf 0; }} transport {{ mu 1.82e-05; Pr 0.71; }} }}"
        ),
    );
    write(
        &dir.join("constant/turbulenceProperties"),
        &format!("{HEADER}simulationType laminar;"),
    );
}

/// Writes the nozzle case for OpenFOAM:
/// `VORTIA_NOZZLE=dir cargo test --test rho_simple write_nozzle_for_openfoam -- --ignored`.
#[test]
#[ignore]
fn write_nozzle_for_openfoam() {
    nozzle(Path::new(&std::env::var("VORTIA_NOZZLE").unwrap()));
}

struct Reference {
    first_steps: [usize; 8],
    /// Iterations per field over the first `checked` iterations: the rest
    /// are solves at the rounding floor of a pressure of 1 bar, whose counts
    /// OpenFOAM's own runs do not repeat.
    totals: [(&'static str, usize); 4],
    checked: usize,
    iterations: usize,
    end: &'static str,
    /// (cell, U, T, p, ρ) at the end.
    cells: [(usize, [f64; 2], f64, f64, f64); 3],
    face_100_flux: f64,
}

fn check(setup: &Setup, r: &Reference) {
    let dir = tempfile::tempdir().unwrap();
    channel(dir.path(), setup);
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_simple::run(&case, |_| {}).unwrap();
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
            .take(r.checked)
            .flat_map(|s| &s.solves)
            .filter(|(f, _)| f == field)
            .map(|(_, p)| p.iterations)
            .sum();
        assert_eq!(sum, total, "iterations solving {field}");
    }

    let end = dir.path().join(r.end);
    let n = 300;
    let read = |name: &str| {
        ScalarField::read(end.join(name))
            .unwrap()
            .internal
            .expand(n)
            .unwrap()
    };
    let u = VectorField::read(end.join("U"))
        .unwrap()
        .internal
        .expand(n)
        .unwrap();
    let (t, p, rho) = (read("T"), read("p"), read("rho"));
    for (c, [ux, uy], tc, pc, rc) in r.cells {
        assert!(
            (u[c].x - ux).abs() < 1e-8 && (u[c].y - uy).abs() < 1e-8,
            "U in cell {c}: {:?}",
            u[c]
        );
        assert_eq!(u[c].z, 0.0);
        assert!((t[c] - tc).abs() < 1e-7, "T in cell {c}: {}", t[c]);
        // A part in 10¹³ of a bar: the rounding floor of the pressure.
        assert!((p[c] - pc).abs() < 5e-8, "p in cell {c}: {}", p[c]);
        assert!((rho[c] - rc).abs() < 1e-9, "rho in cell {c}: {}", rho[c]);
    }
    let phi = ScalarField::read(end.join("phi"))
        .unwrap()
        .internal
        .expand(560)
        .unwrap()[100];
    assert!(
        (phi - r.face_100_flux).abs() < 1e-12,
        "phi on face 100: {phi}"
    );
}

/// Run to convergence: OpenFOAM v1912 stops after 166 iterations on its
/// residual control, and so does the engine.
#[test]
fn rhosimplefoam_simplec_matches_openfoam() {
    check(
        &CONSISTENT,
        &Reference {
            first_steps: [1, 1, 1, 25, 1, 1, 1, 24],
            totals: [("Ux", 128), ("Uy", 128), ("e", 128), ("p", 2935)],
            checked: 128,
            iterations: 166,
            end: "166",
            cells: [
                (
                    0,
                    [1.9618851859278554, 0.010737778560334196],
                    301.74576310735864,
                    100000.85804306426,
                    1.1519292717134737,
                ),
                (
                    155,
                    [2.0665251278331973, 0.00015416745070632228],
                    299.9998817926061,
                    100000.68019118757,
                    1.1586309863599902,
                ),
                (
                    299,
                    [1.2046384919688282, -0.001981927936154163],
                    300.000796842245,
                    100000.01018982442,
                    1.1586196895861844,
                ),
            ],
            face_100_flux: 1.3907176673622536e-07,
        },
    );
}

/// The flow made by a total pressure and a total temperature at the inlet
/// (`pressureInletVelocity` for the velocity), and an outlet that lets the
/// flow reverse (`pressureInletOutletVelocity`, `inletOutlet`).
#[test]
fn rhosimplefoam_pressure_driven_matches_openfoam() {
    check(
        &DRIVEN,
        &Reference {
            first_steps: [1, 1, 1, 23, 1, 1, 1, 22],
            totals: [("Ux", 100), ("Uy", 100), ("e", 100), ("p", 2055)],
            checked: 100,
            iterations: 100,
            end: "100",
            cells: [
                (
                    0,
                    [5.4707198923598419, 0.0090019829344584919],
                    300.644097078024,
                    100002.39520891129,
                    1.1561681089441458,
                ),
                (
                    155,
                    [5.5871510362622105, 0.00030337348467313031],
                    299.98449902510703,
                    100001.95353574178,
                    1.1587051537674027,
                ),
                (
                    299,
                    [4.4748649792485642, -0.0051801253383147025],
                    299.98936578277892,
                    100000.03048622131,
                    1.1586640716089533,
                ),
            ],
            face_100_flux: 1.8701887304490299e-07,
        },
    );
}

/// The wedge's two sides turn the velocity onto themselves, in the matrices
/// (`transform` conditions' diagonal and explicit parts), in `HbyA`, and in
/// the gradient behind the viscous stress. The first sixty iterations take
/// OpenFOAM's iteration count in every solve of `Ux`, `Uy`, `e` and `p`. `Uz`
/// is the wedge's own noise (some 1e-12 m/s, where the mesh is not exactly
/// symmetric), so its counts may differ by one in a few solves.
#[test]
fn rhosimplefoam_on_a_wedge_matches_openfoam() {
    let dir = tempfile::tempdir().unwrap();
    nozzle(dir.path());
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_simple::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), 60);

    let counts: Vec<usize> = steps
        .iter()
        .flat_map(|s| s.solves.iter().map(|(_, p)| p.iterations))
        .collect();
    assert_eq!(counts[..8], [15, 16, 13, 18, 47, 14, 14, 12]);
    let total = |field: &str| -> usize {
        steps
            .iter()
            .flat_map(|s| &s.solves)
            .filter(|(f, _)| f == field)
            .map(|(_, p)| p.iterations)
            .sum()
    };
    for (field, want) in [("Ux", 686), ("Uy", 733), ("e", 743), ("p", 2110)] {
        assert_eq!(total(field), want, "iterations solving {field}");
    }
    assert!(total("Uz").abs_diff(721) <= 3, "Uz: {}", total("Uz"));

    let end = dir.path().join("60");
    let u = VectorField::read(end.join("U"))
        .unwrap()
        .internal
        .expand(7500)
        .unwrap();
    let read = |name: &str| {
        ScalarField::read(end.join(name))
            .unwrap()
            .internal
            .expand(7500)
            .unwrap()
    };
    let (t, p, rho) = (read("T"), read("p"), read("rho"));
    for (c, [ux, uy], tc, pc, rc) in [
        (
            0,
            [18.474916737963866, -5.2721776281703455e-05],
            299.8304856451849,
            101324.0041431037,
            1.1746257292558537,
        ),
        (
            3749,
            [3.007140039578203, -0.009169240040166778],
            299.9997040133394,
            101325.01654498282,
            1.173975646321868,
        ),
        (
            7499,
            [0.009563770478072126, -0.39162831730579856],
            299.99999653346777,
            101325.00848236658,
            1.1739745118901812,
        ),
    ] {
        assert!(
            (u[c].x - ux).abs() < 1e-8 && (u[c].y - uy).abs() < 1e-8,
            "U in cell {c}: {:?}",
            u[c]
        );
        assert!(u[c].z.abs() < 1e-10, "Uz in cell {c}: {}", u[c].z);
        assert!((t[c] - tc).abs() < 1e-9, "T in cell {c}: {}", t[c]);
        assert!((p[c] - pc).abs() < 1e-8, "p in cell {c}: {}", p[c]);
        assert!((rho[c] - rc).abs() < 1e-12, "rho in cell {c}: {}", rho[c]);
    }
}

/// A `totalPressure` or `totalTemperature` without a `value` starts from
/// its `p0` or `T0`, as OpenFOAM's does: here the same numbers, so the
/// same solution.
#[test]
fn total_conditions_without_a_value_start_from_their_total() {
    let end = |strip: bool| {
        let dir = tempfile::tempdir().unwrap();
        channel(dir.path(), &DRIVEN);
        if strip {
            for (file, from, to) in [
                (
                    "0/p",
                    "p0 uniform 100020; value uniform 100020;",
                    "p0 uniform 100020;",
                ),
                ("0/T", "gamma 1.4; value uniform 300;", "gamma 1.4;"),
            ] {
                let path = dir.path().join(file);
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(text.contains(from), "{file}");
                std::fs::write(&path, text.replace(from, to)).unwrap();
            }
        }
        let case = Case::open(dir.path()).unwrap();
        rho_simple::run(&case, |_| {}).unwrap();
        // The first written step shows where the start went, which the
        // converged state forgets.
        ["20", "100"].map(|time| {
            VectorField::read(dir.path().join(time).join("U"))
                .unwrap()
                .internal
                .expand(300)
                .unwrap()
        })
    };
    assert_eq!(end(true), end(false));
}

/// What the driven conditions do not take yet is refused, naming it.
#[test]
fn rhosimplefoam_refuses_what_it_cannot_drive() {
    let refused = |file: &str, from: &str, to: &str| {
        let dir = tempfile::tempdir().unwrap();
        channel(dir.path(), &DRIVEN);
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
        let case = Case::open(dir.path()).unwrap();
        rho_simple::run(&case, |_| {}).unwrap_err().to_string()
    };
    let e = refused(
        "0/p",
        "p0 uniform 100020;",
        "p0 uniform 100020; psi thermo:psi; gamma 1.4;",
    );
    assert!(e.contains("psi"), "{e}");
    let e = refused("0/p", "p0 uniform 100020;", "");
    assert!(e.contains("p0"), "{e}");
    let e = refused("0/T", "gamma 1.4;", "");
    assert!(e.contains("gamma"), "{e}");
    let e = refused("0/T", "inletValue uniform 300;", "");
    assert!(e.contains("inletValue"), "{e}");
    let e = refused(
        "0/U",
        "pressureInletOutletVelocity;",
        "pressureInletOutletVelocity; tangentialVelocity uniform (0 1 0);",
    );
    assert!(e.contains("tangentialVelocity"), "{e}");
}

#[test]
fn rhosimplefoam_enthalpy_matches_openfoam() {
    check(
        &ENTHALPY,
        &Reference {
            first_steps: [2, 2, 2, 25, 2, 2, 1, 25],
            totals: [("Ux", 80), ("Uy", 80), ("h", 79), ("p", 966)],
            checked: 40,
            iterations: 40,
            end: "40",
            cells: [
                (
                    0,
                    [1.9657395968228768, 0.0054020400832627755],
                    300.9584997228688,
                    100000.85169043431,
                    1.154942476713741,
                ),
                (
                    155,
                    [2.0919550476227613, 6.876840567611838e-05],
                    299.98676264904975,
                    100000.67563394201,
                    1.1586816032796188,
                ),
                (
                    299,
                    [1.2464727539462797, -0.0033298442235419703],
                    316.98139632875774,
                    100000.01368647565,
                    1.0965528144319938,
                ),
            ],
            face_100_flux: 7.953995316331021e-08,
        },
    );
}

/// What the engine cannot do yet is refused by name, not run differently.
#[test]
fn unsupported_settings_are_refused() {
    let refuse = |file: &str, from: &str, to: &str, says: &str| {
        let dir = tempfile::tempdir().unwrap();
        channel(dir.path(), &CONSISTENT);
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
        let err = Case::open(dir.path())
            .and_then(|case| rho_simple::run(&case, |_| {}))
            .unwrap_err()
            .to_string();
        assert!(err.contains(says), "{err}");
    };
    refuse(
        "constant/thermophysicalProperties",
        "hePsiThermo",
        "heRhoThermo",
        "hePsiThermo",
    );
    refuse(
        "constant/thermophysicalProperties",
        "hConst",
        "janaf",
        "janaf",
    );
    refuse(
        "constant/thermophysicalProperties",
        "sensibleInternalEnergy",
        "absoluteEnthalpy",
        "absoluteEnthalpy",
    );
    refuse(
        "constant/turbulenceProperties",
        "simulationType laminar;",
        "simulationType RAS; RAS { RASModel kEpsilon; }",
        "RAS",
    );
    refuse(
        "system/fvSolution",
        "consistent yes;",
        "consistent yes; transonic yes;",
        "transonic",
    );
    refuse(
        "system/fvSolution",
        "consistent yes;",
        "consistent yes; pMinFactor 0.5;",
        "pMinFactor",
    );
    refuse(
        "0/T",
        "top { type zeroGradient; }",
        "top { type fixedGradient; gradient uniform 1; }",
        "fixedGradient",
    );
    refuse(
        "0/p",
        "outlet { type fixedValue; value uniform 100000; }",
        "outlet { type zeroGradient; }",
        "fixed pressure",
    );
    refuse(
        "system/fvSchemes",
        "default steadyState",
        "default Euler",
        "steadyState",
    );
    refuse(
        "system/fvSchemes",
        "snGradSchemes { default orthogonal; }",
        "snGradSchemes { default corrected; }",
        "snGrad(p)",
    );
    refuse(
        "0/T",
        "dimensions [0 0 0 1 0 0 0]",
        "dimensions [0 0 0 0 0 0 0]",
        "T",
    );
}

fn edit(dir: &Path, file: &str, from: &str, to: &str) {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{from}");
    std::fs::write(&path, text.replace(from, to)).unwrap();
}

/// Plain SIMPLE asks for `laplacian(rhorAUf,p)`, SIMPLEC for
/// `laplacian(rhorAtU,p)`: with `default none` each runs on its own key.
#[test]
fn each_algorithm_reads_its_own_pressure_laplacian() {
    for (setup, key, energy) in [
        (&ENTHALPY, "laplacian(rhorAUf,p)", "h"),
        (&CONSISTENT, "laplacian(rhorAtU,p)", "e"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        channel(dir.path(), setup);
        edit(
            dir.path(),
            "system/fvSchemes",
            "laplacianSchemes { default Gauss linear orthogonal; }",
            &format!(
                "laplacianSchemes {{ default none; {key} Gauss linear orthogonal; \
                 laplacian((rho*nuEff),U) Gauss linear orthogonal; \
                 laplacian(alphaEff,{energy}) Gauss linear orthogonal; }}"
            ),
        );
        edit(
            dir.path(),
            "system/controlDict",
            &format!("endTime {};", setup.end),
            "endTime 2;",
        );
        let case = Case::open(dir.path()).unwrap();
        let steps = rho_simple::run(&case, |_| {}).unwrap();
        assert_eq!(steps.len(), 2, "{key}");
    }
}

/// A run picks up from the mass flux it wrote.
#[test]
fn a_run_restarts_from_what_it_wrote() {
    let dir = tempfile::tempdir().unwrap();
    channel(dir.path(), &ENTHALPY);
    edit(
        dir.path(),
        "system/controlDict",
        "endTime 40;",
        "endTime 20;",
    );
    let case = Case::open(dir.path()).unwrap();
    rho_simple::run(&case, |_| {}).unwrap();
    assert!(dir.path().join("20/phi").is_file());
    edit(
        dir.path(),
        "system/controlDict",
        "startTime 0;",
        "startTime 20;",
    );
    edit(
        dir.path(),
        "system/controlDict",
        "endTime 20;",
        "endTime 22;",
    );
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_simple::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), 2);
}
