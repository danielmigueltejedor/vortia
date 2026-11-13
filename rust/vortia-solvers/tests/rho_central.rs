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

const SIDE: usize = 8;

/// OpenFOAM v1912's values after 40 steps on [`skewed_cell`].
const SKEWED_REFERENCE: &[Reference] = &[
    (
        "0.0004",
        "T",
        &[
            (0, 300.2873869904484),
            (7, 301.49212831781716),
            (9, 299.5658722245639),
            (18, 298.4420449600283),
            (27, 298.1410542928057),
            (28, 298.3477412767703),
            (35, 298.4509355105221),
            (36, 298.4630808860827),
            (45, 299.2600358222535),
            (54, 300.41934450265643),
            (63, 301.8515463528817),
        ],
    ),
    (
        "0.0004",
        "p",
        &[
            (0, 100727.01795194737),
            (7, 103980.07010466969),
            (9, 100659.78867776744),
            (18, 99918.08833887035),
            (27, 99748.7141985587),
            (28, 100053.49829737605),
            (35, 99998.09420992833),
            (36, 100174.95110606232),
            (45, 101103.1668758527),
            (54, 102488.29628151405),
            (63, 105554.21985377886),
        ],
    ),
    (
        "0.0004",
        "rho",
        &[
            (0, 1.1683497482045653),
            (7, 1.201263030716618),
            (9, 1.1703820770621691),
            (18, 1.1661330192451007),
            (27, 1.1653315554852912),
            (28, 1.1680824723713845),
            (35, 1.1670319932321764),
            (36, 1.1690484350360115),
            (45, 1.1767386527779562),
            (54, 1.188256949417914),
            (63, 1.2179968878435048),
        ],
    ),
];

/// `U` in the same cells: (cell, x, y).
const SKEWED_VELOCITY: &[(usize, f64, f64)] = &[
    (0, -0.8623096988324563, -0.49900548083730545),
    (7, -0.44138520051197666, -0.6719728812501325),
    (9, -1.453136593380057, -0.12118819392313929),
    (18, -0.9663691592784907, 2.582941373979926),
    (27, -3.4589060712909148, 2.3526926605438714),
    (28, -4.671554795272886, -0.3799495680389129),
    (35, -5.523047640709842, 2.488303192258605),
    (36, -6.418225435554284, -0.7183681901270215),
    (45, -4.946571163376982, -3.0461012400884138),
    (54, -0.20808016624992423, -3.8434941576941846),
    (63, 6.9895119608179375, -0.789902626454534),
];

