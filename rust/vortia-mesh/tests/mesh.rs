use std::path::PathBuf;

use vortia_core::{Aabb, Vec3};
use vortia_foamfile::{FaceList, RawPatch, RawPolyMesh};
use vortia_mesh::{MeshGeometry, PatchKind, PolyMesh, blocks, check};

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(f64::MIN_POSITIVE)
}

#[test]
fn a_box_has_exact_volumes_and_centres_and_passes_every_check() {
    let bounds = Aabb {
        min: Vec3::new(-1.0, 0.0, 2.0),
        max: Vec3::new(2.0, 1.0, 4.0),
    };
    let mesh = blocks::structured_box(bounds, [3, 2, 4]).unwrap();
    assert_eq!(mesh.n_cells(), 24);
    // (nx-1)·ny·nz + nx·(ny-1)·nz + nx·ny·(nz-1)
    assert_eq!(mesh.n_internal_faces(), 2 * 2 * 4 + 3 * 4 + 3 * 2 * 3);
    assert_eq!(
        mesh.n_faces() - mesh.n_internal_faces(),
        2 * (2 * 4 + 3 * 4 + 3 * 2)
    );

    let geo = MeshGeometry::compute(&mesh);
    for &v in &geo.cell_volumes {
        assert!(close(v, 0.25, 1e-12), "{v}");
    }
    assert!(close(geo.total_volume(), 6.0, 1e-12));
    assert!((geo.cell_centres[0] - Vec3::new(-0.5, 0.25, 2.25)).norm() < 1e-12);

    let report = check(&mesh, &geo);
    assert!(report.is_ok(), "{:?}", report.failed_checks());
    assert!(report.non_orthogonality.unwrap().max < 1e-6);
    assert!(report.skewness.unwrap().max < 1e-6);
    assert_eq!(report.bounds, bounds);
}

