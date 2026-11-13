//! What the RANS models share: their fields and conditions, the wall
//! functions, `bound`, and the turbulence model a `simpleFoam` run selects.
//!
//! Derived from OpenFOAM v1912's `kqRWallFunction`, `epsilonWallFunction`,
//! `omegaWallFunction`, `nutkWallFunction`, `bound` and `fvMatrix`
//! arithmetic (OpenFOAM, GPL-3.0-or-later), each in OpenFOAM's order and
//! arithmetic so the fields agree with OpenFOAM's to rounding.

use vortia_core::Vec3;
use vortia_fields::{Data, PatchField, ScalarField};
use vortia_foamfile::Dictionary;
use vortia_fv::correction;
use vortia_fv::limited::{self, GradScheme};
use vortia_fv::ops::{Convection, SnGrad};
use vortia_fv::vector::{VectorBc, VectorBoundary};
use vortia_fv::{Boundary, FvGeometry, Performance, ScalarBc, ScalarMatrix};
use vortia_mesh::{PatchKind, PolyMesh, wall::near_wall_distance};

use crate::case::{Case, check_dimensions, read_dict};
use crate::control::{LinearSolver, solve_line};
use crate::error::{Result, SolverError};
use crate::ico::switch_or;
use crate::kepsilon::KEpsilon;
use crate::komegasst::{KOmegaSst, Medium};

/// OpenFOAM's `SMALL`, the floor `bound` holds k, ε and ω to.
pub(crate) const SMALL: f64 = 1.0e-15;

/// Square root of a square root, as OpenFOAM's `pow025`.
pub(crate) fn pow025(x: f64) -> f64 {
    x.sqrt().sqrt()
}

/// The log law a wall patch uses: `nutkWallFunction`'s coefficients and the
/// `y⁺` where the laminar and log layers meet.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LogLaw {
    pub(crate) cmu: f64,
    pub(crate) kappa: f64,
    pub(crate) e: f64,
    pub(crate) y_plus_lam: f64,
}