/// A viscous gas in a skewed 8 × 8 box (`x += 0.3 y`, so every face is off
/// orthogonal) closed by walls, the top one moving, with a pressure bump in
/// the middle: the stress terms, the corrected Laplacians and the
/// conditions of the velocity and the energy all take part.
fn skewed_cell(dir: &Path) {
    let bounds = Aabb {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(0.1, 0.1, 0.01),
    };
    let mut raw = structured_box_raw(bounds, [SIDE, SIDE, 1]).unwrap();
    for point in &mut raw.points {
        point.x += 0.3 * point.y;
    }
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
        merge("walls", "wall", &p[0..3]),
        merge("lid", "wall", &p[3..4]),
        merge("frontAndBack", "empty", &p[4..6]),
    ];
    raw.write_case(dir).unwrap();

    let bump = list((0..SIDE * SIDE).map(|c| {
        let (i, j) = (c % SIDE, c / SIDE);
        if (3..5).contains(&i) && (3..5).contains(&j) {
            "1.2e5"
        } else {
            "1e5"
        }
        .to_string()
    }));
    let field = |name: &str, dims: &str, internal: &str, kind: &str, walls: &str, lid: &str| {
        write(
            &dir.join("0").join(name),
            &format!(
                "FoamFile {{ version 2.0; format ascii; class vol{kind}Field; object {name}; }}\n\
                 dimensions {dims}; internalField {internal};\n\
                 boundaryField {{ walls {{ {walls} }} lid {{ {lid} }} \
                 frontAndBack {{ type empty; }} }}\n"
            ),
        );
    };
    let zero_gradient = "type zeroGradient;";
    field(
        "p",
        "[1 -1 -2 0 0 0 0]",
        &format!("nonuniform List<scalar>\n{bump}"),
        "Scalar",
        zero_gradient,
        zero_gradient,
    );
    field(
        "T",
        "[0 0 0 1 0 0 0]",
        "uniform 300",
        "Scalar",
        "type fixedValue; value uniform 300;",
        zero_gradient,
    );
    field(
        "U",
        "[0 1 -1 0 0 0 0]",
        "uniform (0 0 0)",
        "Vector",
        "type fixedValue; value uniform (0 0 0);",
        "type fixedValue; value uniform (30 0 0);",
    );
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application rhoCentralFoam; startFrom startTime; startTime 0; \
             endTime 0.0004; deltaT 0.00001; writeControl timeStep; writeInterval 40; \
             writePrecision 17;"
        ),
    );
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}fluxScheme Kurganov; ddtSchemes {{ default Euler; }} \
             gradSchemes {{ default Gauss linear; }} \
             divSchemes {{ default none; div(tauMC) Gauss linear; }} \
             laplacianSchemes {{ default Gauss linear corrected; }} \
             interpolationSchemes {{ default linear; {VAN_LEER} }} \
             snGradSchemes {{ default corrected; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \"(rho|rhoU|rhoE)\" {{ solver diagonal; }} \
             \"(U|e)\" {{ solver PCG; preconditioner DIC; tolerance 1e-15; relTol 0; \
             maxIter 1000; }} }}"
        ),
    );
    write(
        &dir.join("constant/thermophysicalProperties"),
        &format!(
            "{HEADER}thermoType {{ type hePsiThermo; mixture pureMixture; transport const; \
             thermo hConst; equationOfState perfectGas; specie specie; \
             energy sensibleInternalEnergy; }} \
             mixture {{ specie {{ nMoles 1; molWeight 28.96; }} \
             thermodynamics {{ Cp 1004.5; Hf 0; }} transport {{ mu 0.5; Pr 0.7; }} }}"
        ),
    );
}

#[test]
fn a_viscous_gas_on_a_skewed_mesh_matches_openfoam() {
    let dir = tempfile::tempdir().unwrap();
    skewed_cell(dir.path());
    if let Ok(keep) = std::env::var("VORTIA_KEEP_CASE") {
        skewed_cell(Path::new(&keep));
    }
    let case = Case::open(dir.path()).unwrap();
    let mut log = String::new();
    rho_central::run(&case, |line| log.push_str(line)).unwrap();
    assert!(log.contains("Solving for Ux"), "{log}");
    assert!(log.contains("Solving for Uy"), "{log}");
    assert!(log.contains("Solving for e,"), "{log}");
    let close = |got: f64, want: f64, what: &str| {
        assert!(
            (got - want).abs() <= 1e-10 * want.abs().max(1e-3),
            "{what}: {got} vs {want}"
        );
    };
    for (time, name, expected) in SKEWED_REFERENCE {
        let v = ScalarField::read(dir.path().join(time).join(name))
            .unwrap()
            .internal
            .expand(SIDE * SIDE)
            .unwrap();
        for (i, want) in expected.iter() {
            close(v[*i], *want, &format!("{name}, cell {i}"));
        }
    }
    let u = VectorField::read(dir.path().join("0.0004/U"))
        .unwrap()
        .internal
        .expand(SIDE * SIDE)
        .unwrap();
    for (i, x, y) in SKEWED_VELOCITY {
        close(u[*i].x, *x, &format!("Ux, cell {i}"));
        close(u[*i].y, *y, &format!("Uy, cell {i}"));
    }
}

