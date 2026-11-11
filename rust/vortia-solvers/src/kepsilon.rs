//! The standard k-ε model with wall functions, as `simpleFoam` runs it.
//!
//! Derived from OpenFOAM v1912's `kEpsilon`, `epsilonWallFunction`,
//! `nutkWallFunction`, `kqRWallFunction` and `bound` (OpenFOAM,
//! GPL-3.0-or-later). Each step is taken in OpenFOAM's order and with its
//! arithmetic, so the fields agree with OpenFOAM's to rounding:
//!
//! 1. `G = ν_t (∇U && dev(twoSymm(∇U)))` in every cell;
//! 2. the ε wall function replaces ε and `G` in the cells beside walls;
//! 3. the ε equation, relaxed, with those cells fixed, solved and bounded;
//! 4. the k equation, relaxed, solved and bounded;
//! 5. `ν_t = Cμ k²/ε`, and on the walls from the log law.

use vortia_core::Vec3;
use vortia_fields::{Data, PatchField, ScalarField};
use vortia_foamfile::Dictionary;
use vortia_fv::ops::{Convection, SnGrad, surface_integrate};
use vortia_fv::stress::{grad, production};
use vortia_fv::vector::{VectorBc, VectorBoundary};
use vortia_fv::{Boundary, FvGeometry, Performance, ScalarBc, ScalarMatrix};
use vortia_mesh::{PatchKind, PolyMesh, wall::near_wall_distance};

use crate::case::{Case, KINEMATIC_DIFFUSIVITY, KINEMATIC_PRESSURE, check_dimensions, read_dict};
use crate::control::{LinearSolver, solve_line};
use crate::error::{Result, SolverError};

/// `[0 2 -3 0 0 0 0]`.
const DISSIPATION: [i32; 7] = [0, 2, -3, 0, 0, 0, 0];

/// OpenFOAM's `SMALL`, the floor `bound` holds k and ε to.
const SMALL: f64 = 1.0e-15;

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

/// The log law a wall patch uses: `nutkWallFunction`'s coefficients and the
/// `y⁺` where the laminar and log layers meet.
#[derive(Debug, Clone, Copy)]
struct LogLaw {
    cmu: f64,
    kappa: f64,
    e: f64,
    y_plus_lam: f64,
}

impl LogLaw {
    fn read(settings: &Dictionary) -> Self {
        let get = |k: &str, def: f64| settings.scalar(k).unwrap_or(def);
        let (cmu, kappa, e) = (get("Cmu", 0.09), get("kappa", 0.41), get("E", 9.8));
        let mut ypl = 11.0_f64;
        for _ in 0..10 {
            ypl = (e * ypl).max(1.0).ln() / kappa;
        }
        Self {
            cmu,
            kappa,
            e,
            y_plus_lam: ypl,
        }
    }
}

/// How each patch treats one of the model's fields.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Fixed(Vec<f64>),
    /// `zeroGradient`, and `kqRWallFunction` for k.
    ZeroGradient,
    Empty,
    /// `calculated` ν_t: from k and ε on the patch.
    Calculated,
    /// `epsilonWallFunction`; `true` with `lowReCorrection`.
    EpsilonWall(bool),
    /// `nutkWallFunction`.
    NutWall(LogLaw),
}

impl PartialEq for LogLaw {
    fn eq(&self, other: &Self) -> bool {
        self.cmu == other.cmu && self.kappa == other.kappa && self.e == other.e
    }
}

/// A field of the model: its cells, its value on every boundary face
/// (`None` on empty faces) and its conditions, with the file it came from.
struct ModelField {
    file: ScalarField,
    kinds: Vec<Kind>,
    cells: Vec<f64>,
    faces: Vec<Option<f64>>,
}

