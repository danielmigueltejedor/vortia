use vortia_core::units::*;
use vortia_core::error::ErrorCode;
use vortia_core::process::{run, ProcessRequest};
use vortia_core::project::Project;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn units_dimensional() {
    let v = Quantity::velocity(2.0);
    let t = Quantity::time(3.0);
    let d = v.mul(t);
    assert!((d.si_value() - 6.0).abs() < 1e-12);
    assert_eq!(d.dim, DIM_LENGTH);
    let u = Quantity::velocity(1.0);
    let l = Quantity::length(0.1);
    let nu = Quantity::kinematic_viscosity(1.5e-5);
    let ul = u.mul(l);
    let re = ul.div(nu);
    assert!((re.si_value() - 6666.6667).abs() < 1e-3);
    assert_eq!(re.dim, DIM_DIMENSIONLESS);
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
    p.manifest.backend_fingerprint = "esi/v2606".into();
    p.save().unwrap();
    let reopened = Project::open(dir.path()).unwrap();
    assert_eq!(reopened.manifest.name, "cavity");
    assert_eq!(reopened.manifest.backend_fingerprint, "esi/v2606");
}

#[test]
fn project_rejects_empty_name() {
    let dir = tempdir().unwrap();
    let r = Project::create(dir.path(), "");
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().code, ErrorCode::InvalidArgument);
}