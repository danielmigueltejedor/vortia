//! `fluentMeshToFoam` against OpenFOAM v1912: the same `.msh`, the same mesh.
//!
//! Every file in `tests/fluent/` was written for these tests: one cell shape
//! or one feature each, and a hexahedral box of 207 cells. The expectations
//! below are what OpenFOAM's `fluentMeshToFoam` made of the same files (all of
//! points, faces, owner, neighbour, boundary, cellZones and faceZones were
//! compared when the importer was built, as numbers; so were four larger
//! meshes of up to 15 600 cells in every shape).

use std::path::PathBuf;

use vortia_foamfile::RawPolyMesh;
use vortia_mesh::fluent::{Options, fluent_to_foam};

fn text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fluent")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn mesh_with(name: &str, options: Options) -> RawPolyMesh {
    fluent_to_foam(&text(name), options).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn mesh(name: &str) -> RawPolyMesh {
    mesh_with(name, Options::default())
}

fn patches(m: &RawPolyMesh) -> Vec<(&str, &str, usize, usize)> {
    m.patches
        .iter()
        .map(|p| (p.name.as_str(), p.kind.as_str(), p.start_face, p.n_faces))
        .collect()
}

#[test]
fn a_box_of_hexahedra_keeps_its_patches() {
    let m = mesh("box.msh");
    assert_eq!(
        (m.points.len(), m.faces.len(), m.neighbour.len()),
        (360, 750, 492)
    );
    assert_eq!(m.owner.iter().max(), Some(&206));
    assert_eq!(
        patches(&m),
        [
            ("left", "patch", 492, 27),
            ("right", "patch", 519, 12),
            ("bottom", "wall", 531, 33),
            ("top", "wall", 564, 48),
            ("back", "patch", 612, 69),
            ("front", "patch", 681, 69),
        ]
    );
    // The wall patches are also in the `wall` group; the interior zone is no patch.
    assert_eq!(m.patches.iter().filter(|p| p.groups == ["wall"]).count(), 2);
}

#[test]
fn every_3d_cell_shape_is_recognised() {
    for (file, faces, points) in [
        ("tet.msh", 4, 4),
        ("pyr.msh", 5, 5),
        ("prism.msh", 5, 6),
        ("hex.msh", 6, 8),
    ] {
        let m = mesh(file);
        assert_eq!((m.faces.len(), m.points.len()), (faces, points), "{file}");
        assert!(m.neighbour.is_empty(), "{file}");
    }
    assert_eq!(
        patches(&mesh("hex.msh")),
        [
            ("inlet", "patch", 0, 1),
            ("outlet", "patch", 1, 1),
            ("wall", "patch", 2, 4)
        ]
    );
    assert_eq!(
        patches(&mesh("pyr.msh")),
        [("inlet", "patch", 0, 1), ("wall", "patch", 1, 4)]
    );
}

#[test]
fn a_cell_section_without_its_element_type_is_tetrahedra() {
    // Tgrid leaves the element type out of the cell group.
    let tgrid = mesh("tet_tgrid.msh");
    let plain = mesh("tet.msh");
    assert_eq!(tgrid.faces, plain.faces);
    assert_eq!(tgrid.points, plain.points);
    assert_eq!(
        patches(&tgrid),
        [("inlet", "patch", 0, 1), ("wall", "patch", 1, 3)]
    );
}

#[test]
fn sections_it_does_not_know_are_skipped() {
    let m = mesh("tet_unknown.msh");
    assert_eq!(m.faces, mesh("tet.msh").faces);
}

#[test]
fn a_2d_mesh_is_extruded_with_an_empty_front_and_back() {
    let quads = mesh("quad2d.msh");
    assert_eq!(
        (quads.points.len(), quads.faces.len(), quads.neighbour.len()),
        (24, 29, 7)
    );
    assert_eq!(
        patches(&quads),
        [
            ("left", "patch", 7, 2),
            ("right", "patch", 9, 2),
            ("bottom", "wall", 11, 3),
            ("top", "symmetry", 14, 3),
            ("frontAndBackPlanes", "empty", 17, 12),
        ]
    );
    // 2 % of the diagonal of the box, half of it each way.
    let diagonal = (3.0f64 * 3.0 + 1.0 * 1.0).sqrt();
    let z: Vec<f64> = quads.points.iter().map(|p| p.z).collect();
    assert!(z.iter().all(|&z| (z.abs() - 0.01 * diagonal).abs() < 1e-12));
    let triangles = mesh("tri2d.msh");
    assert_eq!(triangles.owner.iter().max(), Some(&11));
    assert_eq!(triangles.neighbour.len(), 13);
    assert_eq!(
        patches(&triangles).last(),
        Some(&("frontAndBackPlanes", "empty", 23, 24))
    );
}

#[test]
fn zones_of_an_inside_type_that_are_outside_make_a_default_wall() {
    // `axis` is a symmetry, `interface` is declared inside but is a boundary.
    let m = mesh("quad2d_odd.msh");
    assert_eq!(
        patches(&m),
        [
            ("left", "symmetry", 17, 3),
            ("right", "patch", 20, 3),
            ("bottom", "wall", 23, 4),
            ("frontAndBackPlanes", "empty", 27, 24),
            ("default_wall", "wall", 51, 4),
        ]
    );
}

