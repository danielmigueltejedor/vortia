//! The Spalart–Allmaras model for a compressible gas, as `rhoCentralFoam`
//! runs it.
//!
//! Derived from OpenFOAM's `SpalartAllmaras` (OpenFOAM, GPL-3.0-or-later),
//! each step in OpenFOAM's order and with its arithmetic:
//!
//! 1. `χ = ν̃/ν`, `fv1 = χ³/(χ³ + Cv1³)`, `fv2 = 1 − χ/(1 + χ fv1)`;
//! 2. `S̃ = max(Ω + fv2 ν̃/(κy)², Cs Ω)` with `Ω = √2 |skew(∇U)|`;
//! 3. `r = min(ν̃/(max(S̃, small) (κy)²), 10)`, `g = r + Cw2 (r⁶ − r)` and
//!    `fw = g ((1 + Cw3⁶)/(g⁶ + Cw3⁶))^(1/6)`;
//! 4. `∂(ρν̃)/∂t + ∇·(φ ν̃) − ∇·(ρ (ν̃ + ν)/σ ∇ν̃) − Cb2/σ ρ |∇ν̃|² =
//!    Cb1 ρ S̃ ν̃ − Cw1 ρ fw ν̃/y² ν̃`, solved and bounded at zero;
//! 5. `ν_t = ν̃ fv1` and `α_t = ρ ν_t/Prt`.

use vortia_core::Vec3;
use vortia_foamfile::Dictionary;
use vortia_fv::limited::{self, GradScheme};
use vortia_fv::stress;
use vortia_fv::vector::VectorBoundary;
use vortia_fv::{FvGeometry, Performance};
use vortia_mesh::PolyMesh;
use vortia_mesh::wall::{near_wall_distance, wall_distance};

use crate::case::{Case, KINEMATIC_DIFFUSIVITY};
use crate::coded::{self, Program};
use crate::error::Result;
use crate::komegasst::check_wall_dist;
use crate::rans::{
    EquationSetup, InletSource, Kind, ModelField, SMALL, bound_at, face_diffusivity, rhs, solve,
    transport,
};

/// `[1 -1 -1 0 0 0 0]`.
const DYNAMIC_DIFFUSIVITY: [i32; 7] = [1, -1, -1, 0, 0, 0, 0];

/// The model's coefficients, `SpalartAllmarasCoeffs` over OpenFOAM's
/// defaults.
#[derive(Debug, Clone, Copy)]
struct Coeffs {
    sigma_nut: f64,
    kappa: f64,
    cb1: f64,
    cb2: f64,
    cw1: f64,
    cw2: f64,
    cw3: f64,
    cv1: f64,
    cs: f64,
    prt: f64,
}

impl Coeffs {
    fn read(d: Option<&Dictionary>) -> Self {
        let get = |k: &str, def: f64| d.and_then(|d| d.scalar(k)).unwrap_or(def);
        let sigma_nut = get("sigmaNut", 0.66666);
        let kappa = get("kappa", 0.41);
        let cb1 = get("Cb1", 0.1355);
        let cb2 = get("Cb2", 0.622);
        Self {
            sigma_nut,
            kappa,
            cb1,
            cb2,
            cw1: cb1 / (kappa * kappa) + (1.0 + cb2) / sigma_nut,
            cw2: get("Cw2", 0.3),
            cw3: get("Cw3", 2.0),
            cv1: get("Cv1", 7.1),
            cs: get("Cs", 0.3),
            prt: get("Prt", 1.0),
        }
    }
}

/// The gas the model runs in at one moment: its density now and at the
/// start of the step, its dynamic viscosity and the local time step.
pub(crate) struct Gas<'a> {
    pub(crate) rho: &'a [f64],
    pub(crate) rho_b: &'a [Option<f64>],
    pub(crate) rho_old: &'a [f64],
    pub(crate) mu: &'a [f64],
    pub(crate) mu_b: &'a [Option<f64>],
    pub(crate) r_delta_t: &'a [f64],
}