impl ModelField {
    fn read(
        mesh: &PolyMesh,
        dir: &std::path::Path,
        name: &str,
        dims: [i32; 7],
        kind_of: impl Fn(&PatchField<f64>) -> Option<Kind>,
    ) -> Result<Self> {
        let file = ScalarField::read(dir.join(name))?;
        check_dimensions(name, &file.dimensions, dims)?;
        vortia_fv::bc::check_solvable(&file)?;
        file.check_against(mesh)?;
        let cells = file
            .internal
            .expand(mesh.n_cells())
            .ok_or_else(|| SolverError::Setup(format!("{name} does not fit the mesh")))?;
        let mut kinds = Vec::with_capacity(mesh.patches().len());
        let mut faces = Vec::new();
        for patch in mesh.patches() {
            let pf = file.patch_field(patch).ok_or_else(|| {
                SolverError::Setup(format!("{name}: no condition for patch '{}'", patch.name))
            })?;
            let mut kind = if pf.kind == "empty" {
                Kind::Empty
            } else {
                kind_of(pf).ok_or_else(|| {
                    SolverError::Setup(format!(
                        "{name}: '{}' on patch '{}' is not supported yet",
                        pf.kind, patch.name
                    ))
                })?
            };
            if (patch.kind == PatchKind::Empty) != (kind == Kind::Empty) {
                return Err(SolverError::Setup(format!(
                    "{name}: patch '{}' is {} but its condition is '{}'",
                    patch.name,
                    patch.kind.name(),
                    pf.kind
                )));
            }
            let given = pf.value.as_ref().and_then(|d| d.expand(patch.size));
            if matches!(kind, Kind::Fixed(_)) {
                let v = given.clone().ok_or_else(|| {
                    SolverError::Setup(format!(
                        "{name}: patch '{}' needs a 'value' of {} items",
                        patch.name, patch.size
                    ))
                })?;
                kind = Kind::Fixed(v);
            }
            for (i, f) in patch.faces().enumerate() {
                let inside = mesh.owner().get(f).and_then(|&c| cells.get(c)).copied();
                faces.push(match &kind {
                    Kind::Empty => None,
                    Kind::Fixed(v) => v.get(i).copied(),
                    Kind::ZeroGradient => inside,
                    _ => given.as_ref().and_then(|v| v.get(i).copied()).or(inside),
                });
            }
            kinds.push(kind);
        }
        Ok(Self {
            file,
            kinds,
            cells,
            faces,
        })
    }

    /// The conditions an implicit equation in this field sees; the ε wall
    /// function is a fixed value of what it last set.
    fn boundary(&self, mesh: &PolyMesh) -> Boundary {
        let n_internal = mesh.n_internal_faces();
        let conditions = mesh
            .patches()
            .iter()
            .zip(&self.kinds)
            .map(|(patch, kind)| match kind {
                Kind::Fixed(v) => ScalarBc::FixedValue(v.clone()),
                Kind::ZeroGradient => ScalarBc::ZeroGradient,
                Kind::Empty => ScalarBc::Empty,
                Kind::EpsilonWall(_) | Kind::Calculated | Kind::NutWall(_) => ScalarBc::FixedValue(
                    patch
                        .faces()
                        .map(|f| {
                            self.faces
                                .get(f - n_internal)
                                .copied()
                                .flatten()
                                .unwrap_or(0.0)
                        })
                        .collect(),
                ),
            })
            .collect();
        Boundary { conditions }
    }

    /// `correctBoundaryConditions` for the conditions that follow the cells.
    fn follow_cells(&mut self, mesh: &PolyMesh) {
        let n_internal = mesh.n_internal_faces();
        for (patch, kind) in mesh.patches().iter().zip(&self.kinds) {
            if *kind != Kind::ZeroGradient {
                continue;
            }
            for f in patch.faces() {
                let inside = mesh
                    .owner()
                    .get(f)
                    .and_then(|&c| self.cells.get(c))
                    .copied();
                if let Some(v) = self.faces.get_mut(f - n_internal) {
                    *v = inside;
                }
            }
        }
    }

