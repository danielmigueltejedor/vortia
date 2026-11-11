//! The standard k-ε model with wall functions, as `simpleFoam` runs it.
//!
//! Derived from OpenFOAM v1912's `kEpsilon` (OpenFOAM, GPL-3.0-or-later),
//! each step in OpenFOAM's order and with its arithmetic:
//!
//! 1. `G = ν_t (∇U && dev(twoSymm(∇U)))` in every cell;
//! 2. the ε wall function replaces ε and `G` in the cells beside walls;
//! 3. the ε equation, relaxed, with those cells fixed, solved and bounded;
//! 4. the k equation, relaxed, solved and bounded;
//! 5. `ν_t = Cμ k²/ε`, and on the walls from the log law.

use vortia_core::Vec3;
use vortia_foamfile::Dictionary;
use vortia_fv::ops::surface_integrate;
use vortia_fv::stress::{grad, production};
use vortia_fv::vector::VectorBoundary;
use vortia_fv::{FvGeometry, Performance};
use vortia_mesh::PolyMesh;

use crate::case::{Case, KINEMATIC_DIFFUSIVITY, KINEMATIC_PRESSURE};
use crate::error::{Result, SolverError};
use crate::ico::switch_or;
use crate::rans::{
    EquationSetup, Kind, LogLaw, ModelField, bound, correct_nut_walls, face_diffusivity, near_wall,
    pow025, rhs, solve, transport, wall_function,
};

/// `[0 2 -3 0 0 0 0]`.
const DISSIPATION: [i32; 7] = [0, 2, -3, 0, 0, 0, 0];

/// The model's coefficients, `kEpsilonCoeffs` over OpenFOAM's defaults.
#[derive(Debug, Clone, Copy)]
struct Coeffs {
    cmu: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    sigmak: f64,
    sigma_eps: f64,
}

impl Coeffs {
    fn read(d: Option<&Dictionary>) -> Self {
        let get = |k: &str, def: f64| d.and_then(|d| d.scalar(k)).unwrap_or(def);
        Self {
            cmu: get("Cmu", 0.09),
            c1: get("C1", 1.44),
            c2: get("C2", 1.92),
            c3: get("C3", 0.0),
            sigmak: get("sigmak", 1.0),
            sigma_eps: get("sigmaEps", 1.3),
        }
    }
}

/// The k-ε model of a run: its fields and how it solves them.
pub(crate) struct KEpsilon {
    coeffs: Coeffs,
    nu: f64,
    k: ModelField,
    epsilon: ModelField,
    nut: ModelField,
    /// The distance to the wall of each face of each wall-function patch.
    y: Vec<Vec<f64>>,
    k_eqn: EquationSetup,
    epsilon_eqn: EquationSetup,
}

