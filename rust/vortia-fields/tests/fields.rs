use std::path::PathBuf;

use vortia_core::Vec3;
use vortia_fields::{Data, Field, FieldValue, Location, ScalarField, VectorField, times};
use vortia_foamfile::FoamFile;
use vortia_mesh::PolyMesh;

fn nozzle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/nozzle-practice-2026")
}

fn class_of(path: &std::path::Path) -> String {
    let file = FoamFile::open(path).unwrap();
    file.tokens().unwrap().0.unwrap().class
}

/// Reads, checks against the mesh, writes, reads back: the same field.
fn round_trip<T: FieldValue>(mesh: &PolyMesh, path: &std::path::Path, time: &str) -> Field<T> {
    let field = Field::<T>::read(path).unwrap();
    field.check_against(mesh).unwrap();
    let dir = tempfile::tempdir().unwrap();
    field.write(dir.path(), time).unwrap();
    let back = Field::<T>::read(dir.path().join(time).join(&field.name)).unwrap();
    assert_eq!(field, back, "{}", path.display());
    field
}

#[test]
fn every_nozzle_field_reads_fits_the_mesh_and_round_trips() {
    let mesh = PolyMesh::read_case(nozzle()).unwrap();
    let mut seen = 0;
    for time in ["0", "500"] {
        for entry in std::fs::read_dir(nozzle().join(time)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            // "U 2" and the like are stray copies, not fields of the case.
            if path.is_dir() || name.contains(' ') {
                continue;
            }
            match class_of(&path).as_str() {
                "volScalarField" | "surfaceScalarField" => {
                    round_trip::<f64>(&mesh, &path, time);
                }
                "volVectorField" => {
                    round_trip::<Vec3>(&mesh, &path, time);
                }
                other => panic!("{name}: unexpected class {other}"),
            }
            seen += 1;
        }
    }
    assert!(seen >= 16, "{seen}");
}

#[test]
fn the_solved_fields_hold_what_openfoam_wrote() {
    let mesh = PolyMesh::read_case(nozzle()).unwrap();
    let p = ScalarField::read(nozzle().join("500/p")).unwrap();
    let values = p.internal.expand(mesh.n_cells()).unwrap();
    assert_eq!(values[0], 101603.0);

    let u = VectorField::read(nozzle().join("500/U")).unwrap();
    let inlet = mesh.patch("inlet").unwrap();
    let pf = u.patch_field(inlet).unwrap();
    assert_eq!(pf.kind, "pressureInletVelocity");
    let first = pf.value.as_ref().unwrap().expand(inlet.size).unwrap()[0];
    assert_eq!(first, Vec3::new(172.252, 0.0, 0.0));

    let phi = ScalarField::read(nozzle().join("500/phi")).unwrap();
    assert_eq!(phi.location, Location::Faces);
    assert_eq!(phi.internal.count(), Some(mesh.n_internal_faces()));

    let names: Vec<_> = times(nozzle())
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["0", "100", "200", "300", "400", "500"]);
}

#[test]
fn settings_of_a_condition_survive_the_round_trip() {
    let p = ScalarField::read(nozzle().join("500/p")).unwrap();
    let inlet = p.boundary.iter().find(|b| b.key == "inlet").unwrap();
    assert_eq!(inlet.kind, "totalPressure");
    assert!(inlet.settings.get("p0").is_some(), "{:?}", inlet.settings);
}

#[test]
fn a_field_that_does_not_fit_is_refused() {
    let mesh = PolyMesh::read_case(nozzle()).unwrap();
    let mut p = ScalarField::read(nozzle().join("500/p")).unwrap();
    p.internal = Data::NonUniform(vec![1.0; 3]);
    assert!(p.check_against(&mesh).is_err());

    let mut p = ScalarField::read(nozzle().join("0/p")).unwrap();
    p.boundary.retain(|b| b.key != "walls");
    assert!(p.check_against(&mesh).is_err());

    // A vector field cannot be read as scalars.
    assert!(ScalarField::read(nozzle().join("500/U")).is_err());
}

fn scalar_from(text: &str) -> vortia_fields::Result<ScalarField> {
    let file = FoamFile::from_bytes("T", text.as_bytes().to_vec());
    ScalarField::from_file(&file)
}

#[test]
fn a_field_cannot_name_its_way_out_of_the_case() {
    for object in ["../../escape", "a/b", ".."] {
        let text = format!(
            "FoamFile {{ version 2.0; format ascii; class volScalarField; object {object}; }}\n\
             dimensions [0 0 0 1 0 0 0]; internalField uniform 1; boundaryField {{ }}"
        );
        assert!(scalar_from(&text).is_err(), "{object}");
    }
    let mut ok = scalar_from(
        "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
         dimensions [0 0 0 1 0 0 0]; internalField uniform 1; boundaryField { }",
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(ok.write(dir.path(), "../up").is_err());
    ok.name = "../T".into();
    assert!(ok.write(dir.path(), "0").is_err());
}

#[test]
fn boundary_directives_are_written_back() {
    let field = scalar_from(
        "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
         dimensions [0 0 0 1 0 0 0]; internalField uniform 1;\n\
         boundaryField { #includeEtc \"caseDicts/setConstraintTypes\"\n inlet { type zeroGradient; } }",
    )
    .unwrap();
    let text = field.to_text("0");
    assert!(text.contains("#includeEtc"), "{text}");
    let back = scalar_from(&text).unwrap();
    assert_eq!(field, back);
}

#[test]
fn a_condition_type_that_would_not_read_back_is_refused() {
    let text = "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
                dimensions [0 0 0 1 0 0 0]; internalField uniform 1;\n\
                boundaryField { inlet { type \"fixed; value\"; } }";
    assert!(scalar_from(text).is_err());
}
