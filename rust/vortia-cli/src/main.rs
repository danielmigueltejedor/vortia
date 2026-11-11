//! `vortia`: the command-line front end of Vortia's engine.
//!
//! Today it reads a case's mesh and reports on it the way OpenFOAM's
//! `checkMesh` does, so the two can be compared on the same case. It grows a
//! subcommand per engine stage as the stages land.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::Path;
use std::process::ExitCode;

use vortia_mesh::check::{NON_ORTH_THRESHOLD, SKEW_THRESHOLD, Stat};
use vortia_mesh::{MeshGeometry, PolyMesh, check};

const USAGE: &str = "usage: vortia check-mesh [CASE]\n\n\
    check-mesh   read CASE/constant/polyMesh (default: the current directory)\n\
    \x20            and report its size, geometry and quality";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check-mesh"] => check_mesh(Path::new(".")),
        ["check-mesh", case] => check_mesh(Path::new(case)),
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn check_mesh(case: &Path) -> ExitCode {
    let mesh = match PolyMesh::read_case(case) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("vortia: {e}");
            return ExitCode::FAILURE;
        }
    };
    let geo = MeshGeometry::compute(&mesh);
    let r = check(&mesh, &geo);

    println!("Mesh stats");
    println!("    points:           {}", r.n_points);
    println!("    faces:            {}", r.n_faces);
    println!("    internal faces:   {}", r.n_internal_faces);
    println!("    cells:            {}", r.n_cells);
    println!();
    println!("Patches");
    for p in mesh.patches() {
        println!(
            "    {:<20} {:<14} {:>8} faces",
            p.name,
            p.kind.name(),
            p.size
        );
    }
    println!();
    println!("Geometry");
    let b = r.bounds;
    println!(
        "    bounding box:     ({} {} {}) ({} {} {})",
        b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z
    );
    println!("    total volume:     {:e}", r.total_volume);
    print_stat("cell volume", r.volume.as_ref());
    print_stat("face area", r.face_area.as_ref());
    print_stat("non-orthogonality", r.non_orthogonality.as_ref());
    print_stat("skewness", r.skewness.as_ref());
    println!("    unused points:    {}", r.unused_points);
    println!(
        "    thresholds:       non-orthogonality {NON_ORTH_THRESHOLD}°, skewness {SKEW_THRESHOLD}"
    );
    println!();

    let failed = r.failed_checks();
    if failed.is_empty() {
        println!("Mesh OK.");
        ExitCode::SUCCESS
    } else {
        println!("Failed {} mesh checks:", failed.len());
        for f in failed {
            println!("    {f}");
        }
        ExitCode::FAILURE
    }
}

fn print_stat(name: &str, s: Option<&Stat>) {
    match s {
        Some(s) => println!(
            "    {:<18}min {:e}  max {:e}  mean {:e}",
            format!("{name}:"),
            s.min,
            s.max,
            s.mean
        ),
        None => println!("    {:<18}none", format!("{name}:")),
    }
}