#[test]
fn a_box_rejects_nonsense_sizes() {
    let unit = Aabb {
        min: Vec3::default(),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    assert!(blocks::structured_box(unit, [0, 1, 1]).is_err());
    assert!(blocks::structured_box(unit, [usize::MAX, 2, 2]).is_err());
    assert!(blocks::structured_box(Aabb::default(), [1, 1, 1]).is_err());
    // Within the cell limit, but its points alone would take 12 GiB.
    assert!(blocks::structured_box(unit, [blocks::MAX_CELLS, 1, 1]).is_err());
}

#[test]
fn a_flipped_face_is_reported() {
    let unit = Aabb {
        min: Vec3::default(),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    let mesh = blocks::structured_box(unit, [2, 1, 1]).unwrap();
    let mut faces = mesh.faces().clone();
    let (a, b) = (faces.offsets[0], faces.offsets[1]);
    faces.points[a..b].reverse();
    let raw = RawPolyMesh {
        points: mesh.points().to_vec(),
        faces,
        owner: mesh.owner().to_vec(),
        neighbour: mesh.neighbour().to_vec(),
        patches: raw_patches(&mesh),
    };
    let flipped = PolyMesh::from_raw(raw).unwrap();
    let report = check(&flipped, &MeshGeometry::compute(&flipped));
    assert!(report.wrongly_oriented > 0);
    assert!(!report.is_ok());
}

#[test]
fn geometry_that_overflows_fails_the_check() {
    let huge = Aabb {
        min: Vec3::new(1e160, 1e160, 1e160),
        max: Vec3::new(3e160, 3e160, 3e160),
    };
    let mesh = blocks::structured_box(huge, [1, 1, 1]).unwrap();
    let report = check(&mesh, &MeshGeometry::compute(&mesh));
    assert!(report.non_finite_geometry > 0);
    assert!(!report.is_ok());
}

fn raw_patches(mesh: &PolyMesh) -> Vec<RawPatch> {
    mesh.patches()
        .iter()
        .map(|p| RawPatch {
            name: p.name.clone(),
            kind: p.kind.name().to_owned(),
            start_face: p.start,
            n_faces: p.size,
            groups: Vec::new(),
            dict: Default::default(),
        })
        .collect()
}

#[test]
fn inconsistent_parts_are_refused_not_panicked_on() {
    let tri = |pts: Vec<usize>| FaceList {
        offsets: vec![0, pts.len()],
        points: pts,
    };
    let base = RawPolyMesh {
        points: vec![Vec3::default(); 3],
        faces: tri(vec![0, 1, 2]),
        owner: vec![0],
        neighbour: vec![],
        patches: vec![RawPatch {
            name: "all".into(),
            kind: "wall".into(),
            start_face: 0,
            n_faces: 1,
            groups: vec![],
            dict: Default::default(),
        }],
    };
    assert!(PolyMesh::from_raw(base.clone()).is_ok());

    let mut bad = base.clone();
    bad.faces = tri(vec![0, 1, 7]);
    assert!(PolyMesh::from_raw(bad).is_err(), "point out of range");

    let mut bad = base.clone();
    bad.owner = vec![];
    assert!(PolyMesh::from_raw(bad).is_err(), "owner count");

    let mut bad = base.clone();
    bad.patches[0].n_faces = usize::MAX;
    assert!(PolyMesh::from_raw(bad).is_err(), "patch overflow");

    let mut bad = base.clone();
    bad.patches.clear();
    assert!(PolyMesh::from_raw(bad).is_err(), "faces in no patch");

    let mut bad = base.clone();
    bad.owner = vec![1_000_000_000];
    assert!(
        PolyMesh::from_raw(bad).is_err(),
        "cell label sizes the mesh"
    );

    let mut bad = base.clone();
    bad.owner = vec![usize::MAX];
    assert!(PolyMesh::from_raw(bad).is_err(), "cell label overflow");

    let mut bad = base.clone();
    bad.patches.push(bad.patches[0].clone());
    bad.patches[0].n_faces = 0;
    assert!(PolyMesh::from_raw(bad).is_err(), "two patches, one name");

    let mut bad = base;
    bad.points[0].x = f64::NAN;
    assert!(PolyMesh::from_raw(bad).is_err(), "non-finite point");
}

/// The nozzle practice case: an axisymmetric wedge meshed by OpenFOAM's
/// `blockMesh`, read from disk exactly as OpenFOAM wrote it.
#[test]
fn the_nozzle_wedge_reads_and_has_the_analytic_volume() {
    let mesh = PolyMesh::read_case(example("nozzle-practice-2026")).unwrap();
    // From the header OpenFOAM wrote: "nPoints:15276 nCells:7500 nFaces:30100
    // nInternalFaces:14825".
    assert_eq!(
        (
            mesh.n_points(),
            mesh.n_cells(),
            mesh.n_faces(),
            mesh.n_internal_faces()
        ),
        (15276, 7500, 30100, 14825)
    );
    let names: Vec<_> = mesh.patches().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["axis", "inlet", "walls", "atmosphere", "front", "back"]
    );
    assert_eq!(
        mesh.patch("front").map(|p| &p.kind),
        Some(&PatchKind::Wedge)
    );

    let geo = MeshGeometry::compute(&mesh);
    // The domain is the rectangle 0 ≤ x ≤ 3 mm, 0 ≤ r ≤ 1.6 mm swept through
    // the wedge |z| ≤ r·tanθ, with tanθ = 4.36609429085/100 from blockMeshDict.
    // Its faces are planar, so the volume is exact: L · R² · tanθ.
    let expected = 3.0e-3 * 1.6e-3 * 1.6e-3 * (4.36609429085 / 100.0);
    assert!(
        close(geo.total_volume(), expected, 1e-9),
        "{} vs {expected}",
        geo.total_volume()
    );

    let report = check(&mesh, &geo);
    assert!(report.is_ok(), "{:?}", report.failed_checks());
    assert_eq!(report.unused_points, 0);

    // What OpenFOAM v1912's checkMesh prints for this case, to the digits it
    // prints them with.
    let six = |a: f64, b: f64| close(a, b, 5e-6);
    let vol = report.volume.unwrap();
    assert!(
        six(vol.min, 8.90456e-17) && six(vol.max, 2.92611e-13),
        "{vol:?}"
    );
    let area = report.face_area.unwrap();
    assert!(
        six(area.min, 2.20383e-12) && six(area.max, 7.32728e-09),
        "{area:?}"
    );
    let ortho = report.non_orthogonality.unwrap();
    assert!(
        six(ortho.max, 7.21974) && six(ortho.mean, 4.50556),
        "{ortho:?}"
    );
    assert!(six(report.skewness.unwrap().max, 0.32915));
}
