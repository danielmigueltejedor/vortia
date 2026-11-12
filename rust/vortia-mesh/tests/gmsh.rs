//! `gmshToFoam` against OpenFOAM v1912: the same `.msh`, the same mesh.
//!
//! Every file in `tests/gmsh/` is a small mesh written for these tests, one
//! cell shape or one feature each, in the three Gmsh file formats. The
//! expectations below are what OpenFOAM's `gmshToFoam` wrote for the same
//! files (all of points, faces, owner, neighbour, boundary, cellZones and
//! faceZones were compared when the importer was built, as numbers; a
//! few coordinates of a larger mesh differ in the last bit because OpenFOAM
//! reads numbers through a long double).

use std::path::PathBuf;

use vortia_foamfile::RawPolyMesh;
use vortia_mesh::gmsh::gmsh_to_foam;

fn text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("gmsh")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn mesh(name: &str) -> RawPolyMesh {
    gmsh_to_foam(&text(name), false).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The volume of a one-cell mesh from its faces as they point out of the
/// cell: negative when the cell is inside out.
fn volume(m: &RawPolyMesh) -> f64 {
    let mut total = 0.0;
    for i in 0..m.faces.len() {
        let pts: Vec<_> = m
            .faces
            .face(i)
            .iter()
            .filter_map(|&p| m.points.get(p).copied())
            .collect();
        let Some(&a) = pts.first() else { continue };
        for k in 1..pts.len().saturating_sub(1) {
            let (b, c) = (pts[k], pts[k + 1]);
            let (u, v) = (
                (b.x - a.x, b.y - a.y, b.z - a.z),
                (c.x - a.x, c.y - a.y, c.z - a.z),
            );
            let n = (
                u.1 * v.2 - u.2 * v.1,
                u.2 * v.0 - u.0 * v.2,
                u.0 * v.1 - u.1 * v.0,
            );
            total += (a.x * n.0 + a.y * n.1 + a.z * n.2) / 6.0;
        }
    }
    total
}

fn patches(m: &RawPolyMesh) -> Vec<(&str, usize, usize)> {
    m.patches
        .iter()
        .map(|p| (p.name.as_str(), p.start_face, p.n_faces))
        .collect()
}

#[test]
fn a_hexahedron_in_every_file_format_is_the_same_mesh() {
    let v2 = mesh("hex.msh");
    assert_eq!(v2.points.len(), 8);
    assert_eq!(v2.faces.len(), 6);
    assert!(v2.neighbour.is_empty());
    assert_eq!(
        patches(&v2),
        [("inlet", 0, 1), ("outlet", 1, 1), ("wall", 2, 4)]
    );
    assert_eq!(v2.cell_zones.len(), 1);
    assert_eq!(v2.cell_zones[0].name, "fluid");
    assert_eq!(v2.cell_zones[0].cells, [0]);
    assert!(v2.face_zones.is_empty());
    for old in ["hex1.msh", "hex4.msh"] {
        let m = mesh(old);
        assert_eq!(m.points, v2.points, "{old}");
        assert_eq!(m.faces, v2.faces, "{old}");
        assert_eq!(m.owner, v2.owner, "{old}");
        assert_eq!(
            patches(&m),
            patches(&v2)
                .as_slice()
                .iter()
                .map(|&(n, s, c)| {
                    // The first format has no physical names: patches are numbered.
                    let numbered = ["patch0", "patch1", "patch2"];
                    let at = ["inlet", "outlet", "wall"]
                        .iter()
                        .position(|&w| w == n)
                        .unwrap_or(0);
                    (if old == "hex1.msh" { numbered[at] } else { n }, s, c)
                })
                .collect::<Vec<_>>(),
            "{old}"
        );
    }
}

#[test]
fn inside_out_cells_are_turned_round_unless_kept() {
    // Gmsh wrote this hexahedron with the vertices the wrong way round;
    // OpenFOAM turns it round, and the mesh comes out as the right-way one.
    let fixed = gmsh_to_foam(&text("hexin.msh"), false).unwrap();
    let kept = gmsh_to_foam(&text("hexin.msh"), true).unwrap();
    assert!(volume(&fixed) > 0.0, "turned round");
    assert!(volume(&kept) < 0.0, "kept as written");
    assert_eq!(fixed.faces, mesh("hex.msh").faces);
    // A right-way cell is the same either way.
    assert_eq!(
        gmsh_to_foam(&text("hex.msh"), true).unwrap().faces,
        mesh("hex.msh").faces
    );
    // A prism is "turned round" by swapping its second and third vertices
    // only, as OpenFOAM does, and kept as written otherwise.
    let fixed = gmsh_to_foam(&text("prismin.msh"), false).unwrap();
    let kept = gmsh_to_foam(&text("prismin.msh"), true).unwrap();
    assert_ne!(fixed.faces, kept.faces);
    assert!(volume(&kept) < 0.0);
    assert_eq!(
        gmsh_to_foam(&text("prism.msh"), true).unwrap().faces,
        mesh("prism.msh").faces
    );
}

#[test]
fn every_cell_shape_meshes() {
    for (file, faces, points) in [("prism.msh", 5, 6), ("pyr.msh", 5, 5), ("tet.msh", 4, 4)] {
        let m = mesh(file);
        assert_eq!((m.faces.len(), m.points.len()), (faces, points), "{file}");
        assert_eq!(m.owner.len(), faces);
    }
    // The base of the pyramid is the inlet, the rest the walls.
    assert_eq!(patches(&mesh("pyr.msh")), [("inlet", 0, 1), ("wall", 1, 4)]);
}

#[test]
fn patches_take_the_physical_names_or_a_number() {
    let m = mesh("hexnames.msh");
    // A name's characters OpenFOAM cannot keep in a word are dropped, and a
    // name with a space in it is not read at all, as in OpenFOAM.
    assert_eq!(
        patches(&m),
        [("patch0", 0, 1), ("outlet", 1, 1), ("wall", 2, 4)]
    );
    let m = mesh("hexnonames.msh");
    assert_eq!(
        patches(&m),
        [("patch0", 0, 1), ("patch1", 1, 1), ("patch2", 2, 4)]
    );
    assert_eq!(m.cell_zones[0].name, "cellZone_0");
}

#[test]
fn two_cells_share_a_face_that_becomes_a_face_zone() {
    let m = mesh("two.msh");
    assert_eq!(
        (m.points.len(), m.faces.len(), m.neighbour.len()),
        (12, 11, 1)
    );
    assert_eq!(m.neighbour, [1]);
    // The empty patch for the shared face keeps its place, like OpenFOAM's;
    // the faces nothing named go to the default patch, last.
    assert_eq!(
        patches(&m),
        [
            ("bottom", 1, 2),
            ("side", 3, 4),
            ("top", 7, 1),
            ("mid", 8, 0),
            ("defaultFaces", 8, 3)
        ]
    );
    let zones: Vec<_> = m
        .cell_zones
        .iter()
        .map(|z| (z.name.as_str(), &z.cells[..]))
        .collect();
    assert_eq!(zones, [("left", &[0][..]), ("right", &[1][..])]);
    assert_eq!(m.face_zones.len(), 1);
    assert_eq!(m.face_zones[0].name, "mid");
    assert_eq!(m.face_zones[0].faces, [0]);
    assert_eq!(m.face_zones[0].flip, [true]);
}

#[test]
fn a_mesh_written_to_a_case_reads_back() {
    let m = mesh("two.msh");
    let dir = std::env::temp_dir().join(format!("vortia-gmsh-{}", std::process::id()));
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
    for (what, text) in [
        ("empty", ""),
        ("no nodes", "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"),
        ("binary", "$MeshFormat\n2.2 1 8\n$EndMeshFormat\n"),
        ("not gmsh", "hello world\n"),
    ] {
        assert!(gmsh_to_foam(text, false).is_err(), "{what}");
    }
}

/// Mangled files must be refused or meshed, never a panic or a huge
/// allocation: every prefix of each file, and each number replaced in turn
/// by values a hostile file would use.
#[test]
fn hostile_files_never_panic() {
    for name in ["two.msh", "hex1.msh", "hex4.msh", "prism.msh", "pyr.msh"] {
        let original = text(name);
        let lines: Vec<&str> = original.lines().collect();
        for n in 0..=lines.len() {
            let cut = lines[..n].join("\n");
            let _ = gmsh_to_foam(&cut, false);
        }
        for n in (0..original.len()).step_by(7) {
            if original.is_char_boundary(n) {
                let _ = gmsh_to_foam(&original[..n], false);
            }
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
            if token.trim().parse::<f64>().is_err() {
                continue;
            }
            for bad in [
                "-1",
                "0",
                "99999999999",
                "2147483647",
                "nan",
                "1e400",
                "-0",
                "7",
            ] {
                let tail = tokens.get(k + 1).map_or(original.len(), |(next, _)| *next);
                let mut mangled = String::with_capacity(original.len() + bad.len());
                mangled.push_str(&original[..*start]);
                mangled.push_str(bad);
                mangled.push_str(if token.ends_with('\n') { "\n" } else { " " });
                mangled.push_str(&original[tail..]);
                for keep in [false, true] {
                    let _ = gmsh_to_foam(&mangled, keep);
                }
            }
        }
    }
}
