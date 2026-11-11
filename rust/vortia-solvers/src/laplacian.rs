//! Diffusion of a scalar: `∂T/∂t = ∇·(D_T ∇T)`, steady or transient.
//!
//! The engine's counterpart of OpenFOAM's `laplacianFoam` (OpenFOAM,
//! GPL-3.0-or-later), reading the same case: `T` in the start time,
//! `DT` in `constant/transportProperties`, the time loop in `controlDict`, the
//! schemes and solver in `system/`. It writes `T` to the same time
//! directories, so the two runs can be compared file by file.

use vortia_fields::{Data, ScalarField};
use vortia_fv::ops::{self, SnGrad};
use vortia_fv::{Boundary, LduMatrix, Performance};

use crate::case::{Case, Dims, KINEMATIC_DIFFUSIVITY, check_dimensions, read_dict};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub time: String,
    pub solves: Vec<Performance>,
    pub written: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ddt {
    Steady,
    Euler,
}

impl Ddt {
    /// The `ddtSchemes` entry for `key` (`ddt(T)`).
    pub(crate) fn read(case: &Case, key: &str) -> Result<Self> {
        match case.scheme("ddtSchemes", key)?.first().map(String::as_str) {
            Some("steadyState") => Ok(Self::Steady),
            Some("Euler") => Ok(Self::Euler),
            other => Err(SolverError::Setup(format!(
                "ddt scheme '{}' is not supported yet",
                other.unwrap_or("")
            ))),
        }
    }
}

/// The surface-normal gradient of `laplacianSchemes` for `key`, checked
/// against the gradient its correction needs.
pub(crate) fn sn_grad(case: &Case, key: &str, field: &str) -> Result<SnGrad> {
    let words = case.scheme("laplacianSchemes", key)?;
    let sn_grad = SnGrad::from_scheme(&words.iter().map(String::as_str).collect::<Vec<_>>())?;
    if sn_grad == SnGrad::Corrected {
        // The correction interpolates the cell gradient; the engine has Gauss
        // linear only, so any other choice must not silently become it.
        let grad = case.scheme("gradSchemes", &format!("grad({field})"))?;
        if grad != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "grad scheme '{}' is not supported yet; use Gauss linear",
                grad.join(" ")
            )));
        }
    }
    Ok(sn_grad)
}

pub(crate) fn diffusivity(case: &Case) -> Result<f64> {
    transport_property(case, "DT", KINEMATIC_DIFFUSIVITY)
}

/// A non-negative constant of `constant/transportProperties`, whose
/// dimensions, when it gives them, must be `dims`.
pub(crate) fn transport_property(case: &Case, key: &str, dims: Dims) -> Result<f64> {
    let props = read_dict(&case.dir.join("constant").join("transportProperties"))?;
    let invalid = || SolverError::Setup(format!("transportProperties has no valid '{key}'"));
    // `DT 4e-05;`, `DT [0 2 -1 0 0 0 0] 4e-05;` and `DT DT [...] 4e-05;` all
    // end in the number.
    let values = props.values(key).ok_or_else(invalid)?;
    if let Some(d) = values
        .iter()
        .find(|v| matches!(v, vortia_foamfile::Value::Dimensions(_)))
    {
        check_dimensions(key, d, dims)?;
    }
    values
        .last()
        .and_then(|v| v.as_scalar())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or_else(invalid)
}

/// Runs the case and returns what each time step did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    let dt_coeff = diffusivity(case)?;

    let ddt = Ddt::read(case, "ddt(T)")?;
    let sn_grad = sn_grad(case, "laplacian(DT,T)", "T")?;
    let (solver, controls) = LinearSolver::read(case, "T")?;
    let n_corr = case.non_orthogonal_correctors("SIMPLE");
    let run = RunControl::read(case)?;
    let mut field = ScalarField::read(case.dir.join(&run.start_name).join("T"))?;
    let bc = Boundary::from_field(&field, &mesh)?;
    let mut t = field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("T does not fit the mesh".into()))?;

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        let name = run.name(step);
        log(&format!("Time = {name}\n"));
        let old = t.clone();
        let mut solves = Vec::new();
        for _ in 0..=n_corr {
            let mut m = LduMatrix::new(&mesh);
            if ddt == Ddt::Euler {
                ops::ddt_euler(&mut m, &fv, &old, run.delta_t)?;
            }
            ops::diffusion(&mut m, &mesh, &fv, &bc, dt_coeff, sn_grad, &t)?;
            let perf = solver.solve(&m, &mut t, &controls)?;
            log(&solve_line("T", &perf));
            solves.push(perf);
        }
        let written = run.writes(step);
        if written {
            field.internal = Data::NonUniform(t.clone());
            field.drop_zero_gradient_values();
            field.write(&case.dir, &name)?;
        }
        steps.push(Step {
            time: name,
            solves,
            written,
        });
    }
    log("End\n");
    Ok(steps)
}
