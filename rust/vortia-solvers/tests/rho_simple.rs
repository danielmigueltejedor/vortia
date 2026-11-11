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
    end: &'static str,
    energy: &'static str,
    div: &'static str,
    simple: &'static str,
    relax: &'static str,
}

/// Internal energy, SIMPLEC, bounded upwind: what the nozzle uses, minus
/// the higher-order schemes.
const CONSISTENT: Setup = Setup {
    end: "400",
    energy: "sensibleInternalEnergy",
    div: "bounded Gauss upwind",
    simple: "nNonOrthogonalCorrectors 0; consistent yes; \
             residualControl { p 1e-6; U 1e-7; e 1e-7; }",
    relax: "fields { p 0.3; rho 0.5; } equations { U 0.7; e 0.7; }",
};

/// Enthalpy, plain SIMPLE, linear convection and no density relaxation.
const ENTHALPY: Setup = Setup {
    end: "40",
    energy: "sensibleEnthalpy",
    div: "Gauss linear",
    simple: "nNonOrthogonalCorrectors 0;",
    relax: "fields { p 0.3; } equations { U 0.7; h 0.7; }",
};

/// A 2D channel 0.2 m long and 0.02 m high: air enters at 300 K and
/// 2 m/s, the lower wall is held at 350 K, the upper wall is adiabatic
/// and the outlet is at 1 bar.
fn channel(dir: &Path, s: &Setup) {
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

    write(
        &dir.join("0/U"),
        "FoamFile { version 2.0; format ascii; class volVectorField; object U; }\n\
         dimensions [0 1 -1 0 0 0 0]; internalField uniform (1 0 0);\n\
         boundaryField {\n\
           inlet { type fixedValue; value uniform (2 0 0); }\n\
           outlet { type zeroGradient; }\n\
           hot { type noSlip; }\n\
           top { type noSlip; }\n\
           frontAndBack { type empty; }\n\
         }\n",
    );
    write(
        &dir.join("0/p"),
        "FoamFile { version 2.0; format ascii; class volScalarField; object p; }\n\
         dimensions [1 -1 -2 0 0 0 0]; internalField uniform 100000;\n\
         boundaryField {\n\
           inlet { type zeroGradient; }\n\
           outlet { type fixedValue; value uniform 100000; }\n\
           hot { type zeroGradient; }\n\
           top { type zeroGradient; }\n\
           frontAndBack { type empty; }\n\
         }\n",
    );
    write(
        &dir.join("0/T"),
        "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
         dimensions [0 0 0 1 0 0 0]; internalField uniform 300;\n\
         boundaryField {\n\
           inlet { type fixedValue; value uniform 300; }\n\
           outlet { type zeroGradient; }\n\
           hot { type fixedValue; value uniform 350; }\n\
           top { type zeroGradient; }\n\
           frontAndBack { type empty; }\n\
         }\n",
    );
    let Setup {
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
/// (`VORTIA_ENTHALPY=1` for the other setup).
#[test]
#[ignore]
fn write_channel_for_openfoam() {
    let dir = std::env::var("VORTIA_CHANNEL").unwrap();
    let setup = if std::env::var("VORTIA_ENTHALPY").is_ok() {
        &ENTHALPY
    } else {
        &CONSISTENT
    };
    channel(Path::new(&dir), setup);
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
