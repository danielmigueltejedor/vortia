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

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use vortia_core::Vec3;
use vortia_fields::{Field, FieldValue};
use vortia_foamfile::FoamFile;
use vortia_mesh::check::{NON_ORTH_THRESHOLD, SKEW_THRESHOLD, Stat};
use vortia_mesh::{MeshGeometry, PolyMesh, check};

const USAGE: &str = "usage: vortia check-mesh [CASE]\n\
    \x20      vortia fields [CASE [TIME]]\n\
    \x20      vortia solve [CASE]\n\n\
    check-mesh   read CASE/constant/polyMesh (default: the current directory)\n\
    \x20            and report its size, geometry and quality\n\
    fields       read every field of TIME (default: the latest), check it\n\
    \x20            fits the mesh and print its range\n\
    solve        run the solver controlDict names (laplacianFoam today)";

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
        ["solve"] => solve(Path::new(".")),
        ["solve", case] => solve(Path::new(case)),
        ["fields"] => fields(Path::new("."), None),
        ["fields", case] => fields(Path::new(case), None),
        ["fields", case, time] => fields(Path::new(case), Some(time)),
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
            eprintln!("vortia: {}", shown(&e.to_string()));
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
            shown(&p.name),
            shown(p.kind.name()),
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

fn fields(case: &Path, time: Option<&str>) -> ExitCode {
    let mesh = match PolyMesh::read_case(case) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("vortia: {}", shown(&e.to_string()));
            return ExitCode::FAILURE;
        }
    };
    let time = match time {
        Some(t) => t.to_owned(),
        None => match vortia_fields::latest(case) {
            Ok(Some(t)) => t.name,
            Ok(None) => {
                eprintln!("vortia: {} has no time directories", case.display());
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("vortia: {}", shown(&e.to_string()));
                return ExitCode::FAILURE;
            }
        },
    };
    let dir = case.join(&time);
    let mut entries: Vec<_> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect(),
        Err(e) => {
            eprintln!("vortia: {}: {e}", shown(&dir.display().to_string()));
            return ExitCode::FAILURE;
        }
    };
    entries.sort();
    println!("Time {}", shown(&time));
    let mut failed = false;
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = shown(name.strip_suffix(".gz").unwrap_or(&name));
        let class = FoamFile::open(&path)
            .and_then(|f| f.tokens().map(|(h, _)| h.map(|h| h.class)))
            .ok()
            .flatten()
            .unwrap_or_default();
        let line = match class.as_str() {
            "volScalarField" | "surfaceScalarField" => summary::<f64>(&path, &mesh, |v| v),
            "volVectorField" => summary::<Vec3>(&path, &mesh, Vec3::norm),
            _ => continue,
        };
        match line {
            Ok(text) => println!("    {name:<12} {class:<20} {text}"),
            Err(e) => {
                failed = true;
                println!(
                    "    {name:<12} {class:<20} error: {}",
                    shown(&e.to_string())
                );
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The range of a field's internal values; vectors by magnitude.
fn summary<T: FieldValue>(
    path: &Path,
    mesh: &PolyMesh,
    size: impl Fn(T) -> f64,
) -> Result<String, String> {
    let field = Field::<T>::read(path).map_err(|e| e.to_string())?;
    field.check_against(mesh).map_err(|e| e.to_string())?;
    let values = field
        .internal
        .expand(field.internal_len(mesh))
        .ok_or("internal values do not fit the mesh")?;
    let Some(stat) = Stat::of(values.into_iter().map(size)) else {
        return Ok("no values".into());
    };
    Ok(format!(
        "min {:e}  max {:e}  mean {:e}",
        stat.min, stat.max, stat.mean
    ))
}

fn solve(case: &Path) -> ExitCode {
    let result = vortia_solvers::Case::open(case).and_then(|c| {
        let mut out = std::io::stdout().lock();
        // A closed pipe (`| head`) is not a reason to stop the run.
        vortia_solvers::run(&c, |line| {
            let _ = out.write_all(shown(line).as_bytes());
        })
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vortia: {}", shown(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

/// Case-derived text with control characters escaped, so a crafted case
/// cannot drive the terminal (move the cursor, print a fake "Mesh OK.").
fn shown(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escaped: Vec<char> = if c.is_control() && c != '\n' {
                c.escape_default().collect()
            } else {
                vec![c]
            };
            escaped
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::shown;

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(shown("inlet"), "inlet");
        assert_eq!(shown("a\u{1b}[2Jb"), "a\\u{1b}[2Jb");
        assert_eq!(shown("line\n"), "line\n");
    }
}
