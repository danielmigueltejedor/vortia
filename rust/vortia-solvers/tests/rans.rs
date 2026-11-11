//! The engine's SIMPLE solver with the k-ε and k-ω SST models against
//! OpenFOAM's `simpleFoam` on a turbulent channel and a turbulent cavity.
//!
//! The meshes are the engine's own boxes, written to disk so OpenFOAM v1912
//! ran on identical meshes. The reference numbers come from those runs
//! (`writePrecision 17`).

// The reference numbers are copied as OpenFOAM printed them.
#![allow(clippy::excessive_precision)]

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_fields::{ScalarField, VectorField};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;
use vortia_solvers::{Case, simple};

mod common;

use common::{HEADER, write};

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

/// The turbulence model of a run.
#[derive(Clone, Copy, PartialEq)]
enum Rans {
    KEpsilon,
    KOmegaSst,
}

/// How a run is set up.
#[derive(Clone, Copy)]
struct Setup {
    model: Rans,
    /// A channel with an inlet and an outlet, or a cavity with a lid.
    channel: bool,
    end: &'static str,
    div: &'static str,
    simple: &'static str,
    relax: &'static str,
}

/// A 1 m × 0.1 m channel, 40 × 10 cells, air at 10 m/s with 5 % intensity.
const CHANNEL: Setup = Setup {
    model: Rans::KEpsilon,
    channel: true,
    end: "2000",
    div: "bounded Gauss upwind",
    simple: "nNonOrthogonalCorrectors 0; \
             residualControl { p 1e-3; U 1e-4; \"(k|epsilon|omega)\" 1e-4; }",
    relax: "fields { p 0.3; } equations { U 0.7; \".*\" 0.7; }",
};

