//! `blockMesh` against OpenFOAM v1912: the same dictionary, the same mesh.
//!
//! `tests/blockmesh/tiny.dict` is two blocks, one graded and one rotated, and
//! its expected numbers are the files OpenFOAM's `blockMesh` wrote for it
//! (points to the last digit). `nozzle.dict` is the nozzle practice case's
//! four blocks, two of them degenerate (prisms) and merged by distance; its
//! counts and patches are OpenFOAM's (the whole mesh was compared to the last
//! digit when the engine was built).

use std::path::PathBuf;

use vortia_foamfile::{FoamFile, RawPolyMesh};
use vortia_mesh::blockmesh::block_mesh;

fn here(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("blockmesh")
        .join(name)
}

fn mesh_of_file(name: &str) -> Result<RawPolyMesh, String> {
    let (_, dict) = FoamFile::open(here(name))
        .and_then(|f| f.dictionary())
        .map_err(|e| e.to_string())?;
    block_mesh(&dict).map_err(|e| e.to_string())
}

fn mesh_of_text(tag: &str, text: &str) -> Result<RawPolyMesh, String> {
    let dir = std::env::temp_dir().join(format!("vortia-blockmesh-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("blockMeshDict");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let result = FoamFile::open(&path)
        .and_then(|f| f.dictionary())
        .map_err(|e| e.to_string())
        .and_then(|(_, d)| block_mesh(&d).map_err(|e| e.to_string()));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn two_blocks_give_openfoams_mesh_exactly() {
    let mesh = mesh_of_file("tiny.dict").unwrap();

    let points: [[f64; 3]; 20] = [
        [0.0, 0.0, 0.0],
        [0.16666666666666666, 0.0, 0.0],
        [0.5, 0.0, 0.0],
        [0.0, 0.5, 0.0],
        [0.16666666666666666, 0.5, 0.0],
        [0.5, 0.5, 0.0],
        [0.0, 0.0, 0.5],
        [0.16666666666666666, 0.0, 0.500_000_000_000_000_1],
        [0.5, 0.0, 0.5],
        [0.0, 0.5, 0.5],
        [0.16666666666666666, 0.5, 0.5],
        [0.5, 0.5, 0.5],
        [1.0, 0.5, 0.0],
        [0.75, 0.5, 0.0],
        [1.0, 0.0, 0.0],
        [0.75, 0.0, 0.0],
        [1.0, 0.5, 0.5],
        [0.75, 0.5, 0.5],
        [1.0, 0.0, 0.5],
        [0.75, 0.0, 0.5],
    ];
    assert_eq!(mesh.points.len(), points.len());
    for (got, want) in mesh.points.iter().zip(points) {
        assert_eq!([got.x, got.y, got.z], want);
    }

    let faces: [&[usize]; 21] = [
        &[1, 4, 10, 7],
        &[2, 5, 11, 8],
        &[13, 15, 19, 17],
        &[0, 6, 9, 3],
        &[12, 16, 18, 14],
        &[0, 1, 7, 6],
        &[1, 2, 8, 7],
        &[3, 9, 10, 4],
        &[4, 10, 11, 5],
        &[0, 3, 4, 1],
        &[1, 4, 5, 2],
        &[6, 7, 10, 9],
        &[7, 8, 11, 10],
        &[12, 13, 17, 16],
        &[13, 5, 11, 17],
        &[14, 18, 19, 15],
        &[15, 19, 8, 2],
        &[12, 14, 15, 13],
        &[13, 15, 2, 5],
        &[16, 17, 19, 18],
        &[17, 11, 8, 19],
    ];
    assert_eq!(mesh.faces.len(), faces.len());
    for (i, want) in faces.iter().enumerate() {
        assert_eq!(mesh.faces.face(i), *want, "face {i}");
    }
    assert_eq!(
        mesh.owner,
        [
            0, 1, 2, 0, 2, 0, 1, 0, 1, 0, 1, 0, 1, 2, 3, 2, 3, 2, 3, 2, 3
        ]
    );
    assert_eq!(mesh.neighbour, [1, 3, 3]);

    let patches: Vec<_> = mesh
        .patches
        .iter()
        .map(|p| (p.name.as_str(), p.kind.as_str(), p.start_face, p.n_faces))
        .collect();
    assert_eq!(
        patches,
        [
            ("inlet", "patch", 3, 1),
            ("outlet", "patch", 4, 1),
            ("walls", "wall", 5, 4),
            ("defaultFaces", "empty", 9, 12),
        ]
    );
    assert!(mesh.patches[0].groups.is_empty());
    assert_eq!(mesh.patches[2].groups, ["wall"]);
}

#[test]
fn the_nozzle_with_degenerate_blocks_has_openfoams_counts_and_patches() {
    let mesh = mesh_of_file("nozzle.dict").unwrap();
    assert_eq!(mesh.points.len(), 15276);
    assert_eq!(mesh.faces.len(), 30100);
    assert_eq!(mesh.neighbour.len(), 14825);
    assert_eq!(mesh.owner.len(), 30100);
    let n_cells = mesh.owner.iter().max().map(|m| m + 1);
    assert_eq!(n_cells, Some(7500));
    let patches: Vec<_> = mesh
        .patches
        .iter()
        .map(|p| (p.name.as_str(), p.start_face, p.n_faces))
        .collect();
    assert_eq!(
        patches,
        [
            ("axis", 14825, 0),
            ("inlet", 14825, 30),
            ("walls", 14855, 70),
            ("atmosphere", 14925, 175),
            ("front", 15100, 7500),
            ("back", 22600, 7500),
        ]
    );
}

const BOX: &str = "FoamFile { version 2.0; format ascii; class dictionary; object blockMeshDict; }
vertices ( (0 0 0) (1 0 0) (1 1 0) (0 1 0) (0 0 1) (1 0 1) (1 1 1) (0 1 1) );
edges ();
boundary ();
";

#[test]
fn a_dictionary_without_a_default_patch_name_gets_default_faces() {
    let text = format!("{BOX}blocks ( hex (0 1 2 3 4 5 6 7) (2 2 2) simpleGrading (1 1 1) );");
    let mesh = mesh_of_text("default", &text).unwrap();
    assert_eq!(mesh.neighbour.len(), 12);
    assert_eq!(mesh.patches.len(), 1);
    assert_eq!(mesh.patches[0].name, "defaultFaces");
    assert_eq!(mesh.patches[0].n_faces, 24);
}

#[test]
fn what_is_not_supported_yet_is_refused_with_its_name() {
    for (tag, extra, word) in [
        (
            "project",
            "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (1 1 1) ); edges ( project 0 1 (surface) );",
            "project",
        ),
        (
            "tet",
            "blocks ( tet (0 1 2 4) (1 1 1) simpleGrading (1 1 1) );",
            "tet",
        ),
    ] {
        let text = BOX.replace("edges ();", "") + extra;
        let err = mesh_of_text(tag, &text).unwrap_err();
        assert!(err.contains(word), "{tag}: {err}");
    }
}

#[test]
fn hostile_dictionaries_are_errors_not_panics() {
    let cases = [
        // No blocks, bad vertex labels, zero and negative divisions, NaN and
        // zero gradings, and a request for far more cells than allowed.
        "blocks ();",
        "blocks ( hex (0 1 2 3 4 5 6 99) (1 1 1) simpleGrading (1 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (0 1 1) simpleGrading (1 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (-1 1 1) simpleGrading (1 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (0 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (nan 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (100000 100000 100000) simpleGrading (1 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (1 1) );",
        "blocks ( hex (0 0 0 0 0 0 0 0) (2 2 2) simpleGrading (1 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) edgeGrading (1 1 1) );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (1 1 1) ); boundary ( a { type patch; faces ( (0 1 2) ); } );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (1 1 1) ); boundary ( a { type patch; faces ( (0 1 2 9) ); } );",
        "blocks ( hex (0 1 2 3 4 5 6 7) (2 2 2) simpleGrading (((1 1 1) (1 1 -1)) 1 1) );",
        "blocks 3;",
    ];
    for (i, tail) in cases.iter().enumerate() {
        let text = BOX.replace("boundary ();", "") + tail;
        // Either way: a result or an error, never a panic.
        let _ = mesh_of_text(&format!("hostile{i}"), &text);
    }
    // The ones that cannot possibly be a mesh.
    for tail in [&cases[0], &cases[1], &cases[2], &cases[3], &cases[6]] {
        let text = BOX.replace("boundary ();", "") + tail;
        assert!(mesh_of_text("bad", &text).is_err(), "{tail}");
    }
}

/// Counts, the sums of the coordinates in file order, and two points of the
/// mesh OpenFOAM v1912's `blockMesh` wrote for a dictionary with curved edges.
/// The points matched OpenFOAM's to the last digit, so the sums do too.
fn check_curved(
    name: &str,
    n_points: usize,
    n_faces: usize,
    n_internal: usize,
    sums: [f64; 3],
    spots: [(usize, [f64; 3]); 2],
) {
    let mesh = mesh_of_file(name).unwrap();
    assert_eq!(mesh.points.len(), n_points, "{name}: points");
    assert_eq!(mesh.faces.len(), n_faces, "{name}: faces");
    assert_eq!(mesh.neighbour.len(), n_internal, "{name}: internal faces");
    let sum = mesh
        .points
        .iter()
        .fold([0.0; 3], |a, p| [a[0] + p.x, a[1] + p.y, a[2] + p.z]);
    assert_eq!(sum, sums, "{name}: the sum of the points");
    for (i, want) in spots {
        let got = mesh.points[i];
        assert_eq!([got.x, got.y, got.z], want, "{name}: point {i}");
    }
}

#[test]
fn arcs_with_grading_match_openfoam_exactly() {
    check_curved(
        "arc1.dict",
        168,
        333,
        207,
        [135.67335464947664, 160.0929268846488, 22.345584412271606],
        [
            (
                56,
                [0.7071067811865481, 0.7071067811865479, 0.06796227589829591],
            ),
            (163, [0.0, 1.3333333333333333, 0.3]),
        ],
    );
}

#[test]
fn every_kind_of_edge_on_two_blocks_matches_openfoam_exactly() {
    // spline (with its tangents), BSpline, polyLine, bezier, arc and line,
    // listed against and along the direction of the blocks' edges, on graded
    // edges.
    check_curved(
        "curve2.dict",
        160,
        306,
        180,
        [1.6863894776845916, 0.8023940215261227, 0.8579497050157822],
        [
            (
                53,
                [
                    0.006261292678333823,
                    0.006909047769324247,
                    0.007434384435267171,
                ],
            ),
            (
                155,
                [
                    0.012900182823493034,
                    0.010414311831927576,
                    0.010828623663855152,
                ],
            ),
        ],
    );
}

#[test]
fn an_arc_of_more_than_half_a_circle_with_edge_grading_matches_openfoam_exactly() {
    check_curved(
        "arc2.dict",
        180,
        356,
        220,
        [357.96878379367257, 270.41093345633686, 84.62445460092114],
        [
            (
                60,
                [0.9021628955538787, 0.9510814477769394, 0.44811002230083147],
            ),
            (175, [3.0, 2.5, 1.0]),
        ],
    );
}

#[test]
fn fast_merge_by_topology_gives_the_same_mesh_when_the_gradings_agree() {
    let by_distance = mesh_of_file("tiny.dict").unwrap();
    let text = std::fs::read_to_string(here("tiny.dict")).unwrap();
    let fast = mesh_of_text(
        "fast",
        &text.replace("scale 0.5;", "scale 0.5; fastMerge true;"),
    )
    .unwrap();
    assert_eq!(fast, by_distance);
}

#[test]
fn blocks_whose_gradings_disagree_on_a_shared_face_are_refused_by_fast_merge() {
    let text = "FoamFile { version 2.0; format ascii; class dictionary; object blockMeshDict; }
fastMerge true;
vertices ( (0 0 0) (1 0 0) (1 1 0) (0 1 0) (0 0 1) (1 0 1) (1 1 1) (0 1 1) (2 0 0) (2 1 0) (2 0 1) (2 1 1) );
blocks ( hex (0 1 2 3 4 5 6 7) (2 2 2) simpleGrading (1 1 3) hex (1 8 9 2 5 10 11 6) (2 2 2) simpleGrading (1 1 1) );
boundary ();";
    let err = mesh_of_text("grading", text).unwrap_err();
    assert!(err.contains("grading"), "{err}");
}

#[test]
fn hostile_edges_are_errors_not_panics() {
    let blocks = "blocks ( hex (0 1 2 3 4 5 6 7) (2 2 2) simpleGrading (1 1 1) );";
    for (i, edges) in [
        // The three points of an arc on a line.
        "arc 0 1 (0.5 0 0)",
        // A vertex that does not exist, a vertex joined to itself, a missing
        // middle point, a point of two numbers, a name for a vertex.
        "arc 0 99 (0.5 0.5 0)",
        "arc 0 0 (0.5 0.5 0)",
        "arc 0 1",
        "polyLine 0 1 ((0.5 0.5))",
        "arc a b (0.5 0.5 0)",
        // A line that goes nowhere, and one through coincident points.
        "polyLine 0 1 ()",
        "spline 0 1 ((0 0 0) (0 0 0))",
        "spline 0 1 ((nan 0 0))",
        "bezier 0 1 ()",
        "arc 0 1 (1e300 1e300 1e300)",
    ]
    .iter()
    .enumerate()
    {
        let text = format!(
            "{}{blocks} edges ( {edges} );",
            BOX.replace("edges ();", "")
        );
        let _ = mesh_of_text(&format!("edge{i}"), &text);
    }
    let text = format!(
        "{}{blocks} edges ( arc 0 1 (0.5 0 0) );",
        BOX.replace("edges ();", "")
    );
    assert!(mesh_of_text("online", &text).is_err());
    let text = format!(
        "{}{blocks} edges ( arc 0 99 (0.5 0.5 0) );",
        BOX.replace("edges ();", "")
    );
    assert!(mesh_of_text("novertex", &text).is_err());
}

const CUBE: &str = "FoamFile { version 2.0; format ascii; class dictionary; object blockMeshDict; }
vertices ( (0 0 0) (1 0 0) (1 1 0) (0 1 0) (0 0 1) (1 0 1) (1 1 1) (0 1 1) );
edges ();
boundary ();
";

fn collapsed(tag: &str, labels: &str, cells: &str) -> RawPolyMesh {
    let text = format!("{CUBE}blocks ( hex ({labels}) ({cells}) simpleGrading (1 1 1) );");
    mesh_of_text(tag, &text).unwrap()
}

/// A block whose vertices are repeated has cells of the shape the repeats
/// leave: a tetrahedron, a pyramid, a wedge, a prism or a tetWedge. One cell
/// of each, against the mesh OpenFOAM v1912 wrote.
#[test]
fn one_collapsed_cell_of_each_shape_matches_openfoam() {
    type Case<'a> = (&'a str, &'a str, &'a [[f64; 3]], &'a [&'a [usize]]);
    let cases: [Case<'_>; 5] = [
        (
            "tet",
            "0 0 0 0 4 5 6 6",
            &[[0., 0., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.]],
            &[&[0, 1, 3], &[0, 3, 2], &[0, 2, 1], &[3, 1, 2]],
        ),
        (
            "pyramid",
            "0 0 0 0 4 5 6 7",
            &[
                [0., 0., 0.],
                [0., 0., 1.],
                [1., 0., 1.],
                [0., 1., 1.],
                [1., 1., 1.],
            ],
            &[
                &[1, 3, 0],
                &[2, 0, 4],
                &[1, 0, 2],
                &[3, 4, 0],
                &[1, 2, 4, 3],
            ],
        ),
        (
            "wedge",
            "0 0 2 3 4 5 6 7",
            &[
                [0., 0., 0.],
                [0., 1., 0.],
                [1., 1., 0.],
                [0., 0., 1.],
                [1., 0., 1.],
                [0., 1., 1.],
                [1., 1., 1.],
            ],
            &[
                &[0, 3, 5, 1],
                &[0, 2, 6, 4],
                &[0, 4, 3],
                &[1, 5, 6, 2],
                &[0, 1, 2],
                &[3, 4, 6, 5],
            ],
        ),
        (
            "tetwedge",
            "0 0 2 2 4 5 6 6",
            &[
                [0., 0., 0.],
                [1., 1., 0.],
                [0., 0., 1.],
                [1., 0., 1.],
                [1., 1., 1.],
            ],
            &[&[2, 4, 1, 0], &[3, 0, 1, 4], &[2, 0, 3], &[2, 3, 4]],
        ),
        (
            "prism",
            "0 0 2 3 4 4 6 7",
            &[
                [0., 0., 0.],
                [0., 1., 0.],
                [1., 1., 0.],
                [0., 0., 1.],
                [0., 1., 1.],
                [1., 1., 1.],
            ],
            &[
                &[0, 3, 4, 1],
                &[0, 2, 5, 3],
                &[2, 1, 4, 5],
                &[0, 1, 2],
                &[3, 5, 4],
            ],
        ),
    ];
    for (name, labels, points, faces) in cases {
        let mesh = collapsed(name, labels, "1 1 1");
        let got: Vec<[f64; 3]> = mesh.points.iter().map(|p| [p.x, p.y, p.z]).collect();
        assert_eq!(got, points, "{name}: points");
        assert_eq!(mesh.faces.len(), faces.len(), "{name}: faces");
        for (i, want) in faces.iter().enumerate() {
            assert_eq!(mesh.faces.face(i), *want, "{name}: face {i}");
        }
        assert!(mesh.owner.iter().all(|&o| o == 0), "{name}: owner");
        assert!(mesh.neighbour.is_empty(), "{name}: neighbour");
    }
}

/// Twelve cells of each shape: counts and position-weighted sums of the face
/// vertices and of the owners, which fix the order of OpenFOAM's mesh.
#[test]
fn collapsed_blocks_of_several_cells_match_openfoam() {
    let cases = [
        ("tet", "0 0 0 0 4 5 6 6", 19, 40, 27202, 5001),
        ("pyramid", "0 0 0 0 4 5 6 7", 25, 46, 47570, 6899),
        ("wedge", "0 0 2 3 4 5 6 7", 33, 52, 85923, 7870),
        ("tetwedge", "0 0 2 2 4 5 6 6", 21, 40, 32829, 5001),
        ("prism", "0 0 2 3 4 4 6 7", 27, 46, 55006, 6449),
    ];
    for (name, labels, n_points, n_faces, face_sum, owner_sum) in cases {
        let mesh = collapsed(name, labels, "3 2 2");
        assert_eq!(mesh.points.len(), n_points, "{name}: points");
        assert_eq!(mesh.faces.len(), n_faces, "{name}: faces");
        assert_eq!(mesh.neighbour.len(), 20, "{name}: internal faces");
        let fs: usize = (0..mesh.faces.len())
            .map(|i| mesh.faces.face(i).iter().sum::<usize>() * (i + 1))
            .sum();
        assert_eq!(fs, face_sum, "{name}: faces");
        let os: usize = mesh
            .owner
            .iter()
            .enumerate()
            .map(|(i, o)| o * (i + 1))
            .sum();
        assert_eq!(os, owner_sum, "{name}: owners");
        let ns: usize = mesh
            .neighbour
            .iter()
            .enumerate()
            .map(|(i, o)| o * (i + 1))
            .sum();
        assert_eq!(ns, 1777, "{name}: neighbours");
    }
}

#[test]
fn a_block_turned_inside_out_or_flattened_is_refused() {
    // Its faces point inwards: the top and bottom swapped.
    let text = format!("{CUBE}blocks ( hex (4 5 6 7 0 1 2 3) (1 1 1) simpleGrading (1 1 1) );");
    let err = mesh_of_text("inside-out", &text).unwrap_err();
    assert!(err.contains("inside-out"), "{err}");
    // Its top is its bottom: some faces point out, the rest back in.
    let text = format!("{CUBE}blocks ( hex (0 1 2 3 0 1 2 3) (1 1 1) simpleGrading (1 1 1) );");
    let err = mesh_of_text("flat", &text).unwrap_err();
    assert!(err.contains("inward-pointing"), "{err}");
}

/// Vertices that carry a name stand for their numbers in blocks, edges and
/// patch faces, so a dictionary written with names makes the mesh its
/// numbered twin makes (which OpenFOAM v1912 confirmed, to the last digit,
/// for this very pair).
#[test]
fn named_vertices_make_the_mesh_of_their_numbers() {
    let named = mesh_of_file("named.dict").unwrap();
    let numbered = mesh_of_file("numbered.dict").unwrap();
    assert_eq!(named, numbered);
    assert_eq!((named.points.len(), named.faces.len()), (90, 148));
    assert_eq!(named.patches.len(), 4);
}

#[test]
fn names_may_come_from_namedvertices_and_namedblocks() {
    let head = "vertices ( (0 0 0) (1 0 0) (1 1 0) (0 1 0) (0 0 1) (1 0 1) (1 1 1) (0 1 1) );\n";
    let by_number = format!(
        "{head}blocks ( hex (0 1 2 3 4 5 6 7) (2 2 2) simpleGrading (1 1 1) );\n\
         boundary ( left {{ type patch; faces ( (0 4 7 3) ); }} );"
    );
    let by_name = format!(
        "namedVertices {{ v0 0; v1 1; }}\nnamedBlocks {{ four 4; }}\n{head}\
         blocks ( hex (v0 v1 2 3 four 5 6 7) (2 2 2) simpleGrading (1 1 1) );\n\
         boundary ( left {{ type patch; faces ( (v0 four 7 3) ); }} );"
    );
    // A block's own vertices may use `namedVertices` but not `namedBlocks`.
    let wrong = by_name.replace("(v0 v1 2 3 four 5 6 7)", "(v0 v1 2 3 4 5 6 7)");
    assert_eq!(
        mesh_of_text("by-name", &wrong).unwrap(),
        mesh_of_text("by-number", &by_number).unwrap()
    );
    assert!(mesh_of_text("block-name-for-a-vertex", &by_name).is_err());
}

/// The first meaning of a name stands, as in OpenFOAM, whose dictionary
/// ignores a later entry of the same name; a name nothing defines, and a
/// `name` without its point, are errors.
#[test]
fn a_repeated_name_keeps_its_first_vertex_and_an_undefined_one_is_refused() {
    let cube = |vertices: &str, block: &str| {
        format!("vertices ( {vertices} );\nblocks ( hex ({block}) (1 1 1) simpleGrading (1 1 1) );")
    };
    let cube_points = "name a (0 0 0) name b (1 0 0) name c (1 1 0) name d (0 1 0) \
                       name e (0 0 1) name f (1 0 1) name g (1 1 1) name h (0 1 1)";
    let ok = mesh_of_text("names-ok", &cube(cube_points, "a b c d e f g h")).unwrap();
    // `a` again, at a place that would turn the block inside out.
    let again = format!("{cube_points} name a (9 9 9)");
    assert_eq!(
        mesh_of_text("names-again", &cube(&again, "a b c d e f g h")).unwrap(),
        ok
    );
    for (tag, text) in [
        ("undefined", cube(cube_points, "a b c d e f g z")),
        ("no-point", cube("name a", "0 0 0 0 0 0 0 0")),
        ("no-name", cube("name", "0 0 0 0 0 0 0 0")),
        ("name-and-number", cube("name 3 (0 0 0)", "0 0 0 0 0 0 0 0")),
        (
            "projected",
            cube("project (0 0 0) (surface)", "0 0 0 0 0 0 0 0"),
        ),
    ] {
        assert!(mesh_of_text(tag, &text).is_err(), "{tag}");
    }
}

/// Three blocks side by side, the first and the last in zone `b`, the middle
/// one in zone `a` (checked against OpenFOAM v1912's `cellZones`).
const ZONE_ORDER: &str = "vertices ( (0 0 0) (1 0 0) (2 0 0) (3 0 0) (0 1 0) (1 1 0) (2 1 0) (3 1 0) \
    (0 0 1) (1 0 1) (2 0 1) (3 0 1) (0 1 1) (1 1 1) (2 1 1) (3 1 1) );\n\
    blocks (\n\
      hex (0 1 5 4 8 9 13 12) b (2 1 1) simpleGrading (1 1 1)\n\
      hex (1 2 6 5 9 10 14 13) a (3 1 1) simpleGrading (1 1 1)\n\
      hex (2 3 7 6 10 11 15 14) b (1 1 1) simpleGrading (1 1 1)\n\
    );";

/// A block that names a zone puts its cells in it. The zones, their order
/// (as their names first appear) and their cells are those of the
/// `cellZones` file OpenFOAM v1912 wrote for these dictionaries.
#[test]
fn cell_zones_hold_the_cells_of_the_blocks_that_name_them() {
    // Three blocks of 12, 8 and 16 cells: the first and the last in `hot`,
    // the middle one in no zone.
    let named = mesh_of_file("named.dict").unwrap();
    assert_eq!(named.cell_zones.len(), 1);
    let hot = named.cell_zones.first().unwrap();
    assert_eq!(hot.name, "hot");
    let expected: Vec<usize> = (0..12).chain(20..36).collect();
    assert_eq!(hot.cells, expected);
    // No zone, no file.
    assert!(mesh_of_file("tiny.dict").unwrap().cell_zones.is_empty());

    // Two zones, in the order of their first block, one of them split in two.
    let mesh = mesh_of_text("zones-order", ZONE_ORDER).unwrap();
    let zones: Vec<(&str, &[usize])> = mesh
        .cell_zones
        .iter()
        .map(|z| (z.name.as_str(), z.cells.as_slice()))
        .collect();
    assert_eq!(zones, [("b", &[0, 1, 5][..]), ("a", &[2, 3, 4][..])],);
}

#[test]
fn zones_are_written_as_openfoam_writes_them() {
    let mesh = mesh_of_file("named.dict").unwrap();
    let dir = std::env::temp_dir().join(format!("vortia-zones-{}", std::process::id()));
    mesh.write_dir(&dir).unwrap();
    let text = std::fs::read_to_string(dir.join("cellZones")).unwrap();
    let back = RawPolyMesh::read_cell_zones_in(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(back, mesh.cell_zones);
    let body = text.split("\n// * ").nth(1).unwrap();
    assert!(
        body.contains(
            "\n1\n(\nhot\n{\n    type cellZone;\ncellLabels      List<label> \n28\n(\n0\n"
        )
    );
}

/// Up to ten labels are written on the line of their count, more one to a
/// line: the two layouts of OpenFOAM's lists, byte for byte.
#[test]
fn short_and_long_zone_lists_are_written_in_openfoams_two_layouts() {
    let write = |mesh: &RawPolyMesh, tag: &str| {
        let dir = std::env::temp_dir().join(format!("vortia-zl-{}-{tag}", std::process::id()));
        mesh.write_dir(&dir).unwrap();
        let text = std::fs::read_to_string(dir.join("cellZones")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        text.split("\n// * ").nth(1).unwrap().to_owned()
    };
    let short = write(&mesh_of_text("zl-short", ZONE_ORDER).unwrap(), "short");
    assert!(short.contains(
        "\n2\n(\nb\n{\n    type cellZone;\ncellLabels      List<label> 3(0 1 5);\n}\n\na\n{"
    ));
    assert!(short.contains("cellLabels      List<label> 3(2 3 4);\n}\n)\n"));
}