#[test]
fn a_viscous_gas_needs_the_schemes_of_its_stress() {
    let run = |from: &str, to: &str| {
        let dir = tempfile::tempdir().unwrap();
        skewed_cell(dir.path());
        let schemes = dir.path().join("system/fvSchemes");
        let text = std::fs::read_to_string(&schemes).unwrap();
        assert!(text.contains(from), "{text}");
        write(&schemes, &text.replace(from, to));
        let case = Case::open(dir.path()).unwrap();
        rho_central::run(&case, |_| {})
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    assert!(
        run("div(tauMC) Gauss linear;", "")
            .unwrap_err()
            .contains("div(tauMC)")
    );
    assert!(
        run("div(tauMC) Gauss linear;", "div(tauMC) Gauss upwind;")
            .unwrap_err()
            .contains("div(tauMC)")
    );
    // A gradient scheme the engine does not have is refused, not replaced.
    assert!(run("default Gauss linear;", "default Gauss pointLinear;").is_err());
    // A limited scheme needs a coefficient from 0 to 1.
    for bad in ["limited", "limited 1.5", "limited x"] {
        let e = run(
            "laplacianSchemes { default Gauss linear corrected; }",
            &format!("laplacianSchemes {{ default Gauss linear {bad}; }}"),
        )
        .unwrap_err();
        assert!(e.contains("limited"), "{bad}: {e}");
    }
    assert!(
        run(
            "snGradSchemes { default corrected; }",
            "snGradSchemes { default limited 2; }",
        )
        .is_err()
    );
}

/// The nozzle's wedge mesh (7500 cells, as the example case has it): air at
/// 1.5 bar pushed in at 30 m/s through a 320 K inlet and let out at
/// 1.01325 bar, viscous (mu 0.05, a value that makes the stress matter in
/// twenty iterations), with local time stepping and the nozzle case's
/// `Minmod` for the velocity. OpenFOAM v1912 ran the same files for the
/// reference.
fn wedge(dir: &Path, nozzle_schemes: bool, spalart: bool) {
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
                 boundaryField {{ axis {{ type empty; }} front {{ type wedge; }} \
                 back {{ type wedge; }} {patches} }}\n"
            ),
        );
    };
    field(
        "p",
        "volScalarField",
        "[1 -1 -2 0 0 0 0]",
        "150000",
        "inlet { type zeroGradient; } walls { type zeroGradient; } \
         atmosphere { type fixedValue; value uniform 101325; }",
    );
    field(
        "T",
        "volScalarField",
        "[0 0 0 1 0 0 0]",
        "300",
        "inlet { type fixedValue; value uniform 320; } walls { type zeroGradient; } \
         atmosphere { type zeroGradient; }",
    );
    field(
        "U",
        "volVectorField",
        "[0 1 -1 0 0 0 0]",
        "(0 0 0)",
        "inlet { type fixedValue; value uniform (30 0 0); } \
         walls { type fixedValue; value uniform (0 0 0); } \
         atmosphere { type zeroGradient; }",
    );
    if spalart {
        field(
            "nuTilda",
            "volScalarField",
            "[0 2 -1 0 0 0 0]",
            "0.09",
            "inlet { type fixedValue; value uniform 0.09; } \
             walls { type fixedValue; value uniform 0; } \
             atmosphere { type zeroGradient; }",
        );
        field(
            "nut",
            "volScalarField",
            "[0 2 -1 0 0 0 0]",
            "0",
            "inlet { type calculated; value uniform 0; } \
             walls { type nutUSpaldingWallFunction; value uniform 0; } \
             atmosphere { type calculated; value uniform 0; }",
        );
        field(
            "alphat",
            "volScalarField",
            "[1 -1 -1 0 0 0 0]",
            "0",
            "inlet { type calculated; value uniform 0; } \
             walls { type compressible::alphatWallFunction; Prt 0.85; value uniform 0; } \
             atmosphere { type calculated; value uniform 0; }",
        );
        write(
            &dir.join("constant/turbulenceProperties"),
            &format!(
                "{HEADER}simulationType RAS; \
                 RAS {{ RASModel SpalartAllmaras; turbulence on; printCoeffs on; }}"
            ),
        );
    }
    write(
        &dir.join("system/controlDict"),
        &format!(
            "{HEADER}application rhoCentralFoam; startFrom startTime; startTime 0; \
             endTime 20; deltaT 1; writeControl timeStep; writeInterval 20; \
             writePrecision 17; maxCo 0.5; maxDeltaT 1; rDeltaTSmoothingCoeff 0.1;"
        ),
    );
    // The nozzle case's own gradient, Laplacian and snGrad schemes, or the
    // plain ones.
    let div_nu_tilda = if spalart {
        "div(phi,nuTilda) bounded Gauss upwind;"
    } else {
        ""
    };
    let (grad, laplacian, sn_grad) = if nozzle_schemes {
        (
            "cellLimited Gauss linear 1",
            "Gauss linear limited 0.5",
            "limited 0.5",
        )
    } else {
        ("Gauss linear", "Gauss linear corrected", "corrected")
    };
    write(
        &dir.join("system/fvSchemes"),
        &format!(
            "{HEADER}fluxScheme Kurganov; ddtSchemes {{ default localEuler; }} \
             gradSchemes {{ default {grad}; }} \
             divSchemes {{ default none; div(tauMC) Gauss linear; {div_nu_tilda} }} \
             wallDist {{ method meshWave; }} \
             laplacianSchemes {{ default {laplacian}; }} \
             interpolationSchemes {{ default linear; reconstruct(rho) vanLeer; \
             reconstruct(U) Minmod; reconstruct(T) vanLeer; }} \
             snGradSchemes {{ default {sn_grad}; }}"
        ),
    );
    write(
        &dir.join("system/fvSolution"),
        &format!(
            "{HEADER}solvers {{ \"(rho|rhoU|rhoE)\" {{ solver diagonal; }} \
             \"(U|e)\" {{ solver PCG; preconditioner DIC; tolerance 1e-15; relTol 0; \
             maxIter 1000; }} nuTilda {{ solver smoothSolver; smoother symGaussSeidel; \
             tolerance 1e-13; relTol 0; maxIter 1000; }} }}"
        ),
    );
    let mu = if spalart { "0.004" } else { "0.05" };
    write(
        &dir.join("constant/thermophysicalProperties"),
        &format!(
            "{HEADER}thermoType {{ type hePsiThermo; mixture pureMixture; transport const; \
             thermo hConst; equationOfState perfectGas; specie specie; \
             energy sensibleInternalEnergy; }} \
             mixture {{ specie {{ nMoles 1; molWeight 28.96; }} \
             thermodynamics {{ Cp 1004.5; Hf 0; }} transport {{ mu {mu}; Pr 0.7; }} }}"
        ),
    );
}

