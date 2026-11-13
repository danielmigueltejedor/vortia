use std::fmt::Write as _;

use tempfile::TempDir;
use vortia_core::{Aabb, Vec3};
use vortia_mesh::blocks::structured_box_raw;

use crate::{Case, FieldValues, line_csv};

/// A 4 × 2 × 1 box of unit cells, written as a case with times `0`.
fn case() -> (TempDir, Case) {
    let dir = TempDir::new().unwrap();
    let bounds = Aabb {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(4.0, 2.0, 1.0),
    };
    structured_box_raw(bounds, [4, 2, 1])
        .unwrap()
        .write_case(dir.path())
        .unwrap();
    std::fs::create_dir(dir.path().join("0")).unwrap();
    let case = Case::open(dir.path()).unwrap();
    (dir, case)
}

const PATCHES: [&str; 6] = ["xMin", "xMax", "yMin", "yMax", "zMin", "zMax"];

fn write(dir: &TempDir, name: &str, class: &str, internal: &str, patches: &[(&str, &str)]) {
    let mut text = format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class {class};\n    object {name};\n}}\n\
         dimensions [0 0 0 0 0 0 0];\ninternalField {internal};\nboundaryField\n{{\n"
    );
    for p in PATCHES {
        let body = patches
            .iter()
            .find(|(n, _)| *n == p)
            .map_or("type zeroGradient;", |(_, b)| *b);
        let _ = writeln!(text, "    {p} {{ {body} }}");
    }
    text.push_str("}\n");
    std::fs::write(dir.path().join("0").join(name), text).unwrap();
}

/// A scalar field `f(cell centre)`.
fn write_scalar(dir: &TempDir, case: &Case, name: &str, f: impl Fn(Vec3) -> f64) {
    let values: Vec<String> = case
        .geometry()
        .cell_centres
        .iter()
        .map(|&c| f(c).to_string())
        .collect();
    let internal = format!(
        "nonuniform List<scalar> {}({})",
        values.len(),
        values.join(" ")
    );
    write(
        dir,
        name,
        "volScalarField",
        &internal,
        &[("xMax", "type fixedValue; value uniform 5;")],
    );
}

#[test]
fn a_point_is_found_in_the_cell_that_holds_it() {
    let (_dir, case) = case();
    let cell = case.locate(Vec3::new(2.5, 0.5, 0.5)).unwrap();
    let centre = case.geometry().cell_centres[cell];
    assert_eq!((centre.x, centre.y), (2.5, 0.5));
    assert_eq!(case.locate(Vec3::new(4.5, 0.5, 0.5)), None);
    assert_eq!(case.locate(Vec3::new(2.0, 0.5, 2.0)), None);
}

#[test]
fn a_probe_reads_the_cell_around_the_point() {
    let (dir, case) = case();
    write_scalar(&dir, &case, "T", |c| 10.0 * c.x + c.y);
    let s = case.probe("0", "T", Vec3::new(3.2, 1.7, 0.4)).unwrap();
    assert_eq!(s.value, vec![10.0 * 3.5 + 1.5]);
    let outside = case.probe("0", "T", Vec3::new(9.0, 0.0, 0.0)).unwrap();
    assert!(outside.cell.is_none() && outside.value.is_empty());
}

#[test]
fn a_line_runs_through_the_cells_it_crosses() {
    let (dir, case) = case();
    write_scalar(&dir, &case, "T", |c| c.x);
    let (values, samples) = case
        .sample_line(
            "0",
            "T",
            Vec3::new(0.25, 1.0, 0.5),
            Vec3::new(3.75, 1.0, 0.5),
            4,
        )
        .unwrap();
    let got: Vec<f64> = samples.iter().map(|s| s.value[0]).collect();
    assert_eq!(got, vec![0.5, 1.5, 2.5, 3.5]);
    assert!((samples[3].distance - 3.5).abs() < 1e-12);
    let csv = line_csv("T", &values, &samples);
    assert!(csv.starts_with("distance,x,y,z,cell,T\n"));
    assert_eq!(csv.lines().count(), 5);
    assert!(
        case.sample_line(
            "0",
            "T",
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            0
        )
        .is_err()
    );
}

