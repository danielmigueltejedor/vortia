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

#[test]
fn directives_a_solver_would_need_evaluated_are_named() {
    let head = "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
                dimensions [0 0 0 1 0 0 0]; internalField uniform 1;\n";
    let with = |body: &str| scalar_from(&format!("{head}{body}")).unwrap();
    let constraint = with(
        "boundaryField { #includeEtc \"caseDicts/setConstraintTypes\"\n inlet { type zeroGradient; } }",
    );
    assert_eq!(constraint.unevaluated_directive(), None);
    let include = with("boundaryField { \".*\" { type zeroGradient; }\n #include \"inlets\"\n }");
    assert_eq!(include.unevaluated_directive().as_deref(), Some("#include"));
    let in_patch = with("boundaryField { inlet { type fixedValue; #include \"v\"\n } }");
    assert_eq!(
        in_patch.unevaluated_directive().as_deref(),
        Some("#include")
    );
    let top = with("#include \"initialConditions\"\nboundaryField { }");
    assert_eq!(top.unevaluated_directive().as_deref(), Some("#include"));
}

#[test]
fn what_is_written_reads_back_and_stays_in_the_case() {
    let mut field = scalar_from(
        "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
         dimensions [0 0 0 1 0 0 0]; internalField uniform 1;\n\
         boundaryField { \"inlet one\" { type zeroGradient; } }",
    )
    .unwrap();
    let back = scalar_from(&field.to_text("0")).unwrap();
    assert_eq!(back.boundary[0].key, "inlet one");

    let dir = tempfile::tempdir().unwrap();
    assert!(field.write(dir.path(), "0\"x").is_err());
    field.name = "p field".into();
    assert!(field.write(dir.path(), "0").is_err());
    field.name = "T".into();
    #[cfg(unix)]
    {
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("1")).unwrap();
        assert!(field.write(dir.path(), "1").is_err());
        assert!(!elsewhere.path().join("T").exists());
    }
    field.write(dir.path(), "0").unwrap();
}

#[test]
fn values_the_solver_sets_replace_those_of_the_conditions_that_follow_the_flow() {
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box;

    let bounds = Aabb {
        min: Vec3::default(),
        max: Vec3::new(2.0, 1.0, 1.0),
    };
    let mesh = structured_box(bounds, [2, 1, 1]).unwrap();
    let mut field = scalar_from(
        "FoamFile { version 2.0; format ascii; class volScalarField; object p; }\n\
         dimensions [1 -1 -2 0 0 0 0]; internalField uniform 1;\n\
         boundaryField {\n\
           xMin { type totalPressure; p0 uniform 5; value uniform 1; }\n\
           xMax { type fixedValue; value uniform 2; }\n\
           yMin { type totalPressure; p0 uniform 5; value uniform 1; }\n\
           yMax { type zeroGradient; }\n\
           zMin { type zeroGradient; }\n\
           zMax { type zeroGradient; }\n\
         }",
    )
    .unwrap();
    let faces = mesh.n_faces() - mesh.n_internal_faces();
    let values: Vec<Option<f64>> = (0..faces).map(|k| Some(10.0 + k as f64)).collect();
    field.store_patch_values(&mesh, &["totalPressure"], &values);

    let value = |field: &ScalarField, name: &str| {
        let patch = mesh.patch(name).unwrap();
        field.patch_field(patch).unwrap().value.clone()
    };
    // One face: its value, as a uniform one.
    assert_eq!(value(&field, "xMin"), Some(Data::Uniform(10.0)));
    // Two faces that differ: a list.
    assert_eq!(
        value(&field, "yMin"),
        Some(Data::NonUniform(vec![12.0, 13.0]))
    );
    // Another kind of condition keeps what it had.
    assert_eq!(value(&field, "xMax"), Some(Data::Uniform(2.0)));
    assert_eq!(value(&field, "yMax"), None);
    // A list that is too short for the patch leaves it as it was.
    field.store_patch_values(&mesh, &["totalPressure"], &values[..1]);
    assert_eq!(
        value(&field, "yMin"),
        Some(Data::NonUniform(vec![12.0, 13.0]))
    );
}

#[test]
fn patches_sharing_a_pattern_entry_each_get_their_own_values() {
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box;

    let bounds = Aabb {
        min: Vec3::default(),
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    let mesh = structured_box(bounds, [1, 1, 1]).unwrap();
    let mut field = scalar_from(
        "FoamFile { version 2.0; format ascii; class volScalarField; object p; }\n\
         dimensions [1 -1 -2 0 0 0 0]; internalField uniform 1;\n\
         boundaryField {\n\
           \".*\" { type zeroGradient; }\n\
           \"(xMin|xMax)\" { type totalPressure; p0 uniform 5; value uniform 1; }\n\
         }",
    )
    .unwrap();
    let values: Vec<Option<f64>> = (0..6).map(|k| Some(10.0 + k as f64)).collect();
    field.store_patch_values(&mesh, &["totalPressure"], &values);
    let value = |name: &str| {
        field
            .patch_field(mesh.patch(name).unwrap())
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(value("xMin"), Some(Data::Uniform(10.0)));
    assert_eq!(value("xMax"), Some(Data::Uniform(11.0)));
    // The entry they share keeps its type and settings in each copy.
    let own = field.boundary.iter().find(|p| p.key == "xMax").unwrap();
    assert_eq!(own.kind, "totalPressure");
    assert_eq!(own.settings.scalar("p0"), None);
    assert!(own.settings.values("p0").is_some());
    // Another patch is not touched.
    assert_eq!(value("yMin"), None);
}
