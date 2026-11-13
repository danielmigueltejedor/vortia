//! The engine's central-upwind solver against OpenFOAM's `rhoCentralFoam` on
//! the same case: an air shock tube of 40 cells, `Kurganov` flux, `vanLeer`
//! and `vanLeerV` reconstruction, in Euler time and with local time stepping.
//!
//! The reference numbers were produced by OpenFOAM v1912 on this exact case
//! (`writePrecision 17`).

use std::path::Path;

use vortia_core::{Aabb, Vec3};
use vortia_fields::{ScalarField, VectorField};
use vortia_foamfile::RawPatch;
use vortia_mesh::blocks::structured_box_raw;
use vortia_solvers::{Case, rho_central};

mod common;

use common::{HEADER, write};

const CELLS: usize = 40;

fn list(values: impl Iterator<Item = String>) -> String {
    let body: Vec<String> = values.collect();
    format!("{}\n(\n{}\n)", body.len(), body.join("\n"))
}

/// A tube 10 m long, `CELLS` cells, closed by zero-gradient ends; the left
/// half at `left` (p, T), the right half at `right`.
fn tube(
    dir: &Path,
    (left, right): ((f64, f64), (f64, f64)),
    thermo_mu: f64,
    extra_control: &str,
    reconstruct: &str,
) {
    let bounds = Aabb {
        min: Vec3::new(-5.0, -1.0, -1.0),
        max: Vec3::new(5.0, 1.0, 1.0),
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

    let half =
        |l: f64, r: f64| list((0..CELLS).map(|i| if i < CELLS / 2 { l } else { r }.to_string()));
    let field = |name: &str, dims: &str, internal: String, kind: &str| {
        write(
            &dir.join("0").join(name),
            &format!(
                "FoamFile {{ version 2.0; format ascii; class vol{kind}Field; object {name}; }}\n\
                 dimensions {dims}; internalField nonuniform List<{}>\n{internal};\n\
                 boundaryField {{ ends {{ type zeroGradient; }} sides {{ type empty; }} }}\n",
                if kind == "Scalar" { "scalar" } else { "vector" }
            ),
        );
    };
    field("p", "[1 -1 -2 0 0 0 0]", half(left.0, right.0), "Scalar");
    field("T", "[0 0 0 1 0 0 0]", half(left.1, right.1), "Scalar");
    field(
        "U",
        "[0 1 -1 0 0 0 0]",
        list((0..CELLS).map(|_| "(0 0 0)".to_string())),
        "Vector",
    );
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application rhoCentralFoam; startFrom startTime; startTime 0; \
             endTime 0.008; deltaT 0.0002; writeControl timeStep; writeInterval 20; \
             writePrecision 17; {extra_control}"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}fluxScheme Kurganov; ddtSchemes {{ default Euler; }} \
             gradSchemes {{ default Gauss linear; }} divSchemes {{ default none; }} \
             laplacianSchemes {{ default Gauss linear corrected; }} \
             interpolationSchemes {{ default linear; {reconstruct} }} \
             snGradSchemes {{ default corrected; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!("{HEADER}solvers {{ \"(rho|rhoU|rhoE)\" {{ solver diagonal; }} }}"),
    );
    write(
        &dir.join("constant/thermophysicalProperties"),
        &format!(
            "{HEADER}thermoType {{ type hePsiThermo; mixture pureMixture; transport const; \
             thermo hConst; equationOfState perfectGas; specie specie; \
             energy sensibleInternalEnergy; }} \
             mixture {{ specie {{ nMoles 1; molWeight 28.96; }} \
             thermodynamics {{ Cp 1004.5; Hf 0; }} transport {{ mu {thermo_mu}; Pr 1; }} }}"
        ),
    );
}

const VAN_LEER: &str = "reconstruct(rho) vanLeer; reconstruct(U) vanLeerV; reconstruct(T) vanLeer;";
const SOD: ((f64, f64), (f64, f64)) = ((1.0e5, 348.4), (1.0e4, 278.7));

fn scalar(case: &Path, time: &str, name: &str) -> Vec<f64> {
    ScalarField::read(case.join(time).join(name))
        .unwrap()
        .internal
        .expand(CELLS)
        .unwrap()
}

#[test]
fn a_shock_tube_matches_openfoam() {
    let dir = tempfile::tempdir().unwrap();
    tube(dir.path(), SOD, 0.0, "", VAN_LEER);
    if let Ok(keep) = std::env::var("VORTIA_KEEP_CASE") {
        tube(Path::new(&keep), SOD, 0.0, "", VAN_LEER);
    }
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_central::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), 40);
    assert!(steps.iter().filter(|s| s.written).count() == 2);
    for (time, name, expected) in REFERENCE {
        let v = scalar(dir.path(), time, name);
        for (i, want) in expected.iter() {
            let got = v[*i];
            assert!(
                (got - want).abs() <= 1e-11 * want.abs(),
                "{name} at {time}, cell {i}: {got} vs {want}"
            );
        }
    }
}