/// The Spalart–Allmaras model of a run: its fields and how it solves them.
pub(crate) struct SpalartAllmaras {
    c: Coeffs,
    u_grad: GradScheme,
    nu_tilda_grad: GradScheme,
    nu_tilda: ModelField,
    nut: ModelField,
    alphat: ModelField,
    /// The distance to the nearest wall of every cell.
    y: Vec<f64>,
    /// The distance to the wall of each face of each wall-function patch.
    y_wall: Vec<Vec<f64>>,
    eqn: EquationSetup,
}

fn pow3(x: f64) -> f64 {
    x * x * x
}

fn fv1(cv1: f64, nu_tilda: f64, nu: f64) -> f64 {
    let chi3 = pow3(nu_tilda / nu);
    chi3 / (chi3 + pow3(cv1))
}

fn pow6(x: f64) -> f64 {
    pow3(x) * pow3(x)
}

/// OpenFOAM's `rootVSmall`.
const ROOT_VSMALL: f64 = 1.0e-150;

/// The friction velocity of Spalding's law on a face: Newton's method from
/// `√(τ/ρ)` of the value the face had, at most ten iterations to 1 %.
fn spalding_u_tau(kappa: f64, e: f64, (y, nuw): (f64, f64), mag_up: f64, start: f64) -> f64 {
    let mut ut = start.sqrt();
    if ut <= ROOT_VSMALL {
        return 0.0;
    }
    let mut iter = 0;
    loop {
        let k_uu = (kappa * mag_up / ut).min(50.0);
        let fk_uu = k_uu.exp() - 1.0 - k_uu * (1.0 + 0.5 * k_uu);
        let f = -ut * y / nuw + mag_up / ut + 1.0 / e * (fk_uu - 1.0 / 6.0 * k_uu * (k_uu * k_uu));
        let df = y / nuw + mag_up / (ut * ut) + 1.0 / e * k_uu * fk_uu / ut;
        let new = ut + f / df;
        let err = ((ut - new) / ut).abs();
        ut = new;
        iter += 1;
        if !(ut > ROOT_VSMALL && err > 0.01 && iter < 10) {
            break;
        }
    }
    ut.max(0.0)
}

/// `|skew(g)|`, the magnitude of the antisymmetric part of a gradient.
fn mag_skew(g: &stress::Tensor) -> f64 {
    let [[_, xy, xz], [yx, _, yz], [zx, zy, _]] = *g;
    let (a, b, c) = (0.5 * (xy - yx), 0.5 * (xz - zx), 0.5 * (yz - zy));
    (2.0 * (a * a + b * b + c * c)).sqrt()
}

impl SpalartAllmaras {
    /// Reads ν̃, ν_t and α_t from `dir`, the distance to the walls and the
    /// schemes of the model's equation.
    pub(crate) fn read(
        case: &Case,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        dir: &std::path::Path,
        coeffs: Option<&Dictionary>,
        relax: Option<f64>,
    ) -> Result<Self> {
        check_wall_dist(case)?;
        let refused = std::cell::RefCell::new(None);
        let nu_tilda = ModelField::read(mesh, dir, "nuTilda", KINEMATIC_DIFFUSIVITY, |pf| match pf
            .kind
            .as_str()
        {
            "fixedValue" => Some(Kind::Fixed(Vec::new())),
            "zeroGradient" => Some(Kind::ZeroGradient),
            "inletOutlet" => Some(Kind::Mixed {
                source: InletSource::Value,
                ref_value: Vec::new(),
                fraction: Vec::new(),
            }),
            "codedMixed" => match coded::read(&pf.settings, "nuTilda: codedMixed") {
                Ok(Program::ViscosityRatio(chi_mu)) => Some(Kind::Mixed {
                    source: InletSource::ViscosityRatio(chi_mu),
                    ref_value: Vec::new(),
                    fraction: Vec::new(),
                }),
                Ok(_) => None,
                Err(e) => {
                    *refused.borrow_mut() = Some(e);
                    None
                }
            },
            _ => None,
        })
        .map_err(|e| refused.take().unwrap_or(e))?;
        let nut = ModelField::read(mesh, dir, "nut", KINEMATIC_DIFFUSIVITY, |pf| {
            match pf.kind.as_str() {
                "calculated" => Some(Kind::Calculated),
                "nutUSpaldingWallFunction" => Some(Kind::NutSpalding {
                    kappa: pf.settings.scalar("kappa").unwrap_or(0.41),
                    e: pf.settings.scalar("E").unwrap_or(9.8),
                }),
                _ => None,
            }
        })?;
        let alphat = ModelField::read(mesh, dir, "alphat", DYNAMIC_DIFFUSIVITY, |pf| {
            match pf.kind.as_str() {
                "calculated" => Some(Kind::Calculated),
                "compressible::alphatWallFunction" => Some(Kind::AlphatWall {
                    prt: pf.settings.scalar("Prt").unwrap_or(0.85),
                }),
                _ => None,
            }
        })?;
        let y_wall = mesh
            .patches()
            .iter()
            .zip(&nut.kinds)
            .map(|(patch, kind)| match kind {
                Kind::NutSpalding { .. } => {
                    Ok(near_wall_distance(mesh, patch, &fv.geo.cell_centres)?)
                }
                _ => Ok(Vec::new()),
            })
            .collect::<Result<Vec<_>>>()?;
        let y = wall_distance(mesh, &fv.geo.cell_centres, &fv.geo.face_centres)?;
        Ok(Self {
            c: Coeffs::read(coeffs),
            u_grad: case.grad_scheme("grad(U)")?,
            nu_tilda_grad: case.grad_scheme("grad(nuTilda)")?,
            nu_tilda,
            nut,
            alphat,
            y: y.cells,
            y_wall,
            eqn: EquationSetup::read(case, "nuTilda", "DnuTildaEff", relax)?,
        })
    }

