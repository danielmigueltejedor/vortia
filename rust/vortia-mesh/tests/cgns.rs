//! The CGNS (ADF) importer.
//!
//! The files are written here, by a small ADF writer that follows the layout
//! of real files (the importer was also checked on a 3 527-point Fluent
//! export, whose cells, areas and volume it reproduces, and whose mesh
//! OpenFOAM's `checkMesh` accepts): one node is a `NoDe` header, a table of
//! its children and one chunk of data, and a pointer is eight hex digits of
//! 4 096-byte block and four of offset.

use vortia_foamfile::RawPolyMesh;
use vortia_mesh::cgns::cgns_to_foam;
use vortia_mesh::fluent::Options;
use vortia_mesh::{MeshGeometry, PolyMesh};

enum Data {
    None,
    Text(&'static str),
    Ints(Vec<i32>),
    Reals(Vec<f64>),
}

struct Node {
    name: &'static str,
    label: &'static str,
    data: Data,
    children: Vec<Node>,
}

fn node(name: &'static str, label: &'static str, data: Data, children: Vec<Node>) -> Node {
    Node {
        name,
        label,
        data,
        children,
    }
}

fn pointer(address: usize) -> String {
    format!("{:08X}{:04X}", address / 4096, address % 4096)
}

fn padded(text: &str) -> Vec<u8> {
    let mut v = text.as_bytes().to_vec();
    v.resize(32, b' ');
    v
}

fn write(buf: &mut Vec<u8>, n: &Node, big: bool) -> usize {
    let children: Vec<(&'static str, usize)> = n
        .children
        .iter()
        .map(|c| (c.name, write(buf, c, big)))
        .collect();
    let mut sub_table = 0;
    if !children.is_empty() {
        sub_table = buf.len();
        buf.extend(b"SNTb");
        buf.extend(pointer(sub_table + 16 + 44 * children.len()).bytes());
        for (name, at) in &children {
            buf.extend(padded(name));
            buf.extend(pointer(*at).bytes());
        }
    }
    let (kind, dims, bytes): (&str, Vec<usize>, Vec<u8>) = match &n.data {
        Data::None => ("MT", vec![], vec![]),
        Data::Text(t) => ("C1", vec![t.len()], t.as_bytes().to_vec()),
        Data::Ints(v) => (
            "I4",
            vec![v.len()],
            v.iter()
                .flat_map(|x| {
                    if big {
                        x.to_be_bytes()
                    } else {
                        x.to_le_bytes()
                    }
                })
                .collect(),
        ),
        Data::Reals(v) => (
            "R8",
            vec![v.len()],
            v.iter()
                .flat_map(|x| {
                    if big {
                        x.to_be_bytes()
                    } else {
                        x.to_le_bytes()
                    }
                })
                .collect(),
        ),
    };
    let mut chunk = 0;
    if !bytes.is_empty() {
        chunk = buf.len();
        buf.extend(b"DaTa");
        buf.extend(pointer(chunk + 16 + bytes.len()).bytes());
        buf.extend(&bytes);
        buf.extend(b"dEnD");
    }
    let at = buf.len();
    buf.extend(b"NoDe");
    buf.extend(padded(n.name));
    buf.extend(padded(n.label));
    buf.extend(format!("{:08X}{:08X}", children.len(), children.len()).bytes());
    buf.extend(pointer(sub_table).bytes());
    buf.extend(padded(kind));
    buf.extend(format!("{:02X}", dims.len()).bytes());
    for i in 0..12 {
        buf.extend(format!("{:08X}", dims.get(i).copied().unwrap_or(0)).bytes());
    }
    buf.extend(format!("{:04X}", usize::from(!bytes.is_empty())).bytes());
    buf.extend(pointer(chunk).bytes());
    buf.extend(b"TaiL");
    at
}

/// A file from the pieces of a CGNS tree.
fn file(root: &Node, big: bool) -> Vec<u8> {
    let mut buf = vec![b' '; 0x10A];
    buf[..4].copy_from_slice(&[0xc0, 0xa8, 0xa3, 0xa9]);
    buf[4..32].copy_from_slice(b"ADF Database Version A02011>");
    buf[0x5E..0x64].copy_from_slice(if big { b"AdF2BB" } else { b"AdF2LL" });
    let at = write(&mut buf, root, big);
    buf[0x80..0x84].copy_from_slice(b"AdF4");
    buf[0x84..0x90].copy_from_slice(pointer(at).as_bytes());
    buf
}

struct Section {
    name: &'static str,
    kind: i32,
    connectivity: Vec<i32>,
}

fn section(name: &'static str, kind: i32, connectivity: &[i32]) -> Section {
    Section {
        name,
        kind,
        connectivity: connectivity.to_vec(),
    }
}

/// A CGNS file of one base and one unstructured zone.
fn cgns(
    cell_dim: i32,
    points: &[[f64; 3]],
    sections: &[Section],
    bcs: &[(&'static str, &'static str)],
    big: bool,
) -> Vec<u8> {
    let column = |k: usize| points.iter().map(|p| p[k]).collect::<Vec<_>>();
    let mut zone = vec![
        node("ZoneType", "ZoneType_t", Data::Text("Unstructured"), vec![]),
        node(
            "GridCoordinates",
            "GridCoordinates_t",
            Data::None,
            vec![
                node("CoordinateX", "DataArray_t", Data::Reals(column(0)), vec![]),
                node("CoordinateY", "DataArray_t", Data::Reals(column(1)), vec![]),
                node("CoordinateZ", "DataArray_t", Data::Reals(column(2)), vec![]),
            ],
        ),
    ];
    for s in sections {
        zone.push(node(
            s.name,
            "Elements_t",
            Data::Ints(vec![s.kind, 0]),
            vec![node(
                "ElementConnectivity",
                "DataArray_t",
                Data::Ints(s.connectivity.clone()),
                vec![],
            )],
        ));
    }
    if !bcs.is_empty() {
        zone.push(node(
            "ZoneBC",
            "ZoneBC_t",
            Data::None,
            bcs.iter()
                .map(|(n, t)| node(n, "BC_t", Data::Text(t), vec![]))
                .collect(),
        ));
    }
    let root = node(
        "ADF MotherNode",
        "Root Node of ADF File",
        Data::None,
        vec![
            node(
                "CGNSLibraryVersion",
                "CGNSLibraryVersion_t",
                Data::None,
                vec![],
            ),
            node(
                "Base",
                "CGNSBase_t",
                Data::Ints(vec![cell_dim, 3]),
                vec![node(
                    "Zone",
                    "Zone_t",
                    Data::Ints(vec![points.len() as i32, 1, 0]),
                    zone,
                )],
            ),
        ],
    );
    file(&root, big)
}

const BAR: i32 = 3;
const TRI: i32 = 5;
const QUAD: i32 = 7;
const TETRA: i32 = 10;
const PYRA: i32 = 12;
const PENTA: i32 = 14;
const HEXA: i32 = 17;
const MIXED: i32 = 20;

/// Two squares side by side: cells, an inlet, an outlet and four walls.
fn two_squares(map: impl Fn(f64, f64) -> [f64; 3], reverse: bool, big: bool) -> Vec<u8> {
    let points: Vec<[f64; 3]> = [(0., 0.), (1., 0.), (2., 0.), (0., 1.), (1., 1.), (2., 1.)]
        .iter()
        .map(|&(x, y)| map(x, y))
        .collect();
    let cells: &[i32] = if reverse {
        &[1, 4, 5, 2, 2, 5, 6, 3]
    } else {
        &[1, 2, 5, 4, 2, 3, 6, 5]
    };
    cgns(
        2,
        &points,
        &[
            section("cells", QUAD, cells),
            section("inlet", BAR, &[4, 1]),
            section("outlet", BAR, &[3, 6]),
            section("walls", BAR, &[1, 2, 2, 3, 5, 4, 6, 5]),
        ],
        &[("walls", "BCWall"), ("inlet", "BCInflow")],
        big,
    )
}

fn convert(bytes: &[u8]) -> RawPolyMesh {
    cgns_to_foam(bytes, Options::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn patches(m: &RawPolyMesh) -> Vec<(&str, &str, usize)> {
    m.patches
        .iter()
        .map(|p| (p.name.as_str(), p.kind.as_str(), p.n_faces))
        .collect()
}

/// The volumes of the cells of a mesh, all of which must be positive.
fn volumes(m: &RawPolyMesh) -> Vec<f64> {
    let mesh = PolyMesh::from_raw(m.clone()).unwrap();
    MeshGeometry::compute(&mesh).cell_volumes
}

#[test]
fn a_2d_mesh_is_extruded_across_the_z_axis() {
    let m = convert(&two_squares(|x, y| [x, y, 0.0], false, false));
    assert_eq!(
        (m.points.len(), m.faces.len(), m.neighbour.len()),
        (12, 11, 1)
    );
    assert_eq!(
        patches(&m),
        [
            ("inlet", "patch", 1),
            ("outlet", "patch", 1),
            ("walls", "wall", 4),
            ("frontAndBackPlanes", "empty", 4),
        ]
    );
    // The thickness is 2 % of the diagonal of the plane: sqrt(2^2 + 1^2).
    let thickness = 0.02 * 5.0_f64.sqrt();
    let v = volumes(&m);
    assert_eq!(v.len(), 2);
    for x in v {
        assert!((x - thickness).abs() < 1e-12, "{x} {thickness}");
    }
    assert!(
        m.points
            .iter()
            .all(|p| (p.z.abs() - thickness / 2.0).abs() < 1e-12)
    );
}

#[test]
fn a_2d_mesh_is_extruded_across_whichever_axis_it_is_flat_in() {
    let reference = volumes(&convert(&two_squares(|x, y| [x, y, 0.0], false, false)));
    for (name, map) in [
        ("yz", (|x, y| [0.0, x, y]) as fn(f64, f64) -> [f64; 3]),
        ("zx", |x, y| [y, 0.0, x]),
        ("xz-flipped", |x, y| [x, 5.0, y]),
    ] {
        for reverse in [false, true] {
            let m = convert(&two_squares(map, reverse, false));
            let v = volumes(&m);
            assert_eq!(v.len(), 2, "{name}");
            assert!(
                v.iter().all(|&x| x > 0.0),
                "{name} reversed={reverse}: {v:?}"
            );
            for (a, b) in v.iter().zip(&reference) {
                assert!((a - b).abs() < 1e-12, "{name}");
            }
            assert_eq!(m.patches.last().map(|p| p.kind.as_str()), Some("empty"));
        }
    }
}

#[test]
fn cells_written_clockwise_are_turned_round() {
    let m = convert(&two_squares(|x, y| [x, y, 0.0], true, false));
    assert!(volumes(&m).iter().all(|&x| x > 0.0));
}

#[test]
fn a_big_endian_file_reads_as_the_same_mesh() {
    let little = convert(&two_squares(|x, y| [x, y, 0.0], false, false));
    let big = convert(&two_squares(|x, y| [x, y, 0.0], false, true));
    assert_eq!(little.points, big.points);
    assert_eq!(little.faces, big.faces);
    assert_eq!(little.owner, big.owner);
}

#[test]
fn a_mixed_section_makes_prisms_and_hexahedra() {
    let points = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [2., 0., 0.],
    ];
    let bytes = cgns(
        2,
        &points,
        &[
            section("fluid", MIXED, &[QUAD, 1, 2, 3, 4, TRI, 2, 5, 3]),
            section("edge", BAR, &[1, 2, 2, 5]),
        ],
        &[],
        false,
    );
    let m = convert(&bytes);
    let v = volumes(&m);
    assert_eq!(v.len(), 2);
    let t = 0.02 * 5.0_f64.sqrt();
    assert!(
        (v[0] - t).abs() < 1e-12 && (v[1] - t / 2.0).abs() < 1e-12,
        "{v:?}"
    );
    let triangles = m
        .faces
        .offsets
        .windows(2)
        .filter(|w| w[1] - w[0] == 3)
        .count();
    assert_eq!(triangles, 2);
    assert_eq!(patches(&m)[0], ("edge", "patch", 2));
}

#[test]
fn every_3d_cell_shape_is_right_side_out_whichever_way_it_was_written() {
    let cube = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
        [1., 0., 1.],
        [1., 1., 1.],
        [0., 1., 1.],
    ];
    let wedge = [
        [0., 0., 0.],
        [1., 0., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
        [1., 0., 1.],
        [0., 1., 1.],
    ];
    let tet = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let pyramid = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0.5, 0.5, 1.],
    ];
    type Case = (&'static str, i32, Vec<[f64; 3]>, Vec<i32>, f64);
    let cases: [Case; 4] = [
        ("tet", TETRA, tet.to_vec(), vec![1, 2, 3, 4], 1.0 / 6.0),
        (
            "pyramid",
            PYRA,
            pyramid.to_vec(),
            vec![1, 2, 3, 4, 5],
            1.0 / 3.0,
        ),
        ("prism", PENTA, wedge.to_vec(), vec![1, 2, 3, 4, 5, 6], 0.5),
        (
            "hex",
            HEXA,
            cube.to_vec(),
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            1.0,
        ),
    ];
    for (name, kind, points, order, volume) in cases {
        let n = order.len();
        let flipped: Vec<i32> = match n {
            4 => vec![1, 3, 2, 4],
            5 => vec![1, 4, 3, 2, 5],
            6 => vec![1, 3, 2, 4, 6, 5],
            _ => vec![5, 6, 7, 8, 1, 2, 3, 4],
        };
        for conn in [order, flipped] {
            let m = convert(&cgns(
                3,
                &points,
                &[section("cells", kind, &conn)],
                &[],
                false,
            ));
            let v = volumes(&m);
            assert_eq!(v.len(), 1, "{name}");
            assert!((v[0] - volume).abs() < 1e-12, "{name} {conn:?}: {v:?}");
            assert_eq!(patches(&m), [("default_wall", "wall", m.faces.len())]);
        }
    }
}

#[test]
fn the_boundary_of_a_3d_mesh_is_a_patch_for_each_section() {
    let points = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let m = convert(&cgns(
        3,
        &points,
        &[
            section("tet", TETRA, &[1, 2, 3, 4]),
            section("floor", TRI, &[1, 3, 2]),
            section("slope", TRI, &[2, 3, 4]),
        ],
        &[("floor", "BCSymmetryPlane")],
        false,
    ));
    assert_eq!(
        patches(&m),
        [
            ("floor", "symmetry", 1),
            ("slope", "patch", 1),
            ("default_wall", "wall", 2),
        ]
    );
}

#[test]
fn sections_of_cells_are_cell_zones_when_asked_for() {
    let points = [
        [0., 0., 0.],
        [1., 0., 0.],
        [2., 0., 0.],
        [0., 1., 0.],
        [1., 1., 0.],
        [2., 1., 0.],
    ];
    let bytes = cgns(
        2,
        &points,
        &[
            section("left", QUAD, &[1, 2, 5, 4]),
            section("right", QUAD, &[2, 3, 6, 5]),
        ],
        &[],
        false,
    );
    let plain = convert(&bytes);
    assert!(plain.cell_zones.is_empty());
    let zoned = cgns_to_foam(
        &bytes,
        Options {
            write_zones: true,
            ..Options::default()
        },
    )
    .unwrap();
    let zones: Vec<(&str, &[usize])> = zoned
        .cell_zones
        .iter()
        .map(|z| (z.name.as_str(), z.cells.as_slice()))
        .collect();
    assert_eq!(zones, [("left", [0].as_slice()), ("right", [1].as_slice())]);
    assert_eq!(zoned.face_zones.len(), 2);
}

#[test]
fn scale_and_thickness_apply_in_that_order() {
    let bytes = two_squares(|x, y| [x, y, 0.0], false, false);
    let m = cgns_to_foam(
        &bytes,
        Options {
            scale: 0.001,
            thickness: Some(0.5),
            write_zones: false,
        },
    )
    .unwrap();
    assert!(m.points.iter().all(|p| (p.z.abs() - 0.25e-3).abs() < 1e-15));
    assert!(m.points.iter().any(|p| (p.x - 2e-3).abs() < 1e-15));
}

#[test]
fn files_it_cannot_use_are_errors_that_say_why() {
    let good = two_squares(|x, y| [x, y, 0.0], false, false);
    let hdf5 = b"\x89HDF\r\n\x1a\n and more of it".to_vec();
    let message = |bytes: &[u8]| {
        cgns_to_foam(bytes, Options::default())
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
    };
    assert!(message(&hdf5).contains("not an ADF file"));
    assert!(message(b"").contains("not an ADF file"));
    let points = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    let higher = cgns(
        2,
        &points,
        &[section("c", 6, &[1, 2, 3, 1, 2, 3])],
        &[],
        false,
    );
    assert!(message(&higher).contains("higher-order"));
    let poly = cgns(3, &points, &[section("c", 22, &[1, 2, 3])], &[], false);
    assert!(message(&poly).contains("polyhedral"));
    let outside = cgns(2, &points, &[section("c", TRI, &[1, 2, 9])], &[], false);
    assert!(message(&outside).contains("node 9"));
    let cut = cgns(2, &points, &[section("c", TRI, &[1, 2])], &[], false);
    assert!(message(&cut).contains("ends inside an element"));
    let none = cgns(2, &points, &[section("e", BAR, &[1, 2])], &[], false);
    assert!(message(&none).contains("no cells"));
    let tilted = cgns(
        2,
        &[[0., 0., 0.], [1., 1., 1.], [0., 1., 0.5]],
        &[section("c", TRI, &[1, 2, 3])],
        &[],
        false,
    );
    assert!(message(&tilted).contains("plane"));
    assert!(!message(good.get(..good.len() / 2).unwrap_or_default()).is_empty());
}

#[test]
fn a_mesh_written_to_a_case_reads_back() {
    let m = convert(&two_squares(|x, y| [x, y, 0.0], false, false));
    let dir = std::env::temp_dir().join(format!("vortia-cgns-{}", std::process::id()));
    m.write_case(&dir).unwrap();
    let back = RawPolyMesh::read_case(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(back.points, m.points);
    assert_eq!(back.faces, m.faces);
    assert_eq!(back.owner, m.owner);
    assert_eq!(back.patches.len(), m.patches.len());
}

#[test]
fn hostile_files_never_panic() {
    let tetra = cgns(
        3,
        &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        &[
            section("tet", TETRA, &[1, 2, 3, 4]),
            section("floor", TRI, &[1, 3, 2]),
        ],
        &[("floor", "BCWall")],
        false,
    );
    for original in [
        two_squares(|x, y| [x, y, 0.0], false, false),
        two_squares(|x, y| [x, y, 0.0], false, true),
        tetra,
    ] {
        // Every prefix of the file.
        for n in 0..=original.len() {
            let _ = cgns_to_foam(original.get(..n).unwrap_or_default(), Options::default());
        }
        // Every byte replaced by values that stress counts and pointers.
        for at in 0..original.len() {
            for bad in [0x00, 0xFF, b'0', b'9', b'F', b' '] {
                let mut mangled = original.clone();
                if let Some(b) = mangled.get_mut(at) {
                    *b = bad;
                }
                let _ = cgns_to_foam(&mangled, Options::default());
            }
        }
    }
}