impl KEpsilon {
    /// Reads k, ε and ν_t from `dir` and sets ν_t as OpenFOAM's `validate`
    /// does before the first iteration.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn read(
        case: &Case,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        dir: &std::path::Path,
        coeffs: Option<&Dictionary>,
        nu: f64,
        relax_k: Option<f64>,
        relax_epsilon: Option<f64>,
    ) -> Result<Self> {
        let k = read_k(mesh, dir)?;
        let epsilon = ModelField::read(mesh, dir, "epsilon", DISSIPATION, |pf| {
            match pf.kind.as_str() {
                "fixedValue" => Some(Kind::Fixed(Vec::new())),
                "zeroGradient" => Some(Kind::ZeroGradient),
                "epsilonWallFunction" => switch_or(&pf.settings, "lowReCorrection", false, "")
                    .ok()
                    .map(Kind::EpsilonWall),
                _ => None,
            }
        })?;
        let nut = read_nut(mesh, dir, &epsilon, "epsilonWallFunction")?;
        let mut model = Self {
            coeffs: Coeffs::read(coeffs),
            nu,
            k,
            epsilon,
            y: near_wall(mesh, fv, &nut)?,
            nut,
            k_eqn: EquationSetup::read(case, "k", "DkEff", relax_k)?,
            epsilon_eqn: EquationSetup::read(case, "epsilon", "DepsilonEff", relax_epsilon)?,
        };
        let mut sink = |_: &str| {};
        bound(mesh, fv, "k", &mut model.k, &mut sink);
        bound(mesh, fv, "epsilon", &mut model.epsilon, &mut sink);
        model.correct_nut(mesh);
        Ok(model)
    }

    pub(crate) fn nut(&self) -> &ModelField {
        &self.nut
    }

    pub(crate) fn nu(&self) -> f64 {
        self.nu
    }

    pub(crate) fn fields(&self) -> [&ModelField; 3] {
        [&self.k, &self.epsilon, &self.nut]
    }

    /// `ν_t/σ + ν` linearly on every face.
    fn diffusivity(&self, mesh: &PolyMesh, fv: &FvGeometry, sigma: f64) -> Vec<f64> {
        let cells: Vec<f64> = self
            .nut
            .cells
            .iter()
            .map(|&t| t / sigma + self.nu)
            .collect();
        let boundary: Vec<Option<f64>> = self
            .nut
            .faces
            .iter()
            .map(|t| t.map(|t| t / sigma + self.nu))
            .collect();
        face_diffusivity(mesh, fv, &cells, &boundary)
    }

    /// `ν_t = Cμ k²/ε` in the cells and on calculated patches, and the log
    /// law on wall patches.
    fn correct_nut(&mut self, mesh: &PolyMesh) {
        let cmu = self.coeffs.cmu;
        let nut = |k: f64, e: f64| (cmu * (k * k)) / e;
        for ((t, &k), &e) in self
            .nut
            .cells
            .iter_mut()
            .zip(&self.k.cells)
            .zip(&self.epsilon.cells)
        {
            *t = nut(k, e);
        }
        calculated_faces(mesh, &mut self.nut, |b| {
            match (
                self.k.faces.get(b).copied().flatten(),
                self.epsilon.faces.get(b).copied().flatten(),
            ) {
                (Some(k), Some(e)) => Some(nut(k, e)),
                _ => None,
            }
        });
        correct_nut_walls(mesh, &mut self.nut, &self.k.cells, &self.y, self.nu);
    }

    /// One `turbulence->correct()`: the ε and k equations, then ν_t. Logs
    /// each solve and returns it.
    pub(crate) fn correct(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        u: &[Vec3],
        u_bc: &VectorBoundary,
        phi: &[f64],
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<(String, Performance)>> {
        let c = self.coeffs;
        let div_u = surface_integrate(mesh, fv, phi);
        let (grad_u, _) = grad(mesh, fv, u_bc, u);
        let g_by_nu: Vec<f64> = grad_u.iter().map(production).collect();
        let mut g: Vec<f64> = self
            .nut
            .cells
            .iter()
            .zip(&g_by_nu)
            .map(|(&t, &p)| t * p)
            .collect();

        let wall_cells = wall_function(
            mesh,
            fv,
            &mut self.epsilon,
            &self.nut,
            &self.k.cells,
            &self.y,
            self.nu,
            (u, u_bc),
            &mut g,
            epsilon_wall,
        );

        // ε: (div − laplacian) − (C1 GbyNu Cμ k − SuSp − Sp).
        let gamma = self.diffusivity(mesh, fv, c.sigma_eps);
        let mut eps_eqn = transport(mesh, fv, &self.epsilon, &self.epsilon_eqn, phi, &gamma)?;
        let su: Vec<f64> = g_by_nu
            .iter()
            .zip(&self.k.cells)
            .map(|(&p, &k)| ((c.c1 * p) * c.cmu) * k)
            .collect();
        let susp: Vec<f64> = div_u
            .iter()
            .map(|&d| ((2.0 / 3.0) * c.c1 - c.c3) * d)
            .collect();
        let sp: Vec<f64> = self
            .epsilon
            .cells
            .iter()
            .zip(&self.k.cells)
            .map(|(&e, &k)| (c.c2 * e) / k)
            .collect();
        eps_eqn.sub(&rhs(mesh, fv, &su, &susp, &sp, &self.epsilon.cells));
        let mut solves = vec![solve(
            mesh,
            eps_eqn,
            &mut self.epsilon,
            &self.epsilon_eqn,
            &wall_cells,
            "epsilon",
            log,
        )?];
        bound(mesh, fv, "epsilon", &mut self.epsilon, log);

        // k: (div − laplacian) − (G − SuSp − Sp).
        let gamma = self.diffusivity(mesh, fv, c.sigmak);
        let mut k_eqn = transport(mesh, fv, &self.k, &self.k_eqn, phi, &gamma)?;
        let susp: Vec<f64> = div_u.iter().map(|&d| (2.0 / 3.0) * d).collect();
        let sp: Vec<f64> = self
            .epsilon
            .cells
            .iter()
            .zip(&self.k.cells)
            .map(|(&e, &k)| e / k)
            .collect();
        k_eqn.sub(&rhs(mesh, fv, &g, &susp, &sp, &self.k.cells));
        solves.push(solve(mesh, k_eqn, &mut self.k, &self.k_eqn, &[], "k", log)?);
        bound(mesh, fv, "k", &mut self.k, log);

        self.correct_nut(mesh);
        Ok(solves)
    }
}