    /// OpenFOAM's `validate`: ν_t from the ν̃ the case starts with.
    pub(crate) fn validate(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        flow: (&[Vec3], &VectorBoundary),
        gas: &Gas,
    ) {
        self.correct_nut(mesh, fv, flow, gas);
    }

    /// What OpenFOAM's constructor of a programmed condition does: it
    /// evaluates the condition at once, with the density and the velocity on
    /// the boundary, and the field's value there follows.
    pub(crate) fn evaluate_programmed(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        u_b: &[Option<Vec3>],
        rho_b: &[Option<f64>],
    ) {
        self.nu_tilda.update_viscosity_ratio(mesh, fv, u_b, rho_b);
        self.nu_tilda.follow_cells(mesh);
    }

    pub(crate) fn alphat(&self) -> &ModelField {
        &self.alphat
    }

    /// `ν = μ/ρ` in the cells and on the boundary faces.
    fn nu(gas: &Gas) -> (Vec<f64>, Vec<Option<f64>>) {
        (
            gas.mu.iter().zip(gas.rho).map(|(&m, &r)| m / r).collect(),
            gas.mu_b
                .iter()
                .zip(gas.rho_b)
                .map(|(m, r)| m.zip(*r).map(|(m, r)| m / r))
                .collect(),
        )
    }

    /// `ν_t = ν̃ fv1` and `α_t = ρ ν_t/Prt`, in the cells and on the
    /// faces of the calculated patches.
    fn correct_nut(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        (u, u_bc): (&[Vec3], &VectorBoundary),
        gas: &Gas,
    ) {
        let (nu, nu_b) = Self::nu(gas);
        let cv1 = self.c.cv1;
        for (i, nut) in self.nut.cells.iter_mut().enumerate() {
            let nt = self.nu_tilda.cells.get(i).copied().unwrap_or(0.0);
            let nu = nu.get(i).copied().unwrap_or(1.0);
            *nut = nt * fv1(cv1, nt, nu);
        }
        let n_internal = mesh.n_internal_faces();
        for (patch, kind) in mesh.patches().iter().zip(&self.nut.kinds) {
            if *kind != Kind::Calculated {
                continue;
            }
            for f in patch.faces() {
                let b = f - n_internal;
                let nt = self.nu_tilda.faces.get(b).copied().flatten();
                let nu = nu_b.get(b).copied().flatten();
                if let (Some(nt), Some(nu), Some(slot)) = (nt, nu, self.nut.faces.get_mut(b)) {
                    *slot = Some(nt * fv1(cv1, nt, nu));
                }
            }
        }
        self.spalding_walls(mesh, fv, (u, u_bc), &nu_b);
        self.nut.follow_cells(mesh);
        self.correct_alphat(mesh, gas);
    }