    /// The field as OpenFOAM writes it: each patch by name, with the value
    /// its condition writes.
    fn to_write(&self, mesh: &PolyMesh) -> ScalarField {
        let n_internal = mesh.n_internal_faces();
        let mut out = self.file.clone();
        out.internal = Data::NonUniform(self.cells.clone());
        out.boundary = mesh
            .patches()
            .iter()
            .zip(&self.kinds)
            .filter_map(|(patch, kind)| {
                let mut pf = self.file.patch_field(patch)?.clone();
                pf.key = patch.name.clone();
                pf.value = match kind {
                    Kind::ZeroGradient if pf.kind == "zeroGradient" => None,
                    Kind::Empty | Kind::Fixed(_) => pf.value,
                    _ => Some(Data::NonUniform(
                        patch
                            .faces()
                            .map(|f| {
                                self.faces
                                    .get(f - n_internal)
                                    .copied()
                                    .flatten()
                                    .unwrap_or(0.0)
                            })
                            .collect(),
                    )),
                };
                Some(pf)
            })
            .collect();
        out
    }
}

/// How one of the model's equations is discretised and solved.
struct EquationSetup {
    convection: Convection,
    sn_grad: SnGrad,
    solver: LinearSolver,
    controls: vortia_fv::Controls,
    relax: Option<f64>,
}