/// `(cell, T, p, rho, Ux, Uy)` after twenty iterations: a cell at the
/// inlet next to the axis, one at the corner of the inlet and the wall, one
/// in the middle and one at the outlet.
const WEDGE_REFERENCE: [(usize, f64, f64, f64, f64, f64); 4] = [
    (
        0,
        314.4964727976364,
        158911.81416801992,
        1.7599664542959659,
        17.377704437762045,
        -0.101085400799401,
    ),
    (
        2175,
        309.8433593541364,
        158013.43440646815,
        1.7762979036857756,
        10.378570026014476,
        -0.8076169334014913,
    ),
    (
        3749,
        292.7572916615782,
        145963.10798244664,
        1.7365982730894725,
        72.23896468465378,
        1.0771311963249062,
    ),
    (
        74,
        299.559343436799,
        149704.93198580586,
        1.740673115125714,
        16.546018858752536,
        0.03099519380038165,
    ),
];

const WEDGE_NOZZLE_SCHEMES: [(usize, f64, f64, f64, f64, f64); 4] = [
    (
        0,
        314.4675244650157,
        158881.3448077856,
        1.7597909854443516,
        17.66151475903225,
        -0.07040593995598096,
    ),
    (
        2175,
        309.8202589482329,
        158111.95811834003,
        1.7775379762236037,
        10.409595299307394,
        -0.882894812299362,
    ),
    (
        3749,
        292.6910667221694,
        145643.42930934296,
        1.7331869573384402,
        73.7200582953694,
        -0.2719980325010037,
    ),
    (
        74,
        299.5735903403249,
        149725.83998142133,
        1.7408334266835384,
        16.794733338617824,
        -0.056618064958585734,
    ),
];