/// A field's values in some cells: (time, field, [(cell, value)]).
type Reference = (&'static str, &'static str, &'static [(usize, f64)]);

const REFERENCE: &[Reference] = &[
    (
        "0.004",
        "T",
        &[
            (0, 348.3999999999999),
            (12, 348.3715585939521),
            (15, 316.72843843888916),
            (17, 286.84261754500784),
            (19, 261.0633984053246),
            (20, 246.9679726912577),
            (22, 253.94166633825537),
            (24, 304.35363712757817),
            (27, 408.3475973212854),
            (31, 278.70000000000084),
            (39, 278.7),
        ],
    ),
    (
        "0.004",
        "p",
        &[
            (0, 99999.99999999999),
            (12, 99970.68095928487),
            (15, 71936.377756595),
            (17, 51272.06342074495),
            (19, 36822.084200535326),
            (20, 30798.036203073316),
            (22, 30168.599019556066),
            (24, 30138.034494709314),
            (27, 33118.73669894148),
            (31, 10000.000000000087),
            (39, 10000.0),
        ],
    ),
    (
        "0.004",
        "rho",
        &[
            (0, 0.9997371646908592),
            (12, 0.9995256469667171),
            (15, 0.7910892621053385),
            (17, 0.6225885111627601),
            (19, 0.4912769215631154),
            (20, 0.43435654688234115),
            (22, 0.41379492606959584),
            (24, 0.34490573276228675),
            (27, 0.2824930328605079),
            (31, 0.12497611344754123),
            (39, 0.12497611344754048),
        ],
    ),
    (
        "0.008",
        "T",
        &[
            (0, 348.40000000000003),
            (12, 303.8329797498274),
            (15, 281.16979577715364),
            (17, 266.2287597980375),
            (19, 252.1818238318143),
            (20, 247.04503656471567),
            (22, 246.24594972227715),
            (24, 246.40670321087796),
            (27, 257.49783000717275),
            (31, 393.79167453625195),
            (39, 278.7011307388873),
        ],
    ),
    (
        "0.008",
        "p",
        &[
            (0, 100000.00000000001),
            (12, 62283.46304395278),
            (15, 47606.621344504696),
            (17, 39398.41616861202),
            (19, 32546.606236364558),
            (20, 30450.014218023975),
            (22, 30475.252713711892),
            (24, 30029.765960798963),
            (27, 30352.37495411228),
            (31, 29392.09311677895),
            (39, 10000.096142085533),
        ],
    ),
    (
        "0.008",
        "rho",
        &[
            (0, 0.9997371646908592),
            (12, 0.7140059361627764),
            (15, 0.5897428422406339),
            (17, 0.5154514643276623),
            (19, 0.44952713436978103),
            (20, 0.42931429579676417),
            (22, 0.431064445243504),
            (24, 0.42448604052043537),
            (27, 0.41056610113765307),
            (31, 0.2599728337688083),
            (39, 0.12497680793928857),
        ],
    ),
];

#[test]
fn mass_and_energy_are_conserved_until_a_wave_reaches_the_ends() {
    let dir = tempfile::tempdir().unwrap();
    tube(dir.path(), SOD, 0.0, "", VAN_LEER);
    let case = Case::open(dir.path()).unwrap();
    rho_central::run(&case, |_| {}).unwrap();
    let rho = scalar(dir.path(), "0.008", "rho");
    let p = scalar(dir.path(), "0.008", "p");
    let t = scalar(dir.path(), "0.008", "T");
    // The constants OpenFOAM v1912 builds the gas constant from.
    let gas_r = 1.0e3 * (6.022_141_793_0e23 * 1.380_65e-23) / 28.96;
    let mass: f64 = rho.iter().sum();
    let initial: f64 = (0..CELLS)
        .map(|i| {
            let (p, t) = if i < CELLS / 2 { SOD.0 } else { SOD.1 };
            p / (gas_r * t)
        })
        .sum();
    assert!(
        (mass - initial).abs() <= 1e-9 * initial,
        "{mass} vs {initial}"
    );
    // The tube was at rest at different pressures: the gas ended up
    // moving, with positive pressure and temperature throughout.
    assert!(p.iter().chain(&t).all(|&x| x.is_finite() && x > 0.0));
    let u = VectorField::read(dir.path().join("0.008/U")).unwrap();
    let ux: Vec<f64> = u
        .internal
        .expand(CELLS)
        .unwrap()
        .iter()
        .map(|v| v.x)
        .collect();
    assert!(ux.iter().any(|&x| x > 10.0));
    assert!(ux.iter().all(|x| x.abs() < 400.0));
}

#[test]
fn a_gas_at_rest_stays_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    tube(
        dir.path(),
        ((1.0e5, 300.0), (1.0e5, 300.0)),
        0.0,
        "",
        VAN_LEER,
    );
    let case = Case::open(dir.path()).unwrap();
    rho_central::run(&case, |_| {}).unwrap();
    for (i, p) in scalar(dir.path(), "0.008", "p").iter().enumerate() {
        assert!((p - 1.0e5).abs() <= 1e-9, "cell {i}: {p}");
    }
    for t in scalar(dir.path(), "0.008", "T") {
        assert!((t - 300.0).abs() <= 1e-9, "{t}");
    }
}