impl EquationSetup {
    fn read(case: &Case, field: &str, diffusivity: &str, relax: Option<f64>) -> Result<Self> {
        let convection = case.convection(&format!("div(phi,{field})"))?;
        let sn_grad = case.uncorrected_sn_grad(&format!("laplacian({diffusivity},{field})"))?;
        let (solver, controls) = LinearSolver::read(case, field)?;
        Ok(Self {
            convection,
            sn_grad,
            solver,
            controls,
            relax,
        })
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

/// What `constant/turbulenceProperties` selects.
pub(crate) enum Model {
    Laminar,
    KEpsilon(Option<Dictionary>),
}

impl Model {
    /// Reads the model, refusing what the engine does not have.
    pub(crate) fn read(case: &Case) -> Result<Self> {
        let constant = case.dir.join("constant");
        let path = constant.join("turbulenceProperties");
        if !path.is_file() && !constant.join("turbulenceProperties.gz").is_file() {
            return Ok(Self::Laminar);
        }
        let d = read_dict(&path)?;
        match d.word("simulationType") {
            Some("laminar") => {
                if let Some(model) = d.dict("laminar").and_then(|l| l.word("laminarModel"))
                    && model != "Stokes"
                {
                    return Err(SolverError::Setup(format!(
                        "laminar model '{model}' is not supported yet; use Stokes"
                    )));
                }
                Ok(Self::Laminar)
            }
            Some("RAS") => {
                let ras = d.dict("RAS").cloned().unwrap_or_default();
                let model = ras.word("RASModel").or_else(|| ras.word("model"));
                if model != Some("kEpsilon") {
                    return Err(SolverError::Setup(format!(
                        "RAS model '{}' is not supported yet; use kEpsilon",
                        model.unwrap_or("")
                    )));
                }
                if matches!(
                    ras.word("turbulence"),
                    Some("off" | "false" | "no" | "n" | "none")
                ) {
                    return Err(SolverError::Setup(
                        "RAS with 'turbulence off' is not supported yet".into(),
                    ));
                }
                Ok(Self::KEpsilon(ras.dict("kEpsilonCoeffs").cloned()))
            }
            other => Err(SolverError::Setup(format!(
                "simulationType '{}' is not supported yet; use laminar or RAS",
                other.unwrap_or("")
            ))),
        }
    }
}

/// Square root of a square root, as OpenFOAM's `pow025`.
fn pow025(x: f64) -> f64 {
    x.sqrt().sqrt()
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
        let k = ModelField::read(mesh, dir, "k", KINEMATIC_PRESSURE, |pf| {
            match pf.kind.as_str() {
                "fixedValue" => Some(Kind::Fixed(Vec::new())),
                "zeroGradient" | "kqRWallFunction" => Some(Kind::ZeroGradient),
                _ => None,
            }
        })?;
        let epsilon = ModelField::read(mesh, dir, "epsilon", DISSIPATION, |pf| {
            match pf.kind.as_str() {
                "fixedValue" => Some(Kind::Fixed(Vec::new())),
                "zeroGradient" => Some(Kind::ZeroGradient),
                "epsilonWallFunction" => match pf.settings.word("lowReCorrection") {
                    None => Some(Kind::EpsilonWall(false)),
                    Some(w) => switch(w).map(Kind::EpsilonWall),
                },
                _ => None,
            }
        })?;
        let nut = ModelField::read(mesh, dir, "nut", KINEMATIC_DIFFUSIVITY, |pf| {
            match pf.kind.as_str() {
                "calculated" => Some(Kind::Calculated),
                "nutkWallFunction" => Some(Kind::NutWall(LogLaw::read(&pf.settings))),
                _ => None,
            }
        })?;
        for (patch, (ek, nk)) in mesh
            .patches()
            .iter()
            .zip(epsilon.kinds.iter().zip(&nut.kinds))
        {
            if matches!(ek, Kind::EpsilonWall(_)) && !matches!(nk, Kind::NutWall(_)) {
                return Err(SolverError::Setup(format!(
                    "epsilonWallFunction on '{}' needs a nut wall function there",
                    patch.name
                )));
            }
        }
        let y = mesh
            .patches()
            .iter()
            .zip(&nut.kinds)
            .map(|(patch, kind)| match kind {
                Kind::NutWall(_) => near_wall_distance(mesh, patch, &fv.geo.cell_centres),
                _ => Vec::new(),
            })
            .collect();
        let mut model = Self {
            coeffs: Coeffs::read(coeffs),
            nu,
            k,
            epsilon,
            nut,
            y,
            k_eqn: EquationSetup::read(case, "k", "DkEff", relax_k)?,
            epsilon_eqn: EquationSetup::read(case, "epsilon", "DepsilonEff", relax_epsilon)?,
        };
        let mut sink = |_: &str| {};
        bound(mesh, fv, "k", &mut model.k, &mut sink);
        bound(mesh, fv, "epsilon", &mut model.epsilon, &mut sink);
        model.correct_nut(mesh);
        Ok(model)
    }

    /// `ν + ν_t` in every cell.
    pub(crate) fn nu_eff_cells(&self) -> Vec<f64> {
        self.nut.cells.iter().map(|&t| t + self.nu).collect()
    }

    /// `ν + ν_t` on every boundary face (`None` on empty faces).
    pub(crate) fn nu_eff_boundary(&self) -> Vec<Option<f64>> {
        self.nut
            .faces
            .iter()
            .map(|t| t.map(|t| t + self.nu))
            .collect()
    }

    /// `ν_t/σ + ν` linearly on every face: what `laplacian(DkEff, k)` and
    /// `laplacian(nuEff, U)` (with `σ = 1`) take.
    pub(crate) fn diffusivity_faces(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        sigma: f64,
    ) -> Vec<f64> {
        let cell = |c: usize| {
            self.nut
                .cells
                .get(c)
                .map_or(self.nu, |&t| t / sigma + self.nu)
        };
        let mut out = Vec::with_capacity(mesh.n_faces());
        for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
            let w = fv.weights.get(f).copied().unwrap_or(0.5);
            let (p, q) = (cell(o), cell(n));
            out.push(w * (p - q) + q);
        }
        for t in &self.nut.faces {
            out.push(t.map_or(0.0, |t| t / sigma + self.nu));
        }
        out
    }

