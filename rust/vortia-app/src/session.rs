//! A run of the engine on its own thread, reporting through a channel.
//!
//! The window never waits for the engine: it drains [`Session::poll`] once a frame.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use crate::convergence::Convergence;

/// What the engine's thread reports.
#[derive(Debug)]
pub enum Event {
    /// A line of the solver's log, without its line break.
    Log(String),
    /// A convergence report (the log line carries it too).
    Report(Convergence),
    /// The run is over.
    Finished(Outcome),
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// The user stopped it; what was written before stays.
    Stopped,
    Failed(String),
}

/// The stack the engine's thread gets: the solvers recurse a little over the mesh and the
/// default of a spawned thread is small.
const STACK: usize = 64 * 1024 * 1024;

pub struct Session {
    pub case: PathBuf,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    finished: bool,
    stop: Arc<AtomicBool>,
}

impl Session {
    /// Starts the solver the case's `controlDict` names. `wake` is called after every event,
    /// so the window can draw again.
    pub fn start(case: &Path, wake: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let (tx, events) = channel();
        let dir = case.to_path_buf();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("vortia-engine".into())
            .stack_size(STACK)
            .spawn(move || {
                let outcome = run(&dir, &tx, &wake, flag);
                let _ = tx.send(Event::Finished(outcome));
                wake();
            })?;
        Ok(Self {
            case: case.to_path_buf(),
            events,
            thread: Some(thread),
            finished: false,
            stop,
        })
    }

    /// Everything the thread has reported since the last call.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if matches!(event, Event::Finished(_)) {
                self.finished = true;
            }
            out.push(event);
        }
        if self.finished
            && let Some(thread) = self.thread.take()
        {
            let _ = thread.join();
        }
        out
    }

    /// Asks the engine to stop at the start of its next step.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

fn run(
    dir: &Path,
    tx: &Sender<Event>,
    wake: &(impl Fn() + Send),
    stop: Arc<AtomicBool>,
) -> Outcome {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let case = vortia_solvers::Case::open(dir)?;
        vortia_solvers::cancel::with_flag(stop, || {
            vortia_solvers::run(&case, |chunk| {
                for line in chunk.lines() {
                    if let Some(report) = Convergence::from_log_line(line) {
                        let _ = tx.send(Event::Report(report));
                    }
                    let _ = tx.send(Event::Log(line.to_owned()));
                }
                wake();
            })
        })
    }));
    match result {
        Ok(Ok(())) => Outcome::Done,
        Ok(Err(vortia_solvers::SolverError::Cancelled)) => Outcome::Stopped,
        Ok(Err(e)) => Outcome::Failed(e.to_string()),
        Err(_) => {
            Outcome::Failed("the engine stopped unexpectedly; the case was left as it was".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn drain(session: &mut Session) -> Vec<Event> {
        let start = Instant::now();
        let mut all = Vec::new();
        while !session.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(120), "the run hangs");
            all.extend(session.poll());
            std::thread::sleep(Duration::from_millis(10));
        }
        all
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_case_that_does_not_exist_ends_with_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::start(&dir.path().join("nothing"), || {}).unwrap();
        let events = drain(&mut s);
        assert!(matches!(
            events.last(),
            Some(Event::Finished(Outcome::Failed(_)))
        ));
    }

    #[test]
    fn a_solver_the_engine_lacks_is_said_by_name() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("system/controlDict"),
            "FoamFile { version 2.0; format ascii; class dictionary; object controlDict; }\n\
             application interFoam;\n",
        );
        for file in ["fvSchemes", "fvSolution"] {
            write(
                &dir.path().join("system").join(file),
                "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n",
            );
        }
        let mut s = Session::start(dir.path(), || {}).unwrap();
        let events = drain(&mut s);
        match events.last() {
            Some(Event::Finished(Outcome::Failed(message))) => {
                assert!(message.contains("interFoam"))
            }
            other => panic!("{other:?}"),
        }
    }

    /// A diffusion case on the nozzle mesh that runs `end` steps.
    fn diffusion_case(end: u32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let mesh_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/nozzle-practice-2026/constant/polyMesh");
        for f in ["points", "faces", "owner", "neighbour", "boundary"] {
            let to = d.join("constant/polyMesh").join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(mesh_src.join(f), to).unwrap();
        }
        let h = "FoamFile { version 2.0; format ascii; class dictionary; object x; }\n";
        write(
            &d.join("system/controlDict"),
            &format!(
                "{h}application laplacianFoam; startFrom startTime; startTime 0; \
                 endTime {end}; deltaT 1; writeControl timeStep; writeInterval 1;"
            ),
        );
        write(
            &d.join("system/fvSchemes"),
            &format!(
                "{h}ddtSchemes {{ default steadyState; }} gradSchemes {{ default Gauss linear; }} \
                 laplacianSchemes {{ default Gauss linear corrected; }}"
            ),
        );
        write(
            &d.join("system/fvSolution"),
            &format!(
                "{h}solvers {{ T {{ solver PCG; preconditioner DIC; tolerance 1e-8; relTol 0; }} }} \
                 SIMPLE {{ nNonOrthogonalCorrectors 0; }}"
            ),
        );
        write(
            &d.join("constant/transportProperties"),
            &format!("{h}DT DT [0 2 -1 0 0 0 0] 4e-05;"),
        );
        write(
            &d.join("0/T"),
            "FoamFile { version 2.0; format ascii; class volScalarField; object T; }\n\
             dimensions [0 0 0 1 0 0 0]; internalField uniform 300;\n\
             boundaryField {\n\
               axis { type empty; }\n\
               inlet { type fixedValue; value uniform 400; }\n\
               walls { type fixedGradient; gradient uniform 1000; value uniform 0; }\n\
               atmosphere { type fixedValue; value uniform 300; }\n\
               front { type wedge; } back { type wedge; }\n\
             }\n",
        );
        dir
    }

    #[test]
    fn a_run_streams_its_log_and_then_finishes_well() {
        let dir = diffusion_case(1);
        let d = dir.path();
        let mut s = Session::start(d, || {}).unwrap();
        let events = drain(&mut s);
        assert!(matches!(
            events.last(),
            Some(Event::Finished(Outcome::Done))
        ));
        let lines: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                Event::Log(l) => Some(l.as_str()),
                _ => None,
            })
            .collect();
        assert!(lines.iter().any(|l| l.contains("Time =")), "{lines:?}");
        assert!(lines.iter().all(|l| !l.contains('\n')));
        assert!(d.join("1/T").exists());
    }

    #[test]
    fn a_run_can_be_stopped_and_says_it_was() {
        let dir = diffusion_case(50);
        let mut s = Session::start(dir.path(), || {}).unwrap();
        s.stop();
        let events = drain(&mut s);
        assert!(matches!(
            events.last(),
            Some(Event::Finished(Outcome::Stopped))
        ));
        // Stopped before its first step: nothing was written.
        assert!(!dir.path().join("1").exists());
    }
}
