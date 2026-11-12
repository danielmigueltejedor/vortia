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

/// `println!` that ignores a closed stdout: `vortia fields | head` must not
/// panic once `head` has what it wants.
macro_rules! out {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

const USAGE: &str = "usage: vortia check-mesh [CASE]\n\
    \x20      vortia fields [CASE [TIME]]\n\
    \x20      vortia solve [CASE]\n\
    \x20      vortia blockmesh [CASE]\n\
    \x20      vortia gmshtofoam [-keepOrientation] FILE [CASE]\n\
    \x20      vortia fluenttofoam [-scale F] [-2D T] [-writeZones] FILE [CASE]\n\n\
    check-mesh   read CASE/constant/polyMesh (default: the current directory)\n\
    \x20            and report its size, geometry and quality\n\
    fields       read every field of TIME (default: the latest), check it\n\
    \x20            fits the mesh and print its range\n\
    solve        run the solver controlDict names (laplacianFoam today)\n\
    blockmesh    mesh CASE/system/blockMeshDict into CASE/constant/polyMesh\n\
    gmshtofoam   convert a Gmsh .msh file into CASE/constant/polyMesh\n\
    fluenttofoam convert a Fluent .msh file into CASE/constant/polyMesh";

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
        ["blockmesh"] => blockmesh(Path::new(".")),
        ["blockmesh", case] => blockmesh(Path::new(case)),
        ["gmshtofoam", "-keepOrientation", file] => {
            gmshtofoam(Path::new(file), Path::new("."), true)
        }
        ["gmshtofoam", "-keepOrientation", file, case] => {
            gmshtofoam(Path::new(file), Path::new(case), true)
        }
        ["gmshtofoam", file] => gmshtofoam(Path::new(file), Path::new("."), false),
        ["gmshtofoam", file, case] => gmshtofoam(Path::new(file), Path::new(case), false),
        ["fluenttofoam", rest @ ..] => fluenttofoam(rest),
        ["solve"] => solve(Path::new(".")),
        ["solve", case] => solve(Path::new(case)),
        ["fields"] => fields(Path::new("."), None),
        ["fields", case] => fields(Path::new(case), None),
        ["fields", case, time] => fields(Path::new(case), Some(time)),
        ["-h" | "--help" | "help"] => {
            out!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn gmshtofoam(file: &Path, case: &Path, keep_orientation: bool) -> ExitCode {
    let result = std::fs::read(file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))
        .and_then(|bytes| {
            vortia_mesh::gmsh::gmsh_to_foam(&String::from_utf8_lossy(&bytes), keep_orientation)
                .map_err(|e| e.to_string())
        })
        .and_then(|mesh| {
            mesh.write_case(case)
                .map(|()| mesh)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(mesh) => {
            out!(
                "Wrote {} points, {} faces ({} internal) and {} cells",
                mesh.points.len(),
                mesh.faces.len(),
                mesh.neighbour.len(),
                mesh.owner.iter().max().map_or(0, |m| m + 1)
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vortia: {}", shown(&e));
            ExitCode::FAILURE
        }
    }
}

fn fluenttofoam(args: &[&str]) -> ExitCode {
    let mut options = vortia_mesh::fluent::Options::default();
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        let value =
            |it: &mut std::slice::Iter<'_, &str>| it.next().and_then(|v| v.parse::<f64>().ok());
        match arg {
            "-scale" => match value(&mut it) {
                Some(v) => options.scale = v,
                None => return usage_error("-scale needs a number"),
            },
            "-2D" => match value(&mut it) {
                Some(v) => options.thickness = Some(v),
                None => return usage_error("-2D needs a number"),
            },
            "-writeZones" => options.write_zones = true,
            other => files.push(other),
        }
    }
    let (file, case) = match files.as_slice() {
        [file] => (*file, "."),
        [file, case] => (*file, *case),
        _ => return usage_error("fluenttofoam takes a file and, optionally, a case"),
    };
    let result = std::fs::read(file)
        .map_err(|e| format!("cannot read {file}: {e}"))
        .and_then(|bytes| {
            vortia_mesh::fluent::fluent_to_foam(&String::from_utf8_lossy(&bytes), options)
                .map_err(|e| e.to_string())
        })
        .and_then(|mesh| {
            mesh.write_case(Path::new(case))
                .map(|()| mesh)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(mesh) => {
            out!(
                "Wrote {} points, {} faces ({} internal) and {} cells",
                mesh.points.len(),
                mesh.faces.len(),
                mesh.neighbour.len(),
                mesh.owner.iter().max().map_or(0, |m| m + 1)
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vortia: {}", shown(&e));
            ExitCode::FAILURE
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("vortia: {message}\n{USAGE}");
    ExitCode::from(2)
}

fn blockmesh(case: &Path) -> ExitCode {
    let result = FoamFile::open(case.join("system").join("blockMeshDict"))
        .and_then(|f| f.dictionary())
        .map_err(|e| e.to_string())
        .and_then(|(_, dict)| vortia_mesh::blockmesh::block_mesh(&dict).map_err(|e| e.to_string()))
        .and_then(|mesh| {
            mesh.write_case(case)
                .map(|()| mesh)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(mesh) => {
            out!(
                "Wrote {} points, {} faces ({} internal) and {} cells",
                mesh.points.len(),
                mesh.faces.len(),
                mesh.neighbour.len(),
                mesh.owner.iter().max().map_or(0, |m| m + 1)
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vortia: {}", shown(&e));
            ExitCode::FAILURE
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

    out!("Mesh stats");
    out!("    points:           {}", r.n_points);
    out!("    faces:            {}", r.n_faces);
    out!("    internal faces:   {}", r.n_internal_faces);
    out!("    cells:            {}", r.n_cells);
    out!();
    out!("Patches");
    for p in mesh.patches() {
        out!(
            "    {:<20} {:<14} {:>8} faces",
            shown(&p.name),
            shown(p.kind.name()),
            p.size
        );
    }
    out!();
    out!("Geometry");
    let b = r.bounds;
    out!(
        "    bounding box:     ({} {} {}) ({} {} {})",
        b.min.x,
        b.min.y,
        b.min.z,
        b.max.x,
        b.max.y,
        b.max.z
    );
    out!("    total volume:     {:e}", r.total_volume);
    print_stat("cell volume", r.volume.as_ref());
    print_stat("face area", r.face_area.as_ref());
    print_stat("non-orthogonality", r.non_orthogonality.as_ref());
    print_stat("skewness", r.skewness.as_ref());
    out!("    unused points:    {}", r.unused_points);
    out!(
        "    thresholds:       non-orthogonality {NON_ORTH_THRESHOLD}°, skewness {SKEW_THRESHOLD}"
    );
    out!();

    let failed = r.failed_checks();
    if failed.is_empty() {
        out!("Mesh OK.");
        ExitCode::SUCCESS
    } else {
        out!("Failed {} mesh checks:", failed.len());
        for f in failed {
            out!("    {f}");
        }
        ExitCode::FAILURE
    }
}

fn print_stat(name: &str, s: Option<&Stat>) {
    match s {
        Some(s) => out!(
            "    {:<18}min {:e}  max {:e}  mean {:e}",
            format!("{name}:"),
            s.min,
            s.max,
            s.mean
        ),
        None => out!("    {:<18}none", format!("{name}:")),
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
    out!("Time {}", shown(&time));
    let mut failed = false;
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = shown(name.strip_suffix(".gz").unwrap_or(&name));
        // A file without a header is not a field; one whose header does
        // not read is a field that was not checked, which is a failure.
        let class = match FoamFile::open(&path).and_then(|f| f.tokens().map(|(h, _)| h)) {
            Ok(header) => header.map(|h| h.class).unwrap_or_default(),
            Err(e) => {
                failed = true;
                out!("    {name:<12} error: {}", shown(&e.to_string()));
                continue;
            }
        };
        let line = match class.as_str() {
            "volScalarField" | "surfaceScalarField" => summary::<f64>(&path, &mesh, |v| v),
            "volVectorField" => summary::<Vec3>(&path, &mesh, Vec3::norm),
            _ => continue,
        };
        match line {
            Ok(text) => out!("    {name:<12} {class:<20} {text}"),
            Err(e) => {
                failed = true;
                out!(
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
            let _ = out.write_all(shown_line(line).as_bytes());
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

/// Case-derived text with control characters escaped, newlines included, so
/// a crafted case cannot drive the terminal or print a fake "Mesh OK." line.
fn shown(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escaped: Vec<char> = if c.is_control() {
                c.escape_default().collect()
            } else {
                vec![c]
            };
            escaped
        })
        .collect()
}

/// A solver log line: escaped like [`shown`], keeping the newlines the
/// solver put around it (`Time = 1\n\n`, `\nSIMPLE solution converged…`).
fn shown_line(line: &str) -> String {
    let body = line.trim_matches('\n');
    let lead = line.len() - line.trim_start_matches('\n').len();
    let trail = line.len() - line.trim_end_matches('\n').len();
    if body.is_empty() {
        return "\n".repeat(line.len());
    }
    format!("{}{}{}", "\n".repeat(lead), shown(body), "\n".repeat(trail))
}

#[cfg(test)]
mod tests {
    use super::{shown, shown_line};

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(shown("inlet"), "inlet");
        assert_eq!(shown("a\u{1b}[2Jb"), "a\\u{1b}[2Jb");
        // A newline in a patch type must not start a line of its own.
        assert_eq!(shown("wall\nMesh OK."), "wall\\nMesh OK.");
        assert_eq!(shown_line("Time = 1\n"), "Time = 1\n");
        assert_eq!(shown_line("a\nMesh OK.\n"), "a\\nMesh OK.\n");
        assert_eq!(shown_line("Time = 1\n\n"), "Time = 1\n\n");
        assert_eq!(shown_line("\nEnd\n"), "\nEnd\n");
    }
}