    /// `ν_t = Cμ k²/ε` in the cells and on calculated patches, and the log
    /// law on wall patches.
    fn correct_nut(&mut self, mesh: &PolyMesh) {
        let cmu = self.coeffs.cmu;
        for ((t, &k), &e) in self
            .nut
            .cells
            .iter_mut()
            .zip(&self.k.cells)
            .zip(&self.epsilon.cells)
        {
            *t = (cmu * (k * k)) / e;
        }
        let n_internal = mesh.n_internal_faces();
        for ((patch, kind), y) in mesh.patches().iter().zip(&self.nut.kinds).zip(&self.y) {
            for (i, f) in patch.faces().enumerate() {
                let b = f - n_internal;
                let value = match kind {
                    Kind::Calculated => {
                        match (
                            self.k.faces.get(b).copied().flatten(),
                            self.epsilon.faces.get(b).copied().flatten(),
                        ) {
                            (Some(k), Some(e)) => Some((cmu * (k * k)) / e),
                            _ => None,
                        }
                    }
                    Kind::NutWall(law) => {
                        let k = mesh
                            .owner()
                            .get(f)
                            .and_then(|&c| self.k.cells.get(c))
                            .copied()
                            .unwrap_or(0.0);
                        let y = y.get(i).copied().unwrap_or(0.0);
                        let y_plus = ((pow025(law.cmu) * y) * k.sqrt()) / self.nu;
                        Some(if law.y_plus_lam < y_plus {
                            self.nu * ((y_plus * law.kappa) / (law.e * y_plus).ln() - 1.0)
                        } else {
                            0.0
                        })
                    }
                    _ => continue,
                };
                if let Some(v) = self.nut.faces.get_mut(b) {
                    *v = value;
                }
            }
        }
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

        let wall_cells = self.epsilon_wall(mesh, fv, u, u_bc, &mut g);

        // ε: (div − laplacian) − (C1 GbyNu Cμ k − SuSp − Sp).
        let mut eps_eqn =
            self.transport(mesh, fv, &self.epsilon, &self.epsilon_eqn, phi, c.sigma_eps)?;
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
        if let Some(alpha) = self.epsilon_eqn.relax {
            eps_eqn.relax(mesh, &self.epsilon.cells, alpha);
        }
        let (cells, values): (Vec<usize>, Vec<f64>) = wall_cells
            .iter()
            .map(|&c| (c, self.epsilon.cells.get(c).copied().unwrap_or(0.0)))
            .unzip();
        eps_eqn.set_values(mesh, &cells, &values, &mut self.epsilon.cells);
        let setup = &self.epsilon_eqn;
        let perf = eps_eqn.solve(mesh, &mut self.epsilon.cells, |m, x| {
            setup.solver.solve(m, x, &setup.controls)
        })?;
        log(&solve_line("epsilon", &perf));
        let mut solves = vec![("epsilon".to_owned(), perf)];
        self.epsilon.follow_cells(mesh);
        bound(mesh, fv, "epsilon", &mut self.epsilon, log);

        // k: (div − laplacian) − (G − SuSp − Sp).
        let mut k_eqn = self.transport(mesh, fv, &self.k, &self.k_eqn, phi, c.sigmak)?;
        let susp: Vec<f64> = div_u.iter().map(|&d| (2.0 / 3.0) * d).collect();
        let sp: Vec<f64> = self
            .epsilon
            .cells
            .iter()
            .zip(&self.k.cells)
            .map(|(&e, &k)| e / k)
            .collect();
        k_eqn.sub(&rhs(mesh, fv, &g, &susp, &sp, &self.k.cells));
        if let Some(alpha) = self.k_eqn.relax {
            k_eqn.relax(mesh, &self.k.cells, alpha);
        }
        let setup = &self.k_eqn;
        let perf = k_eqn.solve(mesh, &mut self.k.cells, |m, x| {
            setup.solver.solve(m, x, &setup.controls)
        })?;
        log(&solve_line("k", &perf));
        solves.push(("k".to_owned(), perf));
        self.k.follow_cells(mesh);
        bound(mesh, fv, "k", &mut self.k, log);

        self.correct_nut(mesh);
        Ok(solves)
    }

    /// `fvm::div(φ, ψ) − fvm::laplacian(ν_t/σ + ν, ψ)`.
    fn transport(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        field: &ModelField,
        setup: &EquationSetup,
        phi: &[f64],
        sigma: f64,
    ) -> Result<ScalarMatrix> {
        let bc = field.boundary(mesh);
        let mut m = ScalarMatrix::convection(mesh, fv, &bc, phi, setup.convection.interpolation)?;
        if setup.convection.bounded {
            m.bound_convection(mesh, fv, phi);
        }
        let gamma = self.diffusivity_faces(mesh, fv, sigma);
        m.sub(&ScalarMatrix::laplacian(
            mesh,
            fv,
            &bc,
            &gamma,
            setup.sn_grad,
        )?);
        Ok(m)
    }