/// A 0.1 m cavity, 20 × 20 cells, its lid at 1 m/s, ν = 1e-5: the corners
/// where two walls meet, and wall cells below the log layer.
const CAVITY: Setup = Setup {
    model: Rans::KEpsilon,
    channel: false,
    end: "150",
    div: "bounded Gauss linear",
    simple: "nNonOrthogonalCorrectors 0; pRefCell 0; pRefValue 0;",
    relax: "fields { p 0.3; } equations { U 0.7; k 0.6; epsilon 0.6; omega 0.6; }",
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
    let second = match s.model {
        Rans::KEpsilon => "epsilon",
        Rans::KOmegaSst => "omega",
    };
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
             div(phi,{second}) bounded Gauss upwind; \
             div((nuEff*dev2(T(grad(U))))) Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear orthogonal; }} \
             interpolationSchemes {{ default linear; }} snGradSchemes {{ default orthogonal; }} \
             wallDist {{ method meshWave; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \
               p {{ solver PCG; preconditioner DIC; tolerance 1e-07; relTol 0.05; }} \
               \"(U|k|epsilon|omega)\" {{ solver smoothSolver; smoother symGaussSeidel; \
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
            "{HEADER}simulationType RAS; RAS {{ RASModel {}; turbulence on; printCoeffs on; }}",
            match s.model {
                Rans::KEpsilon => "kEpsilon",
                Rans::KOmegaSst => "kOmegaSST",
            }
        ),
    );

    let vector = "volVectorField";
    let scalar = "volScalarField";
    // ω = ε/(Cμ k) of the k-ε runs' ε.
    let (dims, wall_function) = match s.model {
        Rans::KEpsilon => ("[0 2 -3 0 0 0 0]", "epsilonWallFunction"),
        Rans::KOmegaSst => ("[0 0 -1 0 0 0 0]", "omegaWallFunction"),
    };
    let sst = s.model == Rans::KOmegaSst;
    if s.channel {
        let (k, eps) = ("0.375", if sst { "440.15" } else { "14.855" });
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
            second,
            scalar,
            dims,
            eps,
            &kind(second, wall_function, eps),
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
        let (k, eps) = ("0.00375", if sst { "3.6" } else { "0.0012" });
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
            second,
            scalar,
            dims,
            eps,
            &format!(
                "fixedWalls {{ type {wall_function}; value uniform {eps}; }} \
                 movingWall {{ type {wall_function}; value uniform {eps}; }}"
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
/// `VORTIA_CASE=dir cargo test --test rans write_rans_case_for_openfoam -- --ignored`,
/// with `VORTIA_CAVITY=1` for the cavity and `VORTIA_SST=1` for k-ω SST.
#[test]
#[ignore]
fn write_rans_case_for_openfoam() {
    let dir = std::env::var("VORTIA_CASE").unwrap();
    let mut setup = if std::env::var("VORTIA_CAVITY").is_ok() {
        CAVITY
    } else {
        CHANNEL
    };
    if std::env::var("VORTIA_SST").is_ok() {
        setup.model = Rans::KOmegaSst;
    }
    case(Path::new(&dir), &setup);
}

/// What OpenFOAM printed and wrote for one run.
struct Reference {
    iterations: usize,
    /// Solver iterations over the whole run, per field.
    totals: [(&'static str, usize); 5],
    /// The first iteration's initial residuals of ε or ω, and of k.
    first: [(&'static str, f64); 2],
    end: &'static str,
    /// Cell, U (x, y), p, k, ε or ω and ν_t there at the end.
    cells: [(usize, [f64; 2], f64, f64, f64, f64); 3],
}

fn check(setup: &Setup, r: &Reference) {
    let dir = tempfile::tempdir().unwrap();
    case(dir.path(), setup);
    let case = Case::open(dir.path()).unwrap();
    let steps = simple::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), r.iterations);
    assert!(steps.last().unwrap().written);
    for (field, total) in r.totals {
        let sum: usize = steps
            .iter()
            .flat_map(|s| &s.solves)
            .filter(|(f, _)| f == field)
            .map(|(_, p)| p.iterations)
            .sum();
        assert_eq!(sum, total, "iterations solving {field}");
    }
    for (field, residual) in r.first {
        let (_, p) = steps[0].solves.iter().find(|(f, _)| f == field).unwrap();
        assert!(
            (p.initial_residual - residual).abs() <= 1e-14,
            "{field}: {}",
            p.initial_residual
        );
    }

    let end = dir.path().join(r.end);
    // Both meshes have 400 cells.
    let n = 400;
    let scalar = |name: &str| {
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
    let second = match setup.model {
        Rans::KEpsilon => "epsilon",
        Rans::KOmegaSst => "omega",
    };
    let (p, k, eps, nut) = (scalar("p"), scalar("k"), scalar(second), scalar("nut"));
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-12 * b.abs().max(1e-3);
    for (c, [ux, uy], pc, kc, ec, tc) in r.cells {
        assert!(
            close(u[c].x, ux) && close(u[c].y, uy),
            "U in cell {c}: {:?}",
            u[c]
        );
        assert!(close(p[c], pc), "p in cell {c}: {}", p[c]);
        assert!(close(k[c], kc), "k in cell {c}: {}", k[c]);
        assert!(close(eps[c], ec), "{second} in cell {c}: {}", eps[c]);
        assert!(close(nut[c], tc), "nut in cell {c}: {}", nut[c]);
    }
}

/// OpenFOAM v1912 meets the residual control after 55 iterations, and so
/// does the engine, every solve taking as many iterations as OpenFOAM's.
#[test]
fn kepsilon_channel_matches_openfoam() {
    check(
        &CHANNEL,
        &Reference {
            iterations: 55,
            totals: [
                ("Ux", 55),
                ("Uy", 55),
                ("p", 1411),
                ("epsilon", 43),
                ("k", 52),
            ],
            first: [("epsilon", 0.44066079156468857), ("k", 0.99999999999999845)],
            end: "55",
            cells: [
                (
                    0,
                    [9.9651123556574142, 0.0015493319392805831],
                    4.8098690250121745,
                    0.40342237264728209,
                    20.538465363011994,
                    0.00071317231880875824,
                ),
                (
                    205,
                    [10.054943197355621, -0.0017220701147942167],
                    4.12096349034341,
                    0.23857203587300105,
                    6.4126586260064,
                    0.00079880994230361712,
                ),
                (
                    399,
                    [8.8020513595146319, -0.004699803327319613],
                    0.040554233480886916,
                    0.65117822355511001,
                    42.118977931335486,
                    0.0009060755737503942,
                ),
            ],
        },
    );
}

/// The cavity's corner cells have two wall faces and its wall cells sit
/// below the log layer, which the ε wall function weights and the ν_t one
/// sets to zero.
#[test]
fn kepsilon_cavity_matches_openfoam() {
    check(
        &CAVITY,
        &Reference {
            iterations: 150,
            totals: [
                ("Ux", 300),
                ("Uy", 300),
                ("p", 2224),
                ("epsilon", 151),
                ("k", 240),
            ],
            first: [
                ("epsilon", 0.067750933551312648),
                ("k", 0.99999999999997213),
            ],
            end: "150",
            cells: [
                (
                    0,
                    [0.001738728782152463, -0.0032874822795229235],
                    4.3190382139112015e-07,
                    0.0001921284453595628,
                    0.00042300323416431313,
                    7.8538419760046958e-06,
                ),
                (
                    210,
                    [0.0053879268916013244, 0.0034381615708491842],
                    -0.016143125044324916,
                    0.00010895162453546411,
                    4.73376260987291e-05,
                    2.2568539490643986e-05,
                ),
                (
                    399,
                    [0.17686716698047655, -0.12147885080780901],
                    0.033912117474648604,
                    0.0096311196550028181,
                    0.15156655902433588,
                    5.5079840675593408e-05,
                ),
            ],
        },
    );
}

/// k-ω SST on the channel: OpenFOAM stops after 54 iterations, and so does
/// the engine, with the distance to the walls from the same mesh wave.
#[test]
fn komegasst_channel_matches_openfoam() {
    check(
        &Setup {
            model: Rans::KOmegaSst,
            ..CHANNEL
        },
        &Reference {
            iterations: 54,
            totals: [
                ("Ux", 54),
                ("Uy", 54),
                ("p", 1374),
                ("omega", 43),
                ("k", 52),
            ],
            first: [("omega", 0.29896978161581944), ("k", 0.99999999999999889)],
            end: "54",
            cells: [
                (
                    0,
                    [9.9639205815322107, 0.011341092807946386],
                    4.9184541597673723,
                    0.40394681371410734,
                    566.94443627607461,
                    0.00012534440828572145,
                ),
                (
                    205,
                    [10.05565167877071, -0.0020429988441917238],
                    4.247435078678568,
                    0.24009499085389524,
                    291.9912988696513,
                    0.00082226762161524811,
                ),
                (
                    399,
                    [8.6178076493487481, -0.0068826739493486888],
                    0.039197130296396689,
                    0.67193302203383887,
                    730.74409773480329,
                    0.00022276472611861505,
                ),
            ],
        },
    );
}

/// k-ω SST in the cavity, where the wall cells sit in the viscous layer and
/// the blended ω wall function and the production limiter both act.
#[test]
fn komegasst_cavity_matches_openfoam() {
    check(
        &Setup {
            model: Rans::KOmegaSst,
            ..CAVITY
        },
        &Reference {
            iterations: 150,
            totals: [
                ("Ux", 284),
                ("Uy", 287),
                ("p", 2268),
                ("omega", 172),
                ("k", 207),
            ],
            first: [("omega", 0.067849060148208684), ("k", 0.99999999999998801)],
            end: "150",
            cells: [
                (
                    0,
                    [0.0026477750695581286, -0.0092997240392845085],
                    5.3411133390467558e-08,
                    7.4305056877648659e-06,
                    128.09138772777922,
                    5.8009408903869729e-08,
                ),
                (
                    210,
                    [0.0042770173878814901, 0.0029013206322433729],
                    -0.012626075519472899,
                    5.2916371022482302e-05,
                    4.0855100766432519,
                    1.2952206708534079e-05,
                ),
                (
                    399,
                    [0.15987166535703215, -0.058593406718419949],
                    0.038550944226131111,
                    0.0064717662104797425,
                    192.14643542146041,
                    1.0450529058959387e-05,
                ),
            ],
        },
    );
}

/// What the RANS models cannot do yet is refused by name, and hostile values
/// end the run without a panic.
#[test]
fn unsupported_turbulence_settings_are_refused() {
    let sst = Setup {
        model: Rans::KOmegaSst,
        ..CHANNEL
    };
    let run_on = |setup: &Setup, file: &str, from: &str, to: &str| {
        let dir = tempfile::tempdir().unwrap();
        case(dir.path(), setup);
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
        Case::open(dir.path()).and_then(|case| simple::run(&case, |_| {}))
    };
    let run = |file: &str, from: &str, to: &str| run_on(&CHANNEL, file, from, to);
    let refuse_on = |setup: &Setup, file: &str, from: &str, to: &str, says: &str| {
        let err = run_on(setup, file, from, to).unwrap_err().to_string();
        assert!(err.contains(says), "{err}");
    };
    let refuse = |file: &str, from: &str, to: &str, says: &str| {
        refuse_on(&CHANNEL, file, from, to, says);
    };
    refuse(
        "constant/turbulenceProperties",
        "kEpsilon",
        "realizableKE",
        "RAS model 'realizableKE'",
    );
    // A switch is a word or a number: `0` is off like `off`.
    for off in ["off", "f", "0"] {
        refuse(
            "constant/turbulenceProperties",
            "turbulence on",
            &format!("turbulence {off}"),
            "turbulence off",
        );
    }
    refuse(
        "0/k",
        "inlet { type fixedValue;",
        "inlet { type turbulentIntensityKineticEnergyInlet; intensity 0.05;",
        "turbulentIntensityKineticEnergyInlet",
    );
    refuse(
        "0/nut",
        "walls { type nutkWallFunction;",
        "walls { type calculated;",
        "needs a nut wall function",
    );
    refuse(
        "0/epsilon",
        "walls { type epsilonWallFunction;",
        "walls { type epsilonWallFunction; lowReCorrection maybe;",
        "epsilonWallFunction",
    );
    refuse(
        "system/fvSchemes",
        "div(phi,k) bounded Gauss upwind;",
        "",
        "div(phi,k)",
    );
    refuse_on(
        &sst,
        "system/fvSchemes",
        "wallDist { method meshWave; }",
        "",
        "wallDist",
    );
    refuse_on(
        &sst,
        "system/fvSchemes",
        "method meshWave;",
        "method Poisson;",
        "wallDist method 'Poisson'",
    );
    refuse_on(
        &sst,
        "constant/turbulenceProperties",
        "printCoeffs on;",
        "printCoeffs on; kOmegaSSTCoeffs { decayControl yes; }",
        "decayControl",
    );
    for field in ["k", "omega"] {
        refuse_on(
            &sst,
            "system/fvSchemes",
            "gradSchemes { default Gauss linear; }",
            &format!("gradSchemes {{ default Gauss linear; grad({field}) leastSquares; }}"),
            "leastSquares",
        );
    }
    refuse_on(
        &sst,
        "0/omega",
        "walls { type omegaWallFunction;",
        "walls { type omegaWallFunction; blended perhaps;",
        "omegaWallFunction",
    );
    // ε = 0 and ω = 0 divide by zero in ν_t and the sources; the run may
    // fail, but must not panic.
    let _ = run(
        "0/epsilon",
        "internalField uniform 14.855",
        "internalField uniform 0",
    );
    let _ = run_on(
        &sst,
        "0/omega",
        "internalField uniform 440.15",
        "internalField uniform 0",
    );
}