/// Runs the wedge case twenty iterations and checks the four cells.
fn check_wedge(
    nozzle_schemes: bool,
    edit: &dyn Fn(&Path),
    reference: &[(usize, f64, f64, f64, f64, f64)],
    rel: f64,
) {
    let dir = tempfile::tempdir().unwrap();
    wedge(dir.path(), nozzle_schemes, false);
    edit(dir.path());
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_central::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), 20);
    let end = dir.path().join("20");
    let read = |name: &str| {
        ScalarField::read(end.join(name))
            .unwrap()
            .internal
            .expand(7500)
            .unwrap()
    };
    let (t, p, rho) = (read("T"), read("p"), read("rho"));
    let u = VectorField::read(end.join("U"))
        .unwrap()
        .internal
        .expand(7500)
        .unwrap();
    let close = |got: f64, want: f64, what: &str| {
        assert!(
            (got - want).abs() <= rel * want.abs().max(1e-3),
            "{what}: {got} vs {want}"
        );
    };
    for &(c, tc, pc, rc, ux, uy) in reference {
        close(t[c], tc, &format!("T, cell {c}"));
        close(p[c], pc, &format!("p, cell {c}"));
        close(rho[c], rc, &format!("rho, cell {c}"));
        close(u[c].x, ux, &format!("Ux, cell {c}"));
        close(u[c].y, uy, &format!("Uy, cell {c}"));
        assert!(u[c].z.abs() < 1e-9, "Uz in cell {c}: {}", u[c].z);
    }
}

/// A wedge side turns the velocity onto itself and, being a constraint,
/// takes the cell's density the moment the continuity equation has set it:
/// the faces of the other patches keep the density the step began with. A
/// stale density on the wedge made the limited schemes differ from OpenFOAM
/// by 1e-8 once a pressure wave had reached a cell with a wedge face.
#[test]
fn a_viscous_gas_on_a_wedge_matches_openfoam() {
    check_wedge(false, &|_| {}, &WEDGE_REFERENCE, 1e-10);
}

/// The same with what the nozzle case sets: `cellLimited Gauss linear 1` for
/// every gradient, the limiters' included, and `limited 0.5` for the
/// Laplacians and `snGrad(U)`.
#[test]
fn the_nozzle_cases_gradient_and_limited_schemes_match_openfoam() {
    check_wedge(true, &|_| {}, &WEDGE_NOZZLE_SCHEMES, 1e-9);
}

/// The nozzle case's own conditions, which follow the pressure: total
/// pressure and temperature at the inlet, a free atmosphere at the outlet.
fn driven_conditions(dir: &Path) {
    let field = |name: &str, class: &str, dims: &str, internal: &str, patches: &str| {
        write(
            &dir.join("0").join(name),
            &format!(
                "FoamFile {{ version 2.0; format ascii; class {class}; object {name}; }}\n\
                 dimensions {dims}; internalField uniform {internal};\n\
                 boundaryField {{ axis {{ type empty; }} front {{ type wedge; }} \
                 back {{ type wedge; }} {patches} }}\n"
            ),
        );
    };
    field(
        "p",
        "volScalarField",
        "[1 -1 -2 0 0 0 0]",
        "150000",
        "inlet { type totalPressure; p0 uniform 150000; U U; phi phi; rho rho; \
         psi thermo:psi; gamma 1.4; value uniform 150000; } walls { type zeroGradient; } \
         atmosphere { type totalPressure; p0 uniform 101325; U U; phi phi; rho rho; \
         psi thermo:psi; gamma 1.4; value uniform 101325; }",
    );
    field(
        "U",
        "volVectorField",
        "[0 1 -1 0 0 0 0]",
        "(0 0 0)",
        "inlet { type pressureInletOutletVelocity; value uniform (0 0 0); } \
         walls { type fixedValue; value uniform (0 0 0); } \
         atmosphere { type pressureInletOutletVelocity; value uniform (0 0 0); }",
    );
    field(
        "T",
        "volScalarField",
        "[0 0 0 1 0 0 0]",
        "300",
        "inlet { type totalTemperature; T0 uniform 320; U U; phi phi; psi thermo:psi; \
         gamma 1.4; value uniform 320; } walls { type zeroGradient; } \
         atmosphere { type inletOutletTotalTemperature; T0 uniform 300; U U; phi phi; \
         psi thermo:psi; gamma 1.4; value uniform 300; }",
    );
}

/// `(cell, T, p, rho, Ux, Uy)` after twenty iterations with those conditions.
const WEDGE_DRIVEN: [(usize, f64, f64, f64, f64, f64); 4] = [
    (
        0,
        313.48550875973984,
        156748.23646625303,
        1.7416030514228373,
        -0.4989408985253334,
        0.004060468383132111,
    ),
    (
        74,
        299.57310652240784,
        149725.61632735777,
        1.7408336377854285,
        16.794706158717194,
        -0.056607796744852765,
    ),
    (
        3749,
        292.66339138153785,
        145631.22168852048,
        1.73320556700189,
        73.7140369465763,
        -0.2754374997293082,
    ),
    (
        7499,
        271.0349326339501,
        114741.86092119836,
        1.4745537350231968,
        63.006833442621556,
        68.7367769924979,
    ),
];