    /// The ε wall function's `updateCoeffs`: ε and `G` in each cell beside
    /// a wall from the log law, weighted by how many wall faces it has, and
    /// ε on those faces. Returns the cells, for fixing in the ε equation.
    fn epsilon_wall(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        u: &[Vec3],
        u_bc: &VectorBoundary,
        g: &mut [f64],
    ) -> Vec<usize> {
        let n_internal = mesh.n_internal_faces();
        let owner = mesh.owner();
        let mut count = vec![0usize; mesh.n_cells()];
        for (patch, kind) in mesh.patches().iter().zip(&self.epsilon.kinds) {
            if matches!(kind, Kind::EpsilonWall(_)) {
                for f in patch.faces() {
                    if let Some(n) = owner.get(f).and_then(|&c| count.get_mut(c)) {
                        *n += 1;
                    }
                }
            }
        }
        let mut g0 = vec![0.0; mesh.n_cells()];
        let mut eps0 = vec![0.0; mesh.n_cells()];
        for (((patch, kind), nut_kind), (y, u_cond)) in mesh
            .patches()
            .iter()
            .zip(&self.epsilon.kinds)
            .zip(&self.nut.kinds)
            .zip(self.y.iter().zip(&u_bc.conditions))
        {
            let (Kind::EpsilonWall(low_re), Kind::NutWall(law)) = (kind, nut_kind) else {
                continue;
            };
            let cmu25 = pow025(law.cmu);
            let cmu75 = law.cmu.powf(0.75);
            for (i, f) in patch.faces().enumerate() {
                let Some(&cell) = owner.get(f) else { continue };
                let k = self.k.cells.get(cell).copied().unwrap_or(0.0);
                let y = y.get(i).copied().unwrap_or(0.0);
                let nuw = self.nu;
                let nutw = self
                    .nut
                    .faces
                    .get(f - n_internal)
                    .copied()
                    .flatten()
                    .unwrap_or(0.0);
                let mag_grad_uw = match u_cond {
                    VectorBc::FixedValue(v) => {
                        let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                        let ub = v.get(i).copied().unwrap_or_default();
                        let uc = u.get(cell).copied().unwrap_or_default();
                        ((ub - uc) * dc).norm()
                    }
                    _ => 0.0,
                };
                let w = 1.0 / count.get(cell).copied().unwrap_or(1).max(1) as f64;
                let y_plus = ((cmu25 * y) * k.sqrt()) / nuw;
                let (eps_c, g_c) = if *low_re && y_plus < law.y_plus_lam {
                    (((w * 2.0) * k) * nuw / (y * y), 0.0)
                } else {
                    (
                        ((w * cmu75) * k.powf(1.5)) / (law.kappa * y),
                        ((((w * (nutw + nuw)) * mag_grad_uw) * cmu25) * k.sqrt()) / (law.kappa * y),
                    )
                };
                if let Some(e) = eps0.get_mut(cell) {
                    *e += eps_c;
                }
                if let Some(x) = g0.get_mut(cell) {
                    *x += g_c;
                }
            }
        }
        let mut cells = Vec::new();
        for (patch, kind) in mesh.patches().iter().zip(&self.epsilon.kinds) {
            if !matches!(kind, Kind::EpsilonWall(_)) {
                continue;
            }
            for f in patch.faces() {
                let Some(&cell) = owner.get(f) else { continue };
                let e = eps0.get(cell).copied().unwrap_or(0.0);
                if let Some(v) = self.epsilon.faces.get_mut(f - n_internal) {
                    *v = Some(e);
                }
                if let Some(x) = g.get_mut(cell) {
                    *x = g0.get(cell).copied().unwrap_or(0.0);
                }
                if let Some(x) = self.epsilon.cells.get_mut(cell) {
                    *x = e;
                }
                cells.push(cell);
            }
        }
        cells
    }