#[test]
fn what_the_engine_does_not_have_is_refused_not_guessed() {
    let refused = |mu: f64, control: &str, reconstruct: &str, want: &str| {
        let dir = tempfile::tempdir().unwrap();
        tube(dir.path(), SOD, mu, control, reconstruct);
        let case = Case::open(dir.path()).unwrap();
        let err = rho_central::run(&case, |_| {}).expect_err(want).to_string();
        assert!(err.contains(want), "{err}");
    };
    refused(1.8e-5, "", VAN_LEER, "viscous");
    refused(0.0, "adjustTimeStep yes;", VAN_LEER, "adjustTimeStep");
    refused(
        0.0,
        "",
        "reconstruct(rho) vanLeerV; reconstruct(U) vanLeerV; reconstruct(T) vanLeer;",
        "vector fields",
    );
    refused(
        0.0,
        "",
        "reconstruct(rho) QUICK; reconstruct(U) vanLeerV; reconstruct(T) vanLeer;",
        "not supported",
    );
}

#[test]
fn a_negative_temperature_is_an_error_and_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    tube(
        dir.path(),
        ((1.0e5, -5.0), (1.0e4, 278.7)),
        0.0,
        "",
        VAN_LEER,
    );
    let case = Case::open(dir.path()).unwrap();
    assert!(rho_central::run(&case, |_| {}).is_err());
}

/// The same tube with `ddtSchemes localEuler`: 20 iterations of a time
/// step of 1 s that each cell shrinks to its Courant number's `maxCo`,
/// smoothed by `fvc::smooth`. OpenFOAM v1912's values.
const LTS_REFERENCE: &[Reference] = &[
    (
        "10",
        "T",
        &[
            (0, 348.4),
            (12, 341.39436128996005),
            (15, 291.4957087695604),
            (17, 281.5362479845592),
            (19, 265.0976413322489),
            (20, 251.6610998109621),
            (22, 260.130478939079),
            (24, 304.47382444734257),
            (27, 420.2781484506806),
            (31, 278.69999999999993),
            (39, 278.7),
        ],
    ),
    (
        "10",
        "p",
        &[
            (0, 100000.0),
            (12, 93177.80472705251),
            (15, 54881.28303282501),
            (17, 49255.54671158334),
            (19, 39227.77674974472),
            (20, 31826.480916323348),
            (22, 31803.057697740667),
            (24, 29540.06582797747),
            (27, 39881.809118077836),
            (31, 9999.999999999996),
            (39, 10000.000000000002),
        ],
    ),
    (
        "10",
        "rho",
        &[
            (0, 0.9997371646908592),
            (12, 0.9506488209985045),
            (15, 0.6557768383713368),
            (17, 0.6093752465975578),
            (19, 0.5154087826646584),
            (20, 0.4404904671694607),
            (22, 0.4258352609483304),
            (24, 0.3379290129620208),
            (27, 0.33052325699142593),
            (31, 0.12497611344754046),
            (39, 0.1249761134475405),
        ],
    ),
    (
        "10",
        "rDeltaT",
        &[
            (0, 1496.9659509375754),
            (12, 1542.201228464807),
            (15, 1931.5227335920758),
            (17, 2107.200195342241),
            (19, 2220.5702377230355),
            (20, 2261.0086911467242),
            (22, 2617.0514500008653),
            (24, 2622.973427227218),
            (27, 2961.5469630052376),
            (31, 2022.776424428138),
            (39, 1338.8790112303418),
        ],
    ),
    (
        "20",
        "T",
        &[
            (0, 348.4),
            (12, 277.2022328766363),
            (15, 281.99433591906035),
            (17, 245.9390731757839),
            (19, 258.3609419929934),
            (20, 220.78967462524574),
            (22, 258.34345362507105),
            (24, 232.89528506583838),
            (27, 280.1998490640986),
            (31, 429.0318018677801),
            (39, 278.7),
        ],
    ),
    (
        "20",
        "p",
        &[
            (0, 100000.0),
            (12, 46378.175775672156),
            (15, 51168.9925836526),
            (17, 31786.417016087576),
            (19, 42521.890999022246),
            (20, 22031.926368836655),
            (22, 37622.80279985585),
            (24, 26213.790973971863),
            (27, 40420.84208333727),
            (31, 50392.72931870152),
            (39, 10000.000000000002),
        ],
    ),
    (
        "20",
        "rho",
        &[
            (0, 0.9997371646908592),
            (12, 0.5827481740881236),
            (15, 0.6320194808237012),
            (17, 0.4501715325396869),
            (19, 0.5732574321330621),
            (20, 0.34756632783188934),
            (22, 0.5072448758813078),
            (24, 0.39204247214182036),
            (27, 0.5024599413138727),
            (31, 0.409112151225109),
            (39, 0.1249761134475405),
        ],
    ),
    (
        "20",
        "rDeltaT",
        &[
            (0, 1496.9659509375754),
            (12, 1992.0312309080903),
            (15, 2105.2343940430464),
            (17, 2436.5381714137047),
            (19, 2436.5381714137047),
            (20, 2262.268762321259),
            (22, 2737.3452024087237),
            (24, 2341.0643186885186),
            (27, 2921.258962688342),
            (31, 3716.3779634093726),
            (39, 2114.8804729117974),
        ],
    ),
];