/// Total pressure and temperature (the form with `psi` and `gamma`), the
/// velocity that follows the pressure and the temperature that follows the
/// flow, on the nozzle's wedge, agree with OpenFOAM's.
#[test]
fn the_nozzles_pressure_driven_conditions_match_openfoam() {
    check_wedge(true, &driven_conditions, &WEDGE_DRIVEN, 1e-9);
}

/// The code of `machAwarePressureOutlet`, as the nozzle example writes it.
fn pressure_outlet(p_far: &str) -> String {
    format!(
        "type codedMixed;\nname machAwarePressureOutlet;\n\
         refValue uniform {p_far}; refGradient uniform 0; valueFraction uniform 1; \
         value uniform {p_far};\ncode\n#{{\n\
         \x20const auto& u = patch().lookupPatchField<volVectorField>(\"U\");\n\
         \x20const auto& t = patch().lookupPatchField<volScalarField>(\"T\");\n\
         \x20const vectorField n(patch().nf());\n\
         \x20this->refValue() = {p_far};\n\
         \x20this->refGrad() = 0;\n\
         \x20forAll(this->valueFraction(),i)\n\
         \x20{{\n\
         \x20 const scalar c = sqrt(1.4*287.0*max(t[i],scalar(1)));\n\
         \x20 this->valueFraction()[i] = ((u[i]&n[i]) >= c) ? 0 : 1;\n\
         \x20}}\n#}};"
    )
}

/// Where the flow stays below the speed of sound the programmed pressure
/// outlet is a fixed pressure, which OpenFOAM runs as it is: the wedge with
/// the nozzle's schemes and a fixed atmosphere is the reference.
#[test]
fn the_programmed_pressure_outlet_is_a_fixed_pressure_in_subsonic_flow() {
    check_wedge(
        true,
        &|dir| {
            let path = dir.join("0/p");
            let text = std::fs::read_to_string(&path).unwrap();
            let fixed = "atmosphere { type fixedValue; value uniform 101325; }";
            assert!(text.contains(fixed));
            let coded = format!("atmosphere {{ {} }}", pressure_outlet("101325.0"));
            std::fs::write(&path, text.replace(fixed, &coded)).unwrap();
        },
        &WEDGE_NOZZLE_SCHEMES,
        1e-9,
    );
}

