use std::time::Duration;
use tempfile::tempdir;
use vortia_core::error::ErrorCode;
use vortia_core::process::{ProcessRequest, run};
use vortia_core::project::Project;
use vortia_core::units::*;

#[test]
fn units_dimensional() {
    let d: Length = Velocity::new(2.0) * Time::new(3.0);
    assert!((d.si_value() - 6.0).abs() < 1e-12);
    let ul: KinematicViscosity = Velocity::new(1.0) * Length::new(0.1);
    let re: f64 = ul / KinematicViscosity::new(1.5e-5);
    assert!((re - 6666.6667).abs() < 1e-3);
}

#[test]
fn process_echo() {
    let req = ProcessRequest {
        executable: "/bin/sh".into(),
        arguments: vec!["-c".into(), "printf hello".into()],
        working_directory: None,
        environment: vec![],
        timeout: Some(Duration::from_secs(5)),
    };
    let r = run(&req).unwrap();
    assert!(r.succeeded());
    assert_eq!(r.stdout, "hello");
}

#[test]
fn process_timeout() {
    let req = ProcessRequest {
        executable: "/bin/sleep".into(),
        arguments: vec!["30".into()],
        working_directory: None,
        environment: vec![],
        timeout: Some(Duration::from_millis(200)),
    };
    let r = run(&req).unwrap();
    assert!(r.timed_out);
}

#[test]
fn project_roundtrip() {
    let dir = tempdir().unwrap();
    let mut p = Project::create(dir.path(), "cavity").unwrap();
    p.set_backend_fingerprint("esi/v2606");
    p.save().unwrap();
    let reopened = Project::open(dir.path()).unwrap();
    assert_eq!(reopened.name(), "cavity");
    assert_eq!(reopened.backend_fingerprint(), "esi/v2606");
}

#[test]
fn project_rejects_empty_name() {
    let dir = tempdir().unwrap();
    let r = Project::create(dir.path(), "");
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().code, ErrorCode::InvalidArgument);
}