    /// `nutUSpaldingWallFunction`: ν_t on each wall face from Spalding's law,
    /// iterating from the friction velocity of the value the face had.
    fn spalding_walls(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        (u, u_bc): (&[Vec3], &VectorBoundary),
        nu_b: &[Option<f64>],
    ) {
        let n_internal = mesh.n_internal_faces();
        let u_b = u_bc.face_values(mesh, u);
        for ((patch, kind), y) in mesh.patches().iter().zip(&self.nut.kinds).zip(&self.y_wall) {
            let Kind::NutSpalding { kappa, e } = *kind else {
                continue;
            };
            for (i, f) in patch.faces().enumerate() {
                let b = f - n_internal;
                let (Some(&cell), Some(&dc)) = (mesh.owner().get(f), fv.delta_coeffs.get(f)) else {
                    continue;
                };
                let uc = u.get(cell).copied().unwrap_or_default();
                let uw = u_b.get(b).copied().flatten().unwrap_or_default();
                let nuw = nu_b.get(b).copied().flatten().unwrap_or(1.0);
                let mag_grad_u = ((uw - uc) * dc).norm();
                let mag_up = (uc - uw).norm();
                let nutw = self.nut.faces.get(b).copied().flatten().unwrap_or(0.0);
                let y = y.get(i).copied().unwrap_or(0.0);
                let u_tau = spalding_u_tau(kappa, e, (y, nuw), mag_up, (nutw + nuw) * mag_grad_u);
                let value = (u_tau * u_tau / (mag_grad_u + ROOT_VSMALL) - nuw).max(0.0);
                if let Some(slot) = self.nut.faces.get_mut(b) {
                    *slot = Some(value);
                }
            }
        }
    }

    fn correct_alphat(&mut self, mesh: &PolyMesh, gas: &Gas) {
        let prt = self.c.prt;
        for ((a, &nut), &rho) in self
            .alphat
            .cells
            .iter_mut()
            .zip(&self.nut.cells)
            .zip(gas.rho)
        {
            *a = rho * nut / prt;
        }
        let n_internal = mesh.n_internal_faces();
        for (patch, kind) in mesh.patches().iter().zip(&self.alphat.kinds) {
            let prt = match kind {
                Kind::Calculated => prt,
                Kind::AlphatWall { prt } => *prt,
                _ => continue,
            };
            for f in patch.faces() {
                let b = f - n_internal;
                let nut = self.nut.faces.get(b).copied().flatten();
                let rho = gas.rho_b.get(b).copied().flatten();
                if let (Some(nut), Some(rho), Some(slot)) = (nut, rho, self.alphat.faces.get_mut(b))
                {
                    *slot = Some(rho * nut / prt);
                }
            }
        }
        self.alphat.follow_cells(mesh);
    }

    /// `μ_eff = ρ (ν_t + ν)` in the cells and on the boundary faces.
    pub(crate) fn mu_eff(&self, gas: &Gas) -> (Vec<f64>, Vec<Option<f64>>) {
        let (nu, nu_b) = Self::nu(gas);
        let cells = gas
            .rho
            .iter()
            .zip(&self.nut.cells)
            .zip(&nu)
            .map(|((&r, &t), &nu)| r * (t + nu))
            .collect();
        let faces = gas
            .rho_b
            .iter()
            .zip(&self.nut.faces)
            .zip(&nu_b)
            .map(|((r, t), nu)| match (r, t, nu) {
                (Some(r), Some(t), Some(nu)) => Some(r * (t + nu)),
                _ => None,
            })
            .collect();
        (cells, faces)
    }