/// Everything the app's case programs, together, on the wedge: the run
/// finishes, the gas stays physical and the reservoir condition gives the
/// outgoing characteristic where the flow comes in.
#[test]
fn the_apps_programmed_conditions_run_together() {
    let dir = tempfile::tempdir().unwrap();
    wedge(dir.path(), true, true);
    driven_conditions(dir.path());
    let edit = |file: &str, from: &str, to: &str| {
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file}: {from}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    };
    edit(
        "0/p",
        "atmosphere { type totalPressure; p0 uniform 101325; U U; phi phi; rho rho; \
         psi thermo:psi; gamma 1.4; value uniform 101325; }",
        &format!("atmosphere {{ {} }}", pressure_outlet("101325.0")),
    );
    let reservoir = "type codedFixedValue;\nvalue uniform (0 0 0);\n\
        name reservoirCharacteristicVelocity;\ncode\n#{\n\
        \x20const scalar gamma = 1.4, gasR = 287.0, totalT = 300.0;\n\
        \x20const auto& T = patch().lookupPatchField<volScalarField>(\"T\");\n\
        \x20const scalarField ti(T.patchInternalField());\n\
        \x20const vectorField ui(this->patchInternalField());\n\
        \x20const vectorField n(patch().nf());\n\
        \x20vectorField ub(ui);\n\
        \x20const scalar cstar = sqrt(2*gamma*gasR*totalT/(gamma+1));\n\
        \x20forAll(ub,i) {\n\
        \x20 const scalar un = ui[i]&n[i];\n\
        \x20 if (un <= 0) {\n\
        \x20  const scalar ri = un+2*sqrt(gamma*gasR*max(ti[i],scalar(1)))/(gamma-1);\n\
        \x20  const scalar disc = -2*(gamma-1)*sqr(ri)+4*(gamma+1)*gamma*gasR*totalT/(gamma-1);\n\
        \x20  const scalar q = min(cstar,max(scalar(0),(-(gamma-1)*ri+sqrt(max(disc,scalar(0))))/(gamma+1)));\n\
        \x20  ub[i] = -n[i]*q;\n\
        \x20 }\n\
        \x20}\n\
        \x20operator==(ub);\n#};";
    edit(
        "0/U",
        "atmosphere { type pressureInletOutletVelocity; value uniform (0 0 0); }",
        &format!("atmosphere {{ {reservoir} }}"),
    );
    let ratio = "type codedMixed;\nname viscosityRatioNuTilda;\n\
        refValue uniform 0.0007; refGradient uniform 0; valueFraction uniform 1; \
        value uniform 0.0007;\ncode\n#{\n\
        \x20const auto& u = patch().lookupPatchField<volVectorField>(\"U\");\n\
        \x20const auto& rho = patch().lookupPatchField<volScalarField>(\"rho\");\n\
        \x20const vectorField n(patch().nf());\n\
        \x20this->refGrad() = 0;\n\
        \x20forAll(this->valueFraction(),i)\n\
        \x20{\n\
        \x20 this->refValue()[i] = 0.0002/max(rho[i],scalar(1e-12));\n\
        \x20 this->valueFraction()[i] = ((u[i]&n[i]) <= 0) ? 1 : 0;\n\
        \x20}\n#};";
    edit(
        "0/nuTilda",
        "atmosphere { type zeroGradient; }",
        &format!("atmosphere {{ {ratio} }}"),
    );
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_central::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), 20);
    let end = dir.path().join("20");
    let read = |name: &str| {
        ScalarField::read(end.join(name))
            .unwrap()
            .internal
            .expand(7500)
            .unwrap()
    };
    let (t, p, nu_tilda) = (read("T"), read("p"), read("nuTilda"));
    assert!(t.iter().all(|&t| t.is_finite() && t > 100.0 && t < 1000.0));
    assert!(p.iter().all(|&p| p.is_finite() && p > 1.0e4));
    assert!(nu_tilda.iter().all(|&n| n.is_finite() && n >= 0.0));
    // The programs are written back as they were read, code included.
    let u_text = std::fs::read_to_string(end.join("U")).unwrap();
    assert!(
        u_text.contains("reservoirCharacteristicVelocity") && u_text.contains("operator==(ub);"),
        "{u_text}"
    );
}

/// What differs from the code the engine knows is refused, not run.
#[test]
fn an_edited_program_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    wedge(dir.path(), false, false);
    let path = dir.path().join("0/p");
    let text = std::fs::read_to_string(&path).unwrap();
    let fixed = "atmosphere { type fixedValue; value uniform 101325; }";
    let edited = pressure_outlet("101325.0").replace(">= c", "> c");
    std::fs::write(
        &path,
        text.replace(fixed, &format!("atmosphere {{ {edited} }}")),
    )
    .unwrap();
    let case = Case::open(dir.path()).unwrap();
    let e = rho_central::run(&case, |_| {}).unwrap_err().to_string();
    assert!(e.contains("differs"), "{e}");
}

/// `(cell, T, p, rho, Ux, Uy, nuTilda, nut, alphat)` after twenty iterations
/// of the nozzle's schemes with Spalart–Allmaras, `nutUSpaldingWallFunction`
/// and `compressible::alphatWallFunction`, in the cells of `WEDGE_REFERENCE`.
type SpalartRow = (usize, [f64; 5], [f64; 3]);

const SPALART_REFERENCE: [SpalartRow; 4] = [
    (
        0,
        [
            317.6096889884029,
            160296.19531019704,
            1.7578971223857849,
            25.613381125816236,
            -0.11322483012443968,
        ],
        [
            0.06194184421661786,
            0.06086198198771704,
            0.10698910299890325,
        ],
    ),
    (
        2175,
        [
            316.37045468741076,
            159848.28968152916,
            1.759851645469654,
            24.87405033986418,
            1.9899999016615053,
        ],
        [
            0.020234876923422414,
            0.013424998185898932,
            0.023626005147881355,
        ],
    ),
    (
        3749,
        [
            296.40365030615294,
            151685.6676687623,
            1.782481303057863,
            50.102857317953095,
            -1.8207384210702973,
        ],
        [0.08904966119775758, 0.08854251275173182, 0.1578253735057244],
    ),
    (
        74,
        [
            299.6516109510237,
            149905.30157884036,
            1.742466186075441,
            15.621107716892633,
            -0.163478014939858,
        ],
        [0.08979206976211496, 0.0892582479030983, 0.15552947879948792],
    ),
];