impl LogLaw {
    pub(crate) fn read(settings: &Dictionary) -> Self {
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
pub(crate) enum Kind {
    Fixed(Vec<f64>),
    /// `zeroGradient`, and `kqRWallFunction` for k.
    ZeroGradient,
    Empty,
    /// `calculated` ν_t: from k and ε on the patch.
    Calculated,
    /// `epsilonWallFunction`; `true` with `lowReCorrection`.
    EpsilonWall(bool),
    /// `omegaWallFunction`: whether it blends the viscous and log layers,
    /// and its `β1`.
    OmegaWall {
        blended: bool,
        beta1: f64,
    },
    /// `nutkWallFunction`.
    NutWall(LogLaw),
    /// `nutUSpaldingWallFunction`: `κ` and `E`.
    NutSpalding {
        kappa: f64,
        e: f64,
    },
    /// `inletOutlet` and the inlets built on it: `ref_value` where the flow
    /// comes in, a zero gradient where it leaves. The reference and the
    /// fraction are updated when an equation in the field is assembled.
    Mixed {
        source: InletSource,
        ref_value: Vec<f64>,
        fraction: Vec<f64>,
    },
    /// `compressible::alphatWallFunction`: `ρ ν_t / Prt` on the wall.
    AlphatWall {
        prt: f64,
    },
}

/// Where a `Mixed` condition takes its reference value from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InletSource {
    /// `inletOutlet`: its `inletValue`.
    Value,
    /// `turbulentIntensityKineticEnergyInlet`: `1.5 (I |U|)²`.
    Intensity(f64),
    /// `turbulentMixingLengthFrequencyInlet`: `√k / (Cmu^¼ L)`.
    MixingLength(f64),
}

impl PartialEq for LogLaw {
    fn eq(&self, other: &Self) -> bool {
        self.cmu == other.cmu && self.kappa == other.kappa && self.e == other.e
    }
}

/// A field of the model: its cells, its value on every boundary face
/// (`None` on empty faces) and its conditions, with the file it came from.
pub(crate) struct ModelField {
    file: ScalarField,
    pub(crate) kinds: Vec<Kind>,
    pub(crate) cells: Vec<f64>,
    pub(crate) faces: Vec<Option<f64>>,
}

impl ModelField {
    pub(crate) fn read(
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
            } else if pf.kind == "wedge" {
                Kind::ZeroGradient
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
            if (patch.kind == PatchKind::Wedge) != (pf.kind == "wedge") {
                return Err(SolverError::Setup(format!(
                    "{name}: patch '{}' is {} but its condition is '{}'",
                    patch.name,
                    patch.kind.name(),
                    pf.kind
                )));
            }
            let given = pf.value.as_ref().and_then(|d| d.expand(patch.size));
            if let Kind::Mixed {
                source,
                ref_value,
                fraction,
            } = &mut kind
            {
                // As OpenFOAM's constructors: no inflow until the first update.
                *fraction = vec![0.0; patch.size];
                *ref_value = if *source == InletSource::Value {
                    pf.settings
                        .values("inletValue")
                        .map(|v| Data::<f64>::parse(v, "inletValue"))
                        .transpose()?
                        .and_then(|d| d.expand(patch.size))
                        .ok_or_else(|| {
                            SolverError::Setup(format!(
                                "{name}: patch '{}' needs 'inletValue' with {} items",
                                patch.name, patch.size
                            ))
                        })?
                } else {
                    vec![0.0; patch.size]
                };
            }
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
                    // As OpenFOAM's constructor, which starts it zero-gradient.
                    Kind::ZeroGradient | Kind::OmegaWall { .. } => inside,
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
    pub(crate) fn boundary(&self, mesh: &PolyMesh) -> Boundary {
        let n_internal = mesh.n_internal_faces();
        let conditions = mesh
            .patches()
            .iter()
            .zip(&self.kinds)
            .map(|(patch, kind)| match kind {
                Kind::Fixed(v) => ScalarBc::FixedValue(v.clone()),
                Kind::ZeroGradient => ScalarBc::ZeroGradient,
                Kind::Empty => ScalarBc::Empty,
                Kind::Mixed {
                    ref_value,
                    fraction,
                    ..
                } => ScalarBc::Mixed {
                    fraction: fraction.clone(),
                    ref_value: ref_value.clone(),
                },
                Kind::AlphatWall { .. }
                | Kind::EpsilonWall(_)
                | Kind::OmegaWall { .. }
                | Kind::Calculated
                | Kind::NutSpalding { .. }
                | Kind::NutWall(_) => ScalarBc::FixedValue(
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
    pub(crate) fn follow_cells(&mut self, mesh: &PolyMesh) {
        let n_internal = mesh.n_internal_faces();
        for (patch, kind) in mesh.patches().iter().zip(&self.kinds) {
            for (i, f) in patch.faces().enumerate() {
                let inside = mesh
                    .owner()
                    .get(f)
                    .and_then(|&c| self.cells.get(c))
                    .copied();
                let value = match kind {
                    Kind::ZeroGradient => inside,
                    Kind::Mixed {
                        ref_value,
                        fraction,
                        ..
                    } => inside.map(|inside| {
                        let r = ref_value.get(i).copied().unwrap_or(0.0);
                        let w = fraction.get(i).copied().unwrap_or(0.0);
                        w * r + (1.0 - w) * inside
                    }),
                    _ => continue,
                };
                if let Some(v) = self.faces.get_mut(f - n_internal) {
                    *v = value;
                }
            }
        }
    }

    /// What an equation in this field does when it is assembled, for the
    /// conditions that follow the flow: the reference value of each inlet and
    /// the fraction taken from it, which is 1 where the mass flux `phi` comes
    /// in. `u_b` is the velocity and `k_b` the turbulent kinetic energy on
    /// the boundary faces; `cmu` is the model's `Cmu`.
    pub(crate) fn update_inlets(
        &mut self,
        mesh: &PolyMesh,
        phi: &[f64],
        u_b: &[Option<Vec3>],
        k_b: Option<&[Option<f64>]>,
        cmu: f64,
    ) {
        let n_internal = mesh.n_internal_faces();
        for (patch, kind) in mesh.patches().iter().zip(&mut self.kinds) {
            let Kind::Mixed {
                source,
                ref_value,
                fraction,
            } = kind
            else {
                continue;
            };
            for (i, f) in patch.faces().enumerate() {
                let b = f - n_internal;
                let flux = phi.get(f).copied().unwrap_or(0.0);
                if let Some(w) = fraction.get_mut(i) {
                    *w = 1.0 - if flux >= 0.0 { 1.0 } else { 0.0 };
                }
                let value = match source {
                    InletSource::Value => None,
                    InletSource::Intensity(intensity) => {
                        let u = u_b.get(b).copied().flatten().unwrap_or_default();
                        Some((1.5 * (*intensity * *intensity)) * u.dot(u))
                    }
                    InletSource::MixingLength(length) => {
                        let k = k_b.and_then(|k| k.get(b)).copied().flatten().unwrap_or(0.0);
                        Some(k.sqrt() / (pow025(cmu) * *length))
                    }
                };
                if let (Some(v), Some(r)) = (value, ref_value.get_mut(i)) {
                    *r = v;
                }
            }
        }
    }

    /// The field as OpenFOAM writes it: each patch by name, with the value
    /// its condition writes.
    pub(crate) fn to_write(&self, mesh: &PolyMesh) -> ScalarField {
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
pub(crate) struct EquationSetup {
    convection: Convection,
    sn_grad: SnGrad,
    /// `grad(<field>)`, the gradient the non-orthogonal correction takes.
    grad: GradScheme,
    /// The coefficient of a `limited` Laplacian.
    limit: Option<f64>,
    solver: LinearSolver,
    controls: vortia_fv::Controls,
    relax: Option<f64>,
}

impl EquationSetup {
    /// Whether the Laplacian is corrected for non-orthogonality.
    pub(crate) fn is_corrected(&self) -> bool {
        self.sn_grad == SnGrad::Corrected
    }

    pub(crate) fn read(
        case: &Case,
        field: &str,
        diffusivity: &str,
        relax: Option<f64>,
    ) -> Result<Self> {
        let convection = case.convection(&format!("div(phi,{field})"))?;
        let (sn_grad, limit) =
            case.limited_laplacian_sn_grad(&format!("laplacian({diffusivity},{field})"))?;
        let grad = match sn_grad {
            SnGrad::Corrected => case.grad_scheme(&format!("grad({field})"))?,
            _ => GradScheme::GaussLinear,
        };
        let (solver, controls) = LinearSolver::read(case, field)?;
        Ok(Self {
            convection,
            sn_grad,
            grad,
            limit,
            solver,
            controls,
            relax,
        })
    }
}

/// What `constant/turbulenceProperties` selects.
pub(crate) enum Model {
    Laminar,
    KEpsilon(Option<Dictionary>),
    KOmegaSst(Option<Dictionary>),
    /// `rhoCentralFoam` only; the RAS dictionary, for its coefficients.
    SpalartAllmaras(Dictionary),
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
                let coeffs = |name: &str| ras.dict(&format!("{name}Coeffs")).cloned();
                let model = match model {
                    Some("kEpsilon") => Self::KEpsilon(coeffs("kEpsilon")),
                    // `coeffDict()` is the model's own dictionary, or the RAS one.
                    Some("kOmegaSST") => {
                        Self::KOmegaSst(coeffs("kOmegaSST").or_else(|| Some(ras.clone())))
                    }
                    Some("SpalartAllmaras") => Self::SpalartAllmaras(ras.clone()),
                    other => {
                        return Err(SolverError::Setup(format!(
                            "RAS model '{}' is not supported yet; use kEpsilon, kOmegaSST or SpalartAllmaras",
                            other.unwrap_or("")
                        )));
                    }
                };
                if !switch_or(&ras, "turbulence", true, "RAS")? {
                    return Err(SolverError::Setup(
                        "RAS with 'turbulence off' is not supported yet".into(),
                    ));
                }
                Ok(model)
            }
            other => Err(SolverError::Setup(format!(
                "simulationType '{}' is not supported yet; use laminar or RAS",
                other.unwrap_or("")
            ))),
        }
    }
}

/// The turbulence model of a run.
pub(crate) enum Turbulence {
    KEpsilon(Box<KEpsilon>),
    KOmegaSst(Box<KOmegaSst>),
}

impl Turbulence {
    /// Reads the model's fields from `dir`; `None` for laminar flow.
    /// `relax` gives an equation's relaxation factor.
    pub(crate) fn read(
        case: &Case,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        dir: &std::path::Path,
        nu: f64,
        flow: (&VectorBoundary, &[Vec3], &[usize]),
        relax: impl Fn(&str) -> Option<f64>,
    ) -> Result<Option<Self>> {
        Ok(match Model::read(case)? {
            Model::Laminar => None,
            Model::SpalartAllmaras(_) => {
                return Err(SolverError::Setup(
                    "RAS model 'SpalartAllmaras' runs in rhoCentralFoam only; use kEpsilon or kOmegaSST".into(),
                ));
            }
            Model::KEpsilon(coeffs) => Some(Self::KEpsilon(Box::new(KEpsilon::read(
                case,
                mesh,
                fv,
                dir,
                coeffs.as_ref(),
                nu,
                relax("k"),
                relax("epsilon"),
            )?))),
            Model::KOmegaSst(coeffs) => Some(Self::KOmegaSst(Box::new(KOmegaSst::read(
                case,
                mesh,
                fv,
                dir,
                coeffs.as_ref(),
                Medium::Uniform(nu),
                [relax("k"), relax("omega")],
                flow,
            )?))),
        })
    }

    /// `ν + ν_t` in every cell and on every boundary face (`None` on empty
    /// faces), and linearly on every face.
    pub(crate) fn nu_eff(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
    ) -> (Vec<f64>, Vec<Option<f64>>, Vec<f64>) {
        let (cells, boundary): (Vec<f64>, Vec<Option<f64>>) = match self {
            Self::KEpsilon(m) => {
                let (nut, nu) = (m.nut(), m.nu());
                (
                    nut.cells.iter().map(|&t| t + nu).collect(),
                    nut.faces.iter().map(|t| t.map(|t| t + nu)).collect(),
                )
            }
            Self::KOmegaSst(m) => (m.nu_eff_cells(), m.nu_eff_faces()),
        };
        let faces = face_diffusivity(mesh, fv, &cells, &boundary);
        (cells, boundary, faces)
    }

    /// One `turbulence->correct()`. Logs each solve and returns it.
    pub(crate) fn correct(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        u: &[Vec3],
        u_bc: &VectorBoundary,
        phi: &[f64],
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<(String, Performance)>> {
        match self {
            Self::KEpsilon(m) => m.correct(mesh, fv, u, u_bc, phi, log),
            Self::KOmegaSst(m) => m.correct(mesh, fv, u, u_bc, phi, log),
        }
    }

    /// Writes the model's fields into `dir/time`.
    pub(crate) fn write(&self, mesh: &PolyMesh, dir: &std::path::Path, time: &str) -> Result<()> {
        let fields = match self {
            Self::KEpsilon(m) => m.fields(),
            Self::KOmegaSst(m) => m.fields(),
        };
        for field in fields {
            let mut out = field.to_write(mesh);
            out.drop_zero_gradient_values();
            out.write(dir, time)?;
        }
        Ok(())
    }
}

/// A cell field with its boundary values, linearly on every face, as
/// `fvm::laplacian` interpolates a diffusivity; 0 on empty faces.
pub(crate) fn face_diffusivity(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[f64],
    boundary: &[Option<f64>],
) -> Vec<f64> {
    let mut out = Vec::with_capacity(mesh.n_faces());
    for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
        let w = fv.weights.get(f).copied().unwrap_or(0.5);
        let p = cells.get(o).copied().unwrap_or(0.0);
        let q = cells.get(n).copied().unwrap_or(0.0);
        out.push(w * (p - q) + q);
    }
    out.extend(boundary.iter().map(|b| b.unwrap_or(0.0)));
    out
}

/// `fvm::div(φ, ψ) − fvm::laplacian(Γ, ψ)` with `Γ` on every face.
pub(crate) fn transport(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    field: &ModelField,
    setup: &EquationSetup,
    phi: &[f64],
    gamma: &[f64],
) -> Result<ScalarMatrix> {
    let bc = field.boundary(mesh);
    let mut m = ScalarMatrix::convection(mesh, fv, &bc, phi, setup.convection.interpolation)?;
    if setup.convection.bounded {
        m.bound_convection(mesh, fv, phi);
    }
    let mut lap =
        ScalarMatrix::laplacian(mesh, fv, &bc, gamma, setup.sn_grad.without_correction())?;
    if setup.is_corrected() {
        let g = limited::grad(setup.grad, mesh, fv, &field.cells, &field.faces);
        let mut corr = correction::sn_grad_correction(mesh, fv, &g);
        if let Some(k) = setup.limit {
            corr = correction::limit_correction(mesh, fv, &field.cells, &corr, k, f64::abs);
        }
        let flux = correction::scale_by_gamma_mag_sf(fv, gamma, &corr);
        correction::subtract_divergence(&mut lap.ldu.source, mesh, &flux);
    }
    m.sub(&lap);
    Ok(m)
}

/// Relaxes `eqn`, fixes the `fixed` cells at their values, solves it into
/// `field`, logs the solve as `name` and updates the conditions that follow
/// the cells. `bound` is the caller's, as OpenFOAM's models call it after.
pub(crate) fn solve(
    mesh: &PolyMesh,
    mut eqn: ScalarMatrix,
    field: &mut ModelField,
    setup: &EquationSetup,
    fixed: &[usize],
    name: &str,
    log: &mut dyn FnMut(&str),
) -> Result<(String, Performance)> {
    if let Some(alpha) = setup.relax {
        eqn.relax(mesh, &field.cells, alpha);
    }
    if !fixed.is_empty() {
        let values: Vec<f64> = fixed
            .iter()
            .map(|&c| field.cells.get(c).copied().unwrap_or(0.0))
            .collect();
        eqn.set_values(mesh, fixed, &values, &mut field.cells);
    }
    let perf = eqn.solve(mesh, &mut field.cells, |m, x| {
        setup.solver.solve(m, x, &setup.controls)
    })?;
    log(&solve_line(name, &perf));
    field.follow_cells(mesh);
    Ok((name.to_owned(), perf))
}

/// For each patch with a ν_t wall function, the distance of each of its
/// faces' cells to it; empty for other patches.
pub(crate) fn near_wall(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    nut: &ModelField,
) -> Result<Vec<Vec<f64>>> {
    mesh.patches()
        .iter()
        .zip(&nut.kinds)
        .map(|(patch, kind)| match kind {
            Kind::NutWall(_) => Ok(near_wall_distance(mesh, patch, &fv.geo.cell_centres)?),
            _ => Ok(Vec::new()),
        })
        .collect()
}

/// `nutkWallFunction`: ν_t on each wall face from the log law and the k of
/// its cell, 0 below the log layer.
pub(crate) fn correct_nut_walls(
    mesh: &PolyMesh,
    nut: &mut ModelField,
    k: &[f64],
    y: &[Vec<f64>],
    nu: impl Fn(usize) -> f64,
) {
    let n_internal = mesh.n_internal_faces();
    for ((patch, kind), y) in mesh.patches().iter().zip(&nut.kinds).zip(y) {
        let Kind::NutWall(law) = kind else { continue };
        for (i, f) in patch.faces().enumerate() {
            let k = mesh
                .owner()
                .get(f)
                .and_then(|&c| k.get(c))
                .copied()
                .unwrap_or(0.0);
            let y = y.get(i).copied().unwrap_or(0.0);
            let nu = nu(f - n_internal);
            let y_plus = ((pow025(law.cmu) * y) * k.sqrt()) / nu;
            let value = if law.y_plus_lam < y_plus {
                nu * ((y_plus * law.kappa) / (law.e * y_plus).ln() - 1.0)
            } else {
                0.0
            };
            if let Some(v) = nut.faces.get_mut(f - n_internal) {
                *v = Some(value);
            }
        }
    }
}

/// What a wall function knows about one wall face.
pub(crate) struct WallFace {
    /// k in the cell beside the face.
    pub(crate) k: f64,
    /// The cell centre's distance to the wall.
    pub(crate) y: f64,
    pub(crate) nuw: f64,
    pub(crate) nutw: f64,
    /// `|∂U/∂n|` at the face.
    pub(crate) mag_grad_uw: f64,
    /// 1 over the number of wall-function faces of the cell.
    pub(crate) w: f64,
}

/// A wall function's `updateCoeffs`, shared by ε and ω: each face of a wall
/// patch adds `per_face`'s value and production to its cell, weighted by
/// how many such faces the cell has; then the cells and their faces take
/// the sums and `g` takes the production. Returns the cells, for fixing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wall_function(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    field: &mut ModelField,
    nut: &ModelField,
    k: &[f64],
    y: &[Vec<f64>],
    nu: impl Fn(usize) -> f64,
    (u, u_bc): (&[Vec3], &VectorBoundary),
    g: &mut [f64],
    per_face: impl Fn(&Kind, &LogLaw, &WallFace) -> (f64, f64),
) -> Vec<usize> {
    let is_wall = |k: &Kind| matches!(k, Kind::EpsilonWall(_) | Kind::OmegaWall { .. });
    let n_internal = mesh.n_internal_faces();
    let owner = mesh.owner();
    let mut count = vec![0usize; mesh.n_cells()];
    for (patch, kind) in mesh.patches().iter().zip(&field.kinds) {
        if is_wall(kind) {
            for f in patch.faces() {
                if let Some(n) = owner.get(f).and_then(|&c| count.get_mut(c)) {
                    *n += 1;
                }
            }
        }
    }
    let mut g0 = vec![0.0; mesh.n_cells()];
    let mut v0 = vec![0.0; mesh.n_cells()];
    for (((patch, kind), nut_kind), (y, u_cond)) in mesh
        .patches()
        .iter()
        .zip(&field.kinds)
        .zip(&nut.kinds)
        .zip(y.iter().zip(&u_bc.conditions))
    {
        let Kind::NutWall(law) = nut_kind else {
            continue;
        };
        if !is_wall(kind) {
            continue;
        }
        for (i, f) in patch.faces().enumerate() {
            let Some(&cell) = owner.get(f) else { continue };
            let mag_grad_uw = match u_cond {
                VectorBc::FixedValue(v) => {
                    let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                    let ub = v.get(i).copied().unwrap_or_default();
                    let uc = u.get(cell).copied().unwrap_or_default();
                    ((ub - uc) * dc).norm()
                }
                _ => 0.0,
            };
            let face = WallFace {
                k: k.get(cell).copied().unwrap_or(0.0),
                y: y.get(i).copied().unwrap_or(0.0),
                nuw: nu(f - n_internal),
                nutw: nut
                    .faces
                    .get(f - n_internal)
                    .copied()
                    .flatten()
                    .unwrap_or(0.0),
                mag_grad_uw,
                w: 1.0 / count.get(cell).copied().unwrap_or(1).max(1) as f64,
            };
            let (value, production) = per_face(kind, law, &face);
            if let Some(v) = v0.get_mut(cell) {
                *v += value;
            }
            if let Some(x) = g0.get_mut(cell) {
                *x += production;
            }
        }
    }
    let mut cells = Vec::new();
    for (patch, kind) in mesh.patches().iter().zip(&field.kinds) {
        if !is_wall(kind) {
            continue;
        }
        for f in patch.faces() {
            let Some(&cell) = owner.get(f) else { continue };
            let v = v0.get(cell).copied().unwrap_or(0.0);
            if let Some(face) = field.faces.get_mut(f - n_internal) {
                *face = Some(v);
            }
            if let Some(x) = g.get_mut(cell) {
                *x = g0.get(cell).copied().unwrap_or(0.0);
            }
            if let Some(x) = field.cells.get_mut(cell) {
                *x = v;
            }
            cells.push(cell);
        }
    }
    cells
}

/// The right-hand side `su − SuSp(susp, ψ) − Sp(sp, ψ)` as OpenFOAM builds
/// it: the SuSp matrix negated, the explicit source taken from it, then
/// the Sp matrix subtracted.
pub(crate) fn rhs(
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
pub(crate) fn bound(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    name: &str,
    field: &mut ModelField,
    log: &mut dyn FnMut(&str),
) {
    bound_at(mesh, fv, name, field, SMALL, log);
}

/// [`bound`] holding the field to `floor` instead of `SMALL`.
pub(crate) fn bound_at(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    name: &str,
    field: &mut ModelField,
    floor: f64,
    log: &mut dyn FnMut(&str),
) {
    let min = field
        .cells
        .iter()
        .copied()
        .chain(field.faces.iter().flatten().copied())
        .fold(f64::INFINITY, f64::min);
    if min >= floor {
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

    // fvc::average(max(ψ, floor)): Σ |S_f| ψ_f / Σ |S_f| over each cell's faces.
    let floored: Vec<f64> = field.cells.iter().map(|&v| v.max(floor)).collect();
    let mut sum = vec![0.0; mesh.n_cells()];
    let mut area = vec![0.0; mesh.n_cells()];
    for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
        let w = fv.weights.get(f).copied().unwrap_or(0.5);
        let a = fv.mag_sf.get(f).copied().unwrap_or(0.0);
        let (p, q) = (
            floored.get(o).copied().unwrap_or(floor),
            floored.get(n).copied().unwrap_or(floor),
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
            *s += a * v.max(floor);
        }
        if let Some(s) = area.get_mut(o) {
            *s += a;
        }
    }
    for ((v, &s), &a) in field.cells.iter_mut().zip(&sum).zip(&area) {
        let average = if a > 0.0 { s / a } else { floor };
        let pos0 = if -*v >= 0.0 { 1.0 } else { 0.0 };
        *v = v.max(average * pos0).max(floor);
    }
    for v in field.faces.iter_mut().flatten() {
        *v = v.max(floor);
    }
}