/// `epsilonWallFunction`'s ε and production for one wall face.
fn epsilon_wall(kind: &Kind, law: &LogLaw, f: &crate::rans::WallFace) -> (f64, f64) {
    let low_re = matches!(kind, Kind::EpsilonWall(true));
    let cmu25 = pow025(law.cmu);
    let cmu75 = law.cmu.powf(0.75);
    let y_plus = ((cmu25 * f.y) * f.k.sqrt()) / f.nuw;
    if low_re && y_plus < law.y_plus_lam {
        (((f.w * 2.0) * f.k) * f.nuw / (f.y * f.y), 0.0)
    } else {
        (
            ((f.w * cmu75) * f.k.powf(1.5)) / (law.kappa * f.y),
            ((((f.w * (f.nutw + f.nuw)) * f.mag_grad_uw) * cmu25) * f.k.sqrt()) / (law.kappa * f.y),
        )
    }
}

/// Reads k: fixed, zero-gradient or `kqRWallFunction` on each patch.
pub(crate) fn read_k(mesh: &PolyMesh, dir: &std::path::Path) -> Result<ModelField> {
    ModelField::read(mesh, dir, "k", KINEMATIC_PRESSURE, |pf| {
        match pf.kind.as_str() {
            "fixedValue" => Some(Kind::Fixed(Vec::new())),
            "zeroGradient" | "kqRWallFunction" => Some(Kind::ZeroGradient),
            _ => None,
        }
    })
}

/// Reads ν_t, calculated or `nutkWallFunction` on each patch, and checks
/// that each patch with `wall_function` in `second` has a ν_t wall function.
pub(crate) fn read_nut(
    mesh: &PolyMesh,
    dir: &std::path::Path,
    second: &ModelField,
    wall_function: &str,
) -> Result<ModelField> {
    let nut = ModelField::read(mesh, dir, "nut", KINEMATIC_DIFFUSIVITY, |pf| {
        match pf.kind.as_str() {
            "calculated" => Some(Kind::Calculated),
            "nutkWallFunction" => Some(Kind::NutWall(LogLaw::read(&pf.settings))),
            _ => None,
        }
    })?;
    for (patch, (sk, nk)) in mesh
        .patches()
        .iter()
        .zip(second.kinds.iter().zip(&nut.kinds))
    {
        if matches!(sk, Kind::EpsilonWall(_) | Kind::OmegaWall { .. })
            && !matches!(nk, Kind::NutWall(_))
        {
            return Err(SolverError::Setup(format!(
                "{wall_function} on '{}' needs a nut wall function there",
                patch.name
            )));
        }
    }
    Ok(nut)
}

/// Sets ν_t on each face of a calculated patch to `value` of its boundary
/// face index.
pub(crate) fn calculated_faces(
    mesh: &PolyMesh,
    nut: &mut ModelField,
    value: impl Fn(usize) -> Option<f64>,
) {
    let n_internal = mesh.n_internal_faces();
    for (patch, kind) in mesh.patches().iter().zip(&nut.kinds) {
        if *kind != Kind::Calculated {
            continue;
        }
        for f in patch.faces() {
            let b = f - n_internal;
            if let Some(v) = nut.faces.get_mut(b) {
                *v = value(b);
            }
        }
    }
}