/// `(wall face, nut, alphat)` on the wall patch after the same run.
const SPALART_WALL: [(usize, f64, f64); 4] = [
    (0, 2.8837178707875194e-08, 5.897004550636459e-08),
    (10, 8.804187362661253e-09, 1.7759610310015812e-08),
    (35, 4.5464717060572646e-13, 9.315214024868711e-13),
    (69, 1.0145559627703152e-07, 1.718813170164361e-07),
];

/// Spalart–Allmaras on the wedge: the model's equation, its wall distance,
/// the Spalding wall function and the turbulent diffusion of momentum and
/// energy agree with OpenFOAM's.
#[test]
fn spalart_allmaras_on_a_wedge_matches_openfoam() {
    let dir = tempfile::tempdir().unwrap();
    wedge(dir.path(), true, true);
    let case = Case::open(dir.path()).unwrap();
    let steps = rho_central::run(&case, |_| {}).unwrap();
    assert_eq!(steps.len(), 20);
    let end = dir.path().join("20");
    let read = |name: &str| {
        ScalarField::read(end.join(name))
            .unwrap()
            .internal
            .expand(7500)
            .unwrap()
    };
    let (t, p, rho) = (read("T"), read("p"), read("rho"));
    let (nu_tilda, nut, alphat) = (read("nuTilda"), read("nut"), read("alphat"));
    let u = VectorField::read(end.join("U"))
        .unwrap()
        .internal
        .expand(7500)
        .unwrap();
    let close = |got: f64, want: f64, what: &str| {
        assert!(
            (got - want).abs() <= 1e-8 * want.abs().max(1e-3),
            "{what}: {got} vs {want}"
        );
    };
    for &(c, [tc, pc, rc, ux, uy], [nt, nu_t, at]) in &SPALART_REFERENCE {
        close(t[c], tc, &format!("T, cell {c}"));
        close(p[c], pc, &format!("p, cell {c}"));
        close(rho[c], rc, &format!("rho, cell {c}"));
        close(u[c].x, ux, &format!("Ux, cell {c}"));
        close(u[c].y, uy, &format!("Uy, cell {c}"));
        close(nu_tilda[c], nt, &format!("nuTilda, cell {c}"));
        close(nut[c], nu_t, &format!("nut, cell {c}"));
        close(alphat[c], at, &format!("alphat, cell {c}"));
    }
    let wall = |name: &str| {
        let field = ScalarField::read(end.join(name)).unwrap();
        field
            .boundary
            .iter()
            .find(|b| b.key == "walls")
            .and_then(|b| b.value.as_ref())
            .and_then(|v| v.expand(70))
            .unwrap()
    };
    let (wall_nut, wall_alphat) = (wall("nut"), wall("alphat"));
    for &(f, n, a) in &SPALART_WALL {
        let tiny = |got: f64, want: f64, what: &str| {
            assert!(
                (got - want).abs() <= 1e-8 * want.abs().max(1e-7),
                "{what}, wall face {f}: {got} vs {want}"
            );
        };
        tiny(wall_nut[f], n, "nut");
        tiny(wall_alphat[f], a, "alphat");
    }
}

/// Spalart–Allmaras needs a viscous gas, and `simpleFoam` does not run it.
#[test]
fn spalart_allmaras_asks_for_what_it_needs() {
    let dir = tempfile::tempdir().unwrap();
    wedge(dir.path(), false, true);
    let thermo = dir.path().join("constant/thermophysicalProperties");
    let text = std::fs::read_to_string(&thermo).unwrap();
    std::fs::write(&thermo, text.replace("mu 0.004", "mu 0")).unwrap();
    let case = Case::open(dir.path()).unwrap();
    let err = rho_central::run(&case, |_| {}).unwrap_err().to_string();
    assert!(err.contains("viscous"), "{err}");
}