    /// Writes k, ε and ν_t into `case/time`.
    pub(crate) fn write(&self, mesh: &PolyMesh, dir: &std::path::Path, time: &str) -> Result<()> {
        for field in [&self.k, &self.epsilon, &self.nut] {
            let mut out = field.to_write(mesh);
            out.drop_zero_gradient_values();
            out.write(dir, time)?;
        }
        Ok(())
    }
}

/// The right-hand side `su − SuSp(susp, ψ) − Sp(sp, ψ)` as OpenFOAM builds
/// it: the SuSp matrix negated, the explicit source taken from it, then
/// the Sp matrix subtracted.
fn rhs(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    su: &[f64],
    susp: &[f64],
    sp: &[f64],
    psi: &[f64],
) -> ScalarMatrix {
    let mut m = ScalarMatrix::su_sp(mesh, fv, susp, psi);
    m.negate();
    m.add_source(fv, su);
    m.sub(&ScalarMatrix::sp(mesh, fv, sp));
    m
}

/// OpenFOAM's `bound`: when any value, the boundary included, is below
/// `SMALL`, negative cells take the face-weighted average of their
/// neighbours' bounded values and every value is held to `SMALL`.
fn bound(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    name: &str,
    field: &mut ModelField,
    log: &mut dyn FnMut(&str),
) {
    let min = field
        .cells
        .iter()
        .copied()
        .chain(field.faces.iter().flatten().copied())
        .fold(f64::INFINITY, f64::min);
    if min >= SMALL {
        return;
    }
    let max = field
        .cells
        .iter()
        .copied()
        .chain(field.faces.iter().flatten().copied())
        .fold(f64::NEG_INFINITY, f64::max);
    let average = field.cells.iter().sum::<f64>() / field.cells.len().max(1) as f64;
    log(&format!(
        "bounding {name}, min: {min} max: {max} average: {average}\n"
    ));

    // fvc::average(max(ψ, SMALL)): Σ |S_f| ψ_f / Σ |S_f| over each cell's faces.
    let floored: Vec<f64> = field.cells.iter().map(|&v| v.max(SMALL)).collect();
    let mut sum = vec![0.0; mesh.n_cells()];
    let mut area = vec![0.0; mesh.n_cells()];
    for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
        let w = fv.weights.get(f).copied().unwrap_or(0.5);
        let a = fv.mag_sf.get(f).copied().unwrap_or(0.0);
        let (p, q) = (
            floored.get(o).copied().unwrap_or(SMALL),
            floored.get(n).copied().unwrap_or(SMALL),
        );
        let face = a * (w * (p - q) + q);
        for c in [o, n] {
            if let Some(s) = sum.get_mut(c) {
                *s += face;
            }
            if let Some(s) = area.get_mut(c) {
                *s += a;
            }
        }
    }
    let n_internal = mesh.n_internal_faces();
    for (k, v) in field.faces.iter().enumerate() {
        let f = n_internal + k;
        let (Some(v), Some(&o)) = (v, mesh.owner().get(f)) else {
            continue;
        };
        let a = fv.mag_sf.get(f).copied().unwrap_or(0.0);
        if let Some(s) = sum.get_mut(o) {
            *s += a * v.max(SMALL);
        }
        if let Some(s) = area.get_mut(o) {
            *s += a;
        }
    }
    for ((v, &s), &a) in field.cells.iter_mut().zip(&sum).zip(&area) {
        let average = if a > 0.0 { s / a } else { SMALL };
        let pos0 = if -*v >= 0.0 { 1.0 } else { 0.0 };
        *v = v.max(average * pos0).max(SMALL);
    }
    for v in field.faces.iter_mut().flatten() {
        *v = v.max(SMALL);
    }
}

/// An OpenFOAM `Switch` word, or `None` when it is not one.
fn switch(word: &str) -> Option<bool> {
    match word {
        "yes" | "on" | "true" | "y" | "t" => Some(true),
        "no" | "off" | "false" | "n" | "f" | "none" => Some(false),
        _ => None,
    }
}