    /// One `turbulence->correct()`.
    pub(crate) fn correct(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        (u, u_bc): (&[Vec3], &VectorBoundary),
        phi: &[f64],
        gas: &Gas,
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<(String, Performance)>> {
        let c = self.c;
        let (nu, nu_b) = Self::nu(gas);
        let u_b = u_bc.face_values(mesh, u);
        self.nu_tilda.update_inlets(mesh, phi, &u_b, None, 0.09);
        self.nu_tilda
            .update_viscosity_ratio(mesh, fv, &u_b, gas.rho_b);

        let (grad_u, _) = stress::grad(self.u_grad, mesh, fv, u_bc, u);
        let grad_nt = limited::grad(
            self.nu_tilda_grad,
            mesh,
            fv,
            &self.nu_tilda.cells,
            &self.nu_tilda.faces,
        );
        let n = mesh.n_cells();
        let at = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
        let (mut su, mut sp) = (vec![0.0; n], vec![0.0; n]);
        for i in 0..n {
            let (nt, rho, y) = (at(&self.nu_tilda.cells, i), at(gas.rho, i), at(&self.y, i));
            let fv1 = fv1(c.cv1, nt, at(&nu, i));
            let chi = nt / at(&nu, i);
            let fv2 = 1.0 - chi / (1.0 + chi * fv1);
            let omega = std::f64::consts::SQRT_2 * grad_u.get(i).map_or(0.0, mag_skew);
            let ky2 = (c.kappa * y) * (c.kappa * y);
            let stilda = (omega + fv2 * nt / ky2).max(c.cs * omega);
            let r = (nt / (stilda.max(SMALL) * ky2)).min(10.0);
            let g = r + c.cw2 * (pow6(r) - r);
            let fw = g * ((1.0 + pow6(c.cw3)) / (pow6(g) + pow6(c.cw3))).powf(1.0 / 6.0);
            let mag_sqr_grad = grad_nt.get(i).map_or(0.0, |g| g.dot(*g));
            if let Some(s) = su.get_mut(i) {
                *s = c.cb1 * rho * stilda * nt + c.cb2 / c.sigma_nut * rho * mag_sqr_grad;
            }
            if let Some(s) = sp.get_mut(i) {
                *s = c.cw1 * rho * fw * nt / (y * y);
            }
        }

        let d_cells: Vec<f64> = gas
            .rho
            .iter()
            .zip(&self.nu_tilda.cells)
            .zip(&nu)
            .map(|((&r, &nt), &nu)| r * ((nt + nu) / c.sigma_nut))
            .collect();
        let d_faces: Vec<Option<f64>> = gas
            .rho_b
            .iter()
            .zip(&self.nu_tilda.faces)
            .zip(&nu_b)
            .map(|((r, nt), nu)| match (r, nt, nu) {
                (Some(r), Some(nt), Some(nu)) => Some(r * ((nt + nu) / c.sigma_nut)),
                _ => None,
            })
            .collect();
        let gamma = face_diffusivity(mesh, fv, &d_cells, &d_faces);

        let mut eqn = transport(mesh, fv, &self.nu_tilda, &self.eqn, phi, &gamma)?;
        for (i, &vol) in fv.volumes().iter().enumerate() {
            let r_delta_t = at(gas.r_delta_t, i);
            if let Some(d) = eqn.ldu.diag.get_mut(i) {
                *d += r_delta_t * at(gas.rho, i) * vol;
            }
            if let Some(s) = eqn.ldu.source.get_mut(i) {
                *s += r_delta_t * at(gas.rho_old, i) * at(&self.nu_tilda.cells, i) * vol;
            }
        }
        eqn.sub(&rhs(
            mesh,
            fv,
            &su,
            &vec![0.0; n],
            &sp,
            &self.nu_tilda.cells,
        ));
        let solved = solve(
            mesh,
            eqn,
            &mut self.nu_tilda,
            &self.eqn,
            &[],
            "nuTilda",
            log,
        )?;
        bound_at(mesh, fv, "nuTilda", &mut self.nu_tilda, 0.0, log);
        self.correct_nut(mesh, fv, (u, u_bc), gas);
        Ok(vec![solved])
    }

    pub(crate) fn fields(&self) -> [&ModelField; 3] {
        [&self.nu_tilda, &self.nut, &self.alphat]
    }

    /// Writes ν̃, ν_t and α_t into `dir/time`.
    pub(crate) fn write(&self, mesh: &PolyMesh, dir: &std::path::Path, time: &str) -> Result<()> {
        for field in self.fields() {
            let mut out = field.to_write(mesh);
            out.drop_zero_gradient_values();
            out.write(dir, time)?;
        }
        Ok(())
    }
}