#[test]
fn a_patch_is_averaged_by_area() {
    let (dir, case) = case();
    write_scalar(&dir, &case, "T", |c| c.x);
    // The outlet holds its own value; the inlet has none, so it reads its cells.
    let outlet = case.patch_stats("0", "T", "xMax").unwrap();
    assert_eq!(
        (
            outlet.faces,
            outlet.area,
            outlet.integral[0],
            outlet.average[0]
        ),
        (2, 2.0, 10.0, 5.0)
    );
    let inlet = case.patch_stats("0", "T", "xMin").unwrap();
    assert_eq!(
        (inlet.min[0], inlet.max[0], inlet.average[0]),
        (0.5, 0.5, 0.5)
    );
}

#[test]
fn the_volume_is_averaged_by_cell_volume() {
    let (dir, case) = case();
    write_scalar(&dir, &case, "T", |c| c.x);
    let v = case.volume_stats("0", "T").unwrap();
    assert_eq!(
        (v.cells, v.volume, v.integral[0], v.average[0]),
        (8, 8.0, 16.0, 2.0)
    );
    assert_eq!((v.min[0], v.max[0]), (0.5, 3.5));
}

#[test]
fn a_vector_field_has_a_value_per_component() {
    let (dir, case) = case();
    write(
        &dir,
        "U",
        "volVectorField",
        "uniform (1 2 3)",
        &[("xMax", "type fixedValue; value uniform (4 5 6);")],
    );
    let v = case.volume_stats("0", "U").unwrap();
    assert_eq!(v.average, vec![1.0, 2.0, 3.0]);
    let p = case.patch_stats("0", "U", "xMax").unwrap();
    assert_eq!(p.integral, vec![8.0, 10.0, 12.0]);
    assert!(matches!(case.field("0", "U").unwrap(), FieldValues::Vector(v) if v.len() == 8));
}

#[test]
fn the_flux_through_a_patch_is_the_sum_of_its_faces() {
    let (dir, case) = case();
    let n = case.mesh().n_internal_faces();
    write(
        &dir,
        "phi",
        "surfaceScalarField",
        &format!("uniform 0 /* {n} */"),
        &[(
            "xMax",
            "type calculated; value nonuniform List<scalar> 2(0.5 0.25);",
        )],
    );
    assert_eq!(case.flux("0", "xMax").unwrap(), 0.75);
}

#[test]
fn names_that_are_not_there_are_refused_with_what_is() {
    let (dir, case) = case();
    write_scalar(&dir, &case, "T", |c| c.x);
    let e = case.patch_stats("0", "T", "inlet").unwrap_err().to_string();
    assert!(e.contains("no patch 'inlet'") && e.contains("xMax"), "{e}");
    assert!(case.field("0", "p").is_err());
    assert!(case.field("../0", "T").is_err());
    assert!(case.field("0", "../T").is_err());
    assert!(case.field("7", "T").is_err());
    assert_eq!(case.field_names("0").unwrap(), vec!["T".to_owned()]);
}

#[test]
fn a_report_sums_the_domain_and_each_patch_from_one_read() {
    let (dir, case) = case();
    write_scalar(&dir, &case, "T", |c| c.x);
    let n = case.mesh().n_internal_faces();
    write(
        &dir,
        "phi",
        "surfaceScalarField",
        &format!("uniform 0 /* {n} */"),
        &[(
            "xMax",
            "type calculated; value nonuniform List<scalar> 2(0.5 0.25);",
        )],
    );
    let r = case.report("0", "T").unwrap();
    assert_eq!(r.volume.average, vec![2.0]);
    let names: Vec<&str> = r.patches.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, PATCHES);
    let outlet = r.patches.iter().find(|p| p.name == "xMax").unwrap();
    assert_eq!((outlet.stats.average[0], outlet.flux), (5.0, Some(0.75)));
    // The other patches have no `value` for phi: no flux, but still reported.
    let inlet = r.patches.iter().find(|p| p.name == "xMin").unwrap();
    assert_eq!((inlet.stats.average[0], inlet.flux), (0.5, None));
}

#[test]
fn the_bounds_are_the_box_the_mesh_fills() {
    let (_dir, case) = case();
    let b = case.bounds();
    assert_eq!(
        (b.min, b.max),
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 2.0, 1.0))
    );
}
