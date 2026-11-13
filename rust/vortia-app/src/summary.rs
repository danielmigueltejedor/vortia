//! What a case folder holds, read for the case screen: the run's settings, the mesh and its
//! patches, the fields and the times written. The mesh can be large, so [`spawn`] reads it on
//! a thread of its own.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use vortia_foamfile::RawPolyMesh;

/// How many field files and time directories are listed; a case can hold thousands of times.
const MAX_LISTED: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub name: String,
    /// `patch`, `wall`, `empty`, `wedge`, …
    pub kind: String,
    pub faces: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshInfo {
    pub points: usize,
    pub faces: usize,
    pub cells: usize,
    pub patches: Vec<Patch>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Summary {
    pub application: Option<String>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub delta_t: Option<f64>,
    pub write_interval: Option<f64>,
    /// The mesh, or why it could not be read.
    pub mesh: Option<Result<MeshInfo, String>>,
    /// The time directories written, in time order (at most [`MAX_LISTED`], the last ones).
    pub times: Vec<String>,
    /// The field files of the latest time (the `0` directory when nothing was written).
    pub fields: Vec<String>,
}

/// Reads the case; whatever is missing stays `None` or empty, never an error.
pub fn read(dir: &Path) -> Summary {
    let mut summary = Summary::default();
    if let Ok(control) = vortia_solvers::case::read_dict(&dir.join("system").join("controlDict")) {
        summary.application = control.word("application").map(str::to_owned);
        summary.start_time = control.scalar("startTime");
        summary.end_time = control.scalar("endTime");
        summary.delta_t = control.scalar("deltaT");
        summary.write_interval = control.scalar("writeInterval");
    }
    let mut times = time_directories(dir);
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    let skip = times.len().saturating_sub(MAX_LISTED);
    summary.times = times
        .iter()
        .skip(skip)
        .map(|(_, name)| name.clone())
        .collect();
    let latest = times
        .iter()
        .rfind(|(t, _)| *t > 0.0)
        .map(|(_, name)| name.as_str())
        .unwrap_or("0");
    summary.fields = files_in(&dir.join(latest));
    if dir.join("constant").join("polyMesh").is_dir() {
        summary.mesh = Some(read_mesh(dir));
    }
    summary
}

fn read_mesh(dir: &Path) -> Result<MeshInfo, String> {
    let mesh = RawPolyMesh::read_case(dir).map_err(|e| e.to_string())?;
    let highest = mesh
        .owner
        .iter()
        .chain(mesh.neighbour.iter())
        .max()
        .copied();
    Ok(MeshInfo {
        points: mesh.points.len(),
        faces: mesh.faces.len(),
        cells: highest.map_or(0, |c| c.saturating_add(1)),
        patches: mesh
            .patches
            .iter()
            .map(|p| Patch {
                name: p.name.clone(),
                kind: p.kind.clone(),
                faces: p.n_faces,
            })
            .collect(),
    })
}

/// The sub-folders named by a finite number, with their value.
fn time_directories(dir: &Path) -> Vec<(f64, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let value = name.parse::<f64>().ok().filter(|v| v.is_finite())?;
            Some((value, name))
        })
        .collect()
}

fn files_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names.truncate(MAX_LISTED);
    names
}

/// Reads the case on a thread; `wake` is called when the answer is ready.
pub fn spawn(dir: PathBuf, wake: impl Fn() + Send + 'static) -> Receiver<Summary> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("vortia-summary".into())
        .spawn(move || {
            let summary = std::panic::catch_unwind(|| read(&dir)).unwrap_or_default();
            let _ = tx.send(summary);
            wake();
        });
    // No thread, no summary: the case screen shows what it has.
    drop(spawned);
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn an_empty_folder_is_an_empty_summary() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), Summary::default());
    }

    #[test]
    fn the_settings_times_and_fields_of_a_case_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(
            &d.join("system/controlDict"),
            "FoamFile { version 2.0; format ascii; class dictionary; object controlDict; }\n\
             application rhoCentralFoam; startTime 0; endTime 0.5; deltaT 1e-5; writeInterval 0.1;",
        );
        for t in ["0", "0.2", "0.1", "1e-05", "notATime"] {
            write(&d.join(t).join("p"), "x");
        }
        write(&d.join("0.2/T"), "x");
        write(&d.join("0.2/.hidden"), "x");
        let s = read(d);
        assert_eq!(s.application.as_deref(), Some("rhoCentralFoam"));
        assert_eq!(s.end_time, Some(0.5));
        assert_eq!(s.delta_t, Some(1e-5));
        assert_eq!(s.times, ["0", "1e-05", "0.1", "0.2"]);
        assert_eq!(s.fields, ["T", "p"]);
        assert_eq!(s.mesh, None);
    }

    #[test]
    fn the_nozzle_mesh_reports_its_cells_and_patches() {
        let case =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/nozzle-practice-2026");
        let Some(Ok(mesh)) = read(&case).mesh else {
            panic!("the nozzle mesh did not read");
        };
        assert_eq!(mesh.cells, 7500);
        assert!(mesh.patches.iter().any(|p| p.name == "inlet"));
        assert!(mesh.points > 0 && mesh.faces > mesh.cells);
    }

    #[test]
    fn a_broken_mesh_says_why_and_the_rest_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("constant/polyMesh/points"), "garbage");
        let s = read(dir.path());
        assert!(matches!(s.mesh, Some(Err(_))));
    }

    #[test]
    fn the_thread_answers_through_the_channel() {
        let dir = tempfile::tempdir().unwrap();
        let rx = spawn(dir.path().to_path_buf(), || {});
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap(),
            Summary::default()
        );
    }
}