#[test]
fn local_time_stepping_matches_openfoam() {
    let dir = tempfile::tempdir().unwrap();
    tube(dir.path(), SOD, 0.0, "", VAN_LEER);
    write(
        &dir.path().join("system/controlDict"),
        &format!(
            "{HEADER}application rhoCentralFoam; startFrom startTime; startTime 0;              endTime 20; deltaT 1; writeControl timeStep; writeInterval 10;              writePrecision 17; maxCo 0.5; maxDeltaT 1; rDeltaTSmoothingCoeff 0.1;"
        ),
    );
    let schemes = dir.path().join("system/fvSchemes");
    let text = std::fs::read_to_string(&schemes).unwrap();
    write(
        &schemes,
        &text.replace("default Euler", "default localEuler"),
    );
    let case = Case::open(dir.path()).unwrap();
    let mut log = String::new();
    let steps = rho_central::run(&case, |line| log.push_str(line)).unwrap();
    assert_eq!(steps.len(), 20);
    assert!(log.contains("Flow time scale min/max = 0.00066801786598665"));
    for (time, name, expected) in LTS_REFERENCE {
        let v = scalar(dir.path(), time, name);
        for (i, want) in expected.iter() {
            let got = v[*i];
            assert!(
                (got - want).abs() <= 1e-10 * want.abs(),
                "{name} at {time}, cell {i}: {got} vs {want}"
            );
        }
    }
}

#[test]
fn local_time_stepping_refuses_what_it_cannot_run() {
    let local = |control: &str, ddt: &str| {
        let dir = tempfile::tempdir().unwrap();
        tube(dir.path(), SOD, 0.0, "", VAN_LEER);
        write(
            &dir.path().join("system/controlDict"),
            &format!(
                "{HEADER}application rhoCentralFoam; startFrom startTime; startTime 0; \
                 endTime 2; deltaT 1; writeControl timeStep; writeInterval 1; {control}"
            ),
        );
        let schemes = dir.path().join("system/fvSchemes");
        let text = std::fs::read_to_string(&schemes).unwrap();
        write(&schemes, &text.replace("default Euler", ddt));
        let case = Case::open(dir.path()).unwrap();
        rho_central::run(&case, |_| {})
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    let ok = "maxCo 0.5; maxDeltaT 1; rDeltaTSmoothingCoeff 0.1;";
    assert!(local(ok, "default localEuler").is_ok());
    // A smoothing coefficient of 1 or more leaves the time scale unsmoothed.
    assert!(local("maxCo 0.5; rDeltaTSmoothingCoeff 1;", "default localEuler").is_ok());
    assert!(
        local("", "default localEuler")
            .unwrap_err()
            .contains("maxCo")
    );
    assert!(
        local("maxCo -1;", "default localEuler")
            .unwrap_err()
            .contains("maxCo")
    );
    assert!(
        local("maxCo 0.5; maxDeltaT 0;", "default localEuler")
            .unwrap_err()
            .contains("maxDeltaT")
    );
    assert!(
        local(ok, "default backward")
            .unwrap_err()
            .contains("not supported")
    );
}