#[test]
fn scale_and_thickness_apply_in_that_order() {
    let m = mesh_with(
        "quad2d.msh",
        Options {
            scale: 2.0,
            thickness: Some(0.5),
            write_zones: false,
        },
    );
    assert!(
        m.points.iter().all(|p| (p.z.abs() - 0.5).abs() < 1e-12),
        "z is ±0.25 × 2"
    );
    assert!(m.points.iter().any(|p| (p.x - 6.0).abs() < 1e-12));
}

#[test]
fn cell_zones_are_written_when_asked_for() {
    let none = mesh("zones.msh");
    assert!(none.cell_zones.is_empty() && none.face_zones.is_empty());
    let m = mesh_with(
        "zones.msh",
        Options {
            write_zones: true,
            ..Options::default()
        },
    );
    let zones: Vec<_> = m
        .cell_zones
        .iter()
        .map(|z| (z.name.as_str(), z.cells.len(), z.cells.first().copied()))
        .collect();
    assert_eq!(
        zones,
        [
            ("first", 12, Some(0)),
            ("second", 12, Some(12)),
            ("third", 12, Some(24))
        ]
    );
    // The faces both of whose cells are in the zone, never flipped.
    assert_eq!(m.face_zones.len(), 3);
    assert!(m.face_zones.iter().all(|z| z.flip.iter().all(|&f| !f)));
    assert_eq!(
        m.face_zones
            .iter()
            .map(|z| z.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
    assert_eq!(
        patches(&m),
        [
            ("inlet", "patch", 68, 4),
            ("outlet", "patch", 72, 4),
            ("walls", "wall", 76, 36),
            ("sides", "patch", 112, 36),
        ]
    );
}

#[test]
fn a_mesh_written_to_a_case_reads_back() {
    let m = mesh("quad2d.msh");
    let dir = std::env::temp_dir().join(format!("vortia-fluent-{}", std::process::id()));
    m.write_case(&dir).unwrap();
    let back = RawPolyMesh::read_case(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(back.points, m.points);
    assert_eq!(back.faces, m.faces);
    assert_eq!(back.owner, m.owner);
    assert_eq!(back.neighbour, m.neighbour);
    assert_eq!(back.patches.len(), m.patches.len());
}

#[test]
fn files_that_are_not_meshes_are_errors() {
    let tet = text("tet.msh");
    for (what, text) in [
        ("empty", String::new()),
        ("not fluent", "hello world\n".into()),
        ("no cells", tet.replace("(12 (0 1 1 0 0))", "")),
        (
            "unknown zone type",
            tet.replace("(39 (11 pressure-outlet wall)", "(39 (11 weird-type wall)"),
        ),
        (
            "a face that is not the cell's",
            tet.replace("3 2 3 1 1 0", "3 2 3 1 5 0"),
        ),
        ("a swapped face", tet.replace("3 2 3 1 1 0", "3 2 3 1 0 1")),
        (
            "a hexahedron with a tetrahedron's faces",
            tet.replace("(12 (1 1 1 1 0)(\n 2)", "(12 (1 1 1 1 0)(\n 4)"),
        ),
        (
            "points beyond the declared",
            tet.replace("(10 (1 1 4 1 3)", "(10 (1 1 9 1 3)"),
        ),
        (
            "a huge count",
            tet.replace("(10 (0 1 4 0 3))", "(10 (0 1 7fffffff 0 3))"),
        ),
    ] {
        assert!(fluent_to_foam(&text, Options::default()).is_err(), "{what}");
    }
}

/// Mangled files must be refused or meshed, never a panic or a huge
/// allocation: every prefix of each file, and each number replaced in turn
/// by values a hostile file would use.
#[test]
fn hostile_files_never_panic() {
    for name in [
        "tet.msh",
        "hex.msh",
        "prism.msh",
        "pyr.msh",
        "quad2d.msh",
        "tri2d.msh",
    ] {
        let original = text(name);
        let lines: Vec<&str> = original.lines().collect();
        for n in 0..=lines.len() {
            let _ = fluent_to_foam(&lines[..n].join("\n"), Options::default());
        }
        let tokens: Vec<(usize, &str)> = original
            .split_inclusive([' ', '\n'])
            .scan(0, |at, t| {
                let start = *at;
                *at += t.len();
                Some((start, t))
            })
            .collect();
        for (k, (start, token)) in tokens.iter().enumerate() {
            let bare = token.trim().trim_matches(|c| c == '(' || c == ')');
            if bare.is_empty()
                || bare.parse::<f64>().is_err() && i64::from_str_radix(bare, 16).is_err()
            {
                continue;
            }
            for bad in [
                "-1",
                "0",
                "99999999999",
                "7fffffff",
                "nan",
                "1e400",
                "1",
                "9",
            ] {
                let tail = tokens.get(k + 1).map_or(original.len(), |(next, _)| *next);
                let mut mangled = String::with_capacity(original.len() + bad.len());
                mangled.push_str(&original[..*start]);
                mangled.push_str(bad);
                mangled.push_str(if token.ends_with('\n') { "\n" } else { " " });
                mangled.push_str(&original[tail..]);
                let _ = fluent_to_foam(&mangled, Options::default());
            }
        }
    }
}
