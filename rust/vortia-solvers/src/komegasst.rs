//! Menter's k-ω SST model with wall functions, as `simpleFoam` runs it.
//!
//! Derived from OpenFOAM v1912's `kOmegaSSTBase` and `omegaWallFunction`
//! (OpenFOAM, GPL-3.0-or-later), each step in OpenFOAM's order and with its
//! arithmetic:
//!
//! 1. `S2 = 2|symm(∇U)|²`, `G = ν_t (∇U && dev(twoSymm(∇U)))`;
//! 2. the ω wall function replaces ω and `G` in the cells beside walls;
//! 3. the cross-diffusion `CDkω` and the blending functions `F1` and `F2`,
//!    from the distance to the nearest wall;
//! 4. the ω equation, relaxed, with the wall cells fixed, solved and bounded;
//! 5. the k equation with its production limited, solved and bounded;
//! 6. `ν_t = a1 k / max(a1 ω, b1 F2 √S2)`, and on the walls from the log law.

use vortia_core::Vec3;
use vortia_foamfile::Dictionary;
use vortia_fv::limited::GradScheme;
use vortia_fv::limited::grad as limited_grad;
use vortia_fv::ops::surface_integrate;
use vortia_fv::stress::{Tensor, grad, grad_with, production};
use vortia_fv::vector::VectorBoundary;
use vortia_fv::{FvGeometry, Performance, ScalarMatrix};
use vortia_mesh::PolyMesh;
use vortia_mesh::wall::wall_distance;

use crate::case::Case;
use crate::error::{Result, SolverError};
use crate::ico::switch_or;
use crate::kepsilon::{calculated_faces, read_k, read_nut};
use crate::rans::{
    EquationSetup, InletSource, Kind, LogLaw, ModelField, WallFace, bound, correct_nut_walls,
    face_diffusivity, near_wall, pow025, rhs, solve, transport, wall_function,
};

/// `[0 0 -1 0 0 0 0]`.
const FREQUENCY: [i32; 7] = [0, 0, -1, 0, 0, 0, 0];

/// The model's coefficients, `kOmegaSSTCoeffs` over OpenFOAM's defaults.
#[derive(Debug, Clone, Copy)]
struct Coeffs {
    alpha_k1: f64,
    alpha_k2: f64,
    alpha_omega1: f64,
    alpha_omega2: f64,
    gamma1: f64,
    gamma2: f64,
    beta1: f64,
    beta2: f64,
    beta_star: f64,
    a1: f64,
    b1: f64,
    c1: f64,
    cmu: f64,
    f3: bool,
}

impl Coeffs {
    fn read(d: Option<&Dictionary>) -> Result<Self> {
        let get = |k: &str, def: f64| d.and_then(|d| d.scalar(k)).unwrap_or(def);
        let flag = |k: &str| d.map_or(Ok(false), |d| switch_or(d, k, false, "kOmegaSST"));
        if flag("decayControl")? {
            return Err(SolverError::Setup(
                "kOmegaSST decayControl is not supported yet".into(),
            ));
        }
        Ok(Self {
            alpha_k1: get("alphaK1", 0.85),
            alpha_k2: get("alphaK2", 1.0),
            alpha_omega1: get("alphaOmega1", 0.5),
            alpha_omega2: get("alphaOmega2", 0.856),
            gamma1: get("gamma1", 5.0 / 9.0),
            gamma2: get("gamma2", 0.44),
            beta1: get("beta1", 0.075),
            beta2: get("beta2", 0.0828),
            beta_star: get("betaStar", 0.09),
            a1: get("a1", 0.31),
            b1: get("b1", 1.0),
            c1: get("c1", 10.0),
            cmu: get("Cmu", 0.09),
            f3: flag("F3")?,
        })
    }
}

/// An `inletOutlet`-style condition, filled in as the field is read.
fn mixed(source: InletSource) -> Kind {
    Kind::Mixed {
        source,
        ref_value: Vec::new(),
        fraction: Vec::new(),
    }
}

/// `F1 (ψ1 − ψ2) + ψ2`.
fn blend(f1: f64, psi1: f64, psi2: f64) -> f64 {
    f1 * (psi1 - psi2) + psi2
}

/// `x⁴` as OpenFOAM's `pow4`, the square of the square.
fn pow4(x: f64) -> f64 {
    let s = x * x;
    s * s
}

/// `2 |symm(g)|²`.
fn s2(g: &Tensor) -> f64 {
    let [[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]] = *g;
    let (sxy, sxz, syz) = (0.5 * (xy + yx), 0.5 * (xz + zx), 0.5 * (yz + zy));
    2.0 * (xx * xx + 2.0 * (sxy * sxy) + 2.0 * (sxz * sxz) + yy * yy + 2.0 * (syz * syz) + zz * zz)
}

/// The k-ω SST model of a run: its fields and how it solves them.
pub(crate) struct KOmegaSst {
    c: Coeffs,
    /// ν in the cells and on the boundary faces: one number, or `μ/ρ`.
    nu: Vec<f64>,
    nu_b: Vec<Option<f64>>,
    /// The gas the model runs in, when the equations are ρ-weighted and the
    /// flux is a mass flux.
    density: Option<Density>,
    /// `grad(U)`, `grad(k)` and `grad(omega)`.
    u_grad: GradScheme,
    k_grad: GradScheme,
    omega_grad: GradScheme,
    k: ModelField,
    omega: ModelField,
    nut: ModelField,
    /// The distance to the nearest wall of every cell.
    y_cells: Vec<f64>,
    /// ... and of every boundary face (`None` on empty faces).
    y_faces: Vec<Option<f64>>,
    /// The distance to the wall of each face of each wall-function patch.
    y_wall: Vec<Vec<f64>>,
    k_eqn: EquationSetup,
    omega_eqn: EquationSetup,
}

/// The density of the gas, in the cells and on the boundary faces.
struct Density {
    cells: Vec<f64>,
    faces: Vec<Option<f64>>,
}

/// What the model runs in: a liquid of constant kinematic viscosity, or a
/// gas with its density and dynamic viscosity.
#[derive(Clone, Copy)]
pub(crate) enum Medium<'a> {
    Uniform(f64),
    Gas {
        rho: &'a [f64],
        rho_b: &'a [Option<f64>],
        mu: &'a [f64],
        mu_b: &'a [Option<f64>],
    },
}

/// A field in the cells and on the boundary faces.
struct Both {
    cells: Vec<f64>,
    faces: Vec<Option<f64>>,
}

impl KOmegaSst {
    /// Reads k, ω and ν_t from `dir`, the distance to the walls, and sets
    /// ν_t as OpenFOAM's `validate` does before the first iteration.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn read(
        case: &Case,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        dir: &std::path::Path,
        coeffs: Option<&Dictionary>,
        medium: Medium,
        [relax_k, relax_omega]: [Option<f64>; 2],
        (u_bc, u, unevaluated): (&VectorBoundary, &[Vec3], &[usize]),
    ) -> Result<Self> {
        check_wall_dist(case)?;
        let k = read_k(mesh, dir, true)?;
        let omega = ModelField::read(mesh, dir, "omega", FREQUENCY, |pf| match pf.kind.as_str() {
            "fixedValue" => Some(Kind::Fixed(Vec::new())),
            "zeroGradient" => Some(Kind::ZeroGradient),
            "inletOutlet" => Some(mixed(InletSource::Value)),
            "turbulentMixingLengthFrequencyInlet" => {
                let length = pf.settings.scalar("mixingLength")?;
                if pf.settings.word("k").is_some_and(|k| k != "k")
                    || pf.settings.word("phi").is_some_and(|p| p != "phi")
                {
                    return None;
                }
                Some(mixed(InletSource::MixingLength(length)))
            }
            "omegaWallFunction" => {
                let blended = switch_or(&pf.settings, "blended", true, "").ok()?;
                let beta1 = pf.settings.scalar("beta1").unwrap_or(0.075);
                Some(Kind::OmegaWall { blended, beta1 })
            }
            _ => None,
        })?;
        let nut = read_nut(mesh, dir, &omega, "omegaWallFunction")?;
        let y = wall_distance(mesh, &fv.geo.cell_centres, &fv.geo.face_centres)?;
        let mut model = Self {
            c: Coeffs::read(coeffs)?,
            nu: Vec::new(),
            nu_b: Vec::new(),
            density: None,
            u_grad: case.grad_scheme("grad(U)")?,
            k_grad: case.grad_scheme("grad(k)")?,
            omega_grad: case.grad_scheme("grad(omega)")?,
            k,
            omega,
            y_cells: y.cells,
            y_faces: y.faces,
            y_wall: near_wall(mesh, fv, &nut)?,
            nut,
            k_eqn: EquationSetup::read(case, "k", "DkEff", relax_k)?,
            omega_eqn: EquationSetup::read(case, "omega", "DomegaEff", relax_omega)?,
        };
        model.set_medium(medium);
        let mut sink = |_: &str| {};
        bound(mesh, fv, "k", &mut model.k, &mut sink);
        bound(mesh, fv, "omega", &mut model.omega, &mut sink);
        let (grad_u, grad_u_b) = grad_with(model.u_grad, mesh, fv, u_bc, u, unevaluated);
        model.correct_nut(mesh, &strain(&grad_u, &grad_u_b));
        Ok(model)
    }

    pub(crate) fn nut(&self) -> &ModelField {
        &self.nut
    }

    /// The medium the model runs in, as of this iteration: `ν` is `μ/ρ`
    /// with the density the gas has now.
    pub(crate) fn set_medium(&mut self, medium: Medium) {
        match medium {
            Medium::Uniform(nu) => {
                self.nu = vec![nu; self.k.cells.len()];
                self.nu_b = self.k.faces.iter().map(|f| f.map(|_| nu)).collect();
                self.density = None;
            }
            Medium::Gas {
                rho,
                rho_b,
                mu,
                mu_b,
            } => {
                self.nu = mu.iter().zip(rho).map(|(&m, &r)| m / r).collect();
                self.nu_b = mu_b
                    .iter()
                    .zip(rho_b)
                    .map(|(m, r)| m.zip(*r).map(|(m, r)| m / r))
                    .collect();
                self.density = Some(Density {
                    cells: rho.to_vec(),
                    faces: rho_b.to_vec(),
                });
            }
        }
    }

    /// `ν_t + ν` in the cells.
    pub(crate) fn nu_eff_cells(&self) -> Vec<f64> {
        self.nut
            .cells
            .iter()
            .zip(&self.nu)
            .map(|(&t, &nu)| t + nu)
            .collect()
    }

    /// `ν_t + ν` on the boundary faces.
    pub(crate) fn nu_eff_faces(&self) -> Vec<Option<f64>> {
        self.nut
            .faces
            .iter()
            .zip(&self.nu_b)
            .map(|(t, nu)| t.zip(*nu).map(|(t, nu)| t + nu))
            .collect()
    }

    /// Writes k, ω and ν_t into `dir/time`.
    pub(crate) fn write(&self, mesh: &PolyMesh, dir: &std::path::Path, time: &str) -> Result<()> {
        for field in self.fields() {
            let mut out = field.to_write(mesh);
            out.drop_zero_gradient_values();
            out.write(dir, time)?;
        }
        Ok(())
    }

    pub(crate) fn fields(&self) -> [&ModelField; 3] {
        [&self.k, &self.omega, &self.nut]
    }

    /// `F2`, times `F3` when the model asks for it, in a cell or on a face.
    fn f23(&self, k: f64, omega: f64, y: f64, nu: f64) -> f64 {
        let c = &self.c;
        let viscous = (500.0 * nu) / ((y * y) * omega);
        let arg2 = ((((2.0 / c.beta_star) * k.sqrt()) / (omega * y)).max(viscous)).min(100.0);
        let f2 = (arg2 * arg2).tanh();
        if c.f3 {
            let arg3 = ((150.0 * nu) / (omega * (y * y))).min(10.0);
            f2 * (1.0 - pow4(arg3).tanh())
        } else {
            f2
        }
    }

    /// `F1` in a cell or on a face, from `CDkω` there.
    fn f1(&self, k: f64, omega: f64, y: f64, nu: f64, cd_k_omega: f64) -> f64 {
        let c = &self.c;
        let plus = cd_k_omega.max(1.0e-10);
        let arg1 = ((((1.0 / c.beta_star) * k.sqrt()) / (omega * y))
            .max((500.0 * nu) / ((y * y) * omega)))
        .min(((4.0 * c.alpha_omega2) * k) / (plus * (y * y)))
        .min(10.0);
        pow4(arg1).tanh()
    }

    /// `F23` in the cells and on the faces, from the current k and ω.
    fn f23_field(&self) -> Both {
        Both {
            cells: (0..self.k.cells.len())
                .map(|c| {
                    self.f23(
                        self.k.cells.get(c).copied().unwrap_or(0.0),
                        self.omega.cells.get(c).copied().unwrap_or(0.0),
                        self.y_cells.get(c).copied().unwrap_or(0.0),
                        self.nu.get(c).copied().unwrap_or(0.0),
                    )
                })
                .collect(),
            faces: (0..self.k.faces.len())
                .map(|b| {
                    Some(self.f23(
                        self.k.faces.get(b).copied().flatten()?,
                        self.omega.faces.get(b).copied().flatten()?,
                        self.y_faces.get(b).copied().flatten()?,
                        self.nu_b.get(b).copied().flatten()?,
                    ))
                })
                .collect(),
        }
    }

    /// `ν_t = a1 k / max(a1 ω, b1 F23 √S2)` in the cells and on calculated
    /// patches, with F23 from the current k and ω, and the log law on walls.
    fn correct_nut(&mut self, mesh: &PolyMesh, s2: &Both) {
        let (a1, b1) = (self.c.a1, self.c.b1);
        let f23 = self.f23_field();
        let nut =
            |k: f64, omega: f64, f: f64, s: f64| (a1 * k) / (a1 * omega).max((b1 * f) * s.sqrt());
        for (c, t) in self.nut.cells.iter_mut().enumerate() {
            let get = |v: &[f64]| v.get(c).copied().unwrap_or(0.0);
            *t = nut(
                get(&self.k.cells),
                get(&self.omega.cells),
                get(&f23.cells),
                get(&s2.cells),
            );
        }
        let (k, omega) = (&self.k.faces, &self.omega.faces);
        calculated_faces(mesh, &mut self.nut, |b| {
            let get = |v: &[Option<f64>]| v.get(b).copied().flatten();
            Some(nut(get(k)?, get(omega)?, get(&f23.faces)?, get(&s2.faces)?))
        });
        let nu_b = &self.nu_b;
        let nu = |b: usize| nu_b.get(b).copied().flatten().unwrap_or(0.0);
        correct_nut_walls(mesh, &mut self.nut, &self.k.cells, &self.y_wall, nu);
        // The wedge faces take the cell values the way `correctBoundaryConditions` does.
        self.nut.follow_cells(mesh);
    }

    /// `α(F1) ν_t + ν` linearly on every face, `α` blended between
    /// `alpha1` and `alpha2`.
    fn diffusivity(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        f1: &Both,
        alpha1: f64,
        alpha2: f64,
    ) -> Vec<f64> {
        let rho =
            |v: Option<&Vec<f64>>, i: usize| v.map_or(1.0, |v| v.get(i).copied().unwrap_or(1.0));
        let cells: Vec<f64> = f1
            .cells
            .iter()
            .zip(&self.nut.cells)
            .zip(&self.nu)
            .enumerate()
            .map(|(i, ((&f, &t), &nu))| {
                let d = blend(f, alpha1, alpha2) * t + nu;
                match &self.density {
                    Some(r) => rho(Some(&r.cells), i) * d,
                    None => d,
                }
            })
            .collect();
        let faces: Vec<Option<f64>> = f1
            .faces
            .iter()
            .zip(&self.nut.faces)
            .zip(&self.nu_b)
            .enumerate()
            .map(|(b, ((f, t), nu))| {
                let d = blend((*f)?, alpha1, alpha2) * (*t)? + (*nu)?;
                match &self.density {
                    Some(r) => Some(r.faces.get(b).copied().flatten()? * d),
                    None => Some(d),
                }
            })
            .collect();
        face_diffusivity(mesh, fv, &cells, &faces)
    }

    /// One `turbulence->correct()`: the ω and k equations, then ν_t. Logs
    /// each solve and returns it. `phi` is the mass flux in a gas and the
    /// volumetric flux otherwise.
    pub(crate) fn correct(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        u: &[Vec3],
        u_bc: &VectorBoundary,
        phi: &[f64],
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<(String, Performance)>> {
        let c = self.c;
        let n = mesh.n_cells();
        // ρ in a gas, 1 otherwise: the equations are ρ times the liquid's.
        let rho: Vec<f64> = match &self.density {
            Some(d) => d.cells.clone(),
            None => vec![1.0; n],
        };
        // `divU`, from the volumetric flux `phi/ρ_f`.
        let div_u = match &self.density {
            Some(d) => {
                let rho_f = face_diffusivity(mesh, fv, &d.cells, &d.faces);
                let phi_vol: Vec<f64> = phi.iter().zip(&rho_f).map(|(&f, &r)| f / r).collect();
                surface_integrate(mesh, fv, &phi_vol)
            }
            None => surface_integrate(mesh, fv, phi),
        };
        let (grad_u, grad_u_b) = grad(self.u_grad, mesh, fv, u_bc, u);
        let s2 = strain(&grad_u, &grad_u_b);
        let g_by_nu0: Vec<f64> = grad_u.iter().map(production).collect();
        let mut g: Vec<f64> = self
            .nut
            .cells
            .iter()
            .zip(&g_by_nu0)
            .map(|(&t, &p)| t * p)
            .collect();
        let u_b = u_bc.face_values(mesh, u);

        // ω's conditions that follow the flow take the k of the faces now.
        self.omega
            .update_inlets(mesh, phi, &u_b, Some(&self.k.faces), c.cmu);
        let nu_b = &self.nu_b;
        let wall_cells = wall_function(
            mesh,
            fv,
            &mut self.omega,
            &self.nut,
            &self.k.cells,
            &self.y_wall,
            |b| nu_b.get(b).copied().flatten().unwrap_or(0.0),
            (u, u_bc),
            &mut g,
            omega_wall,
        );

        // CDkω = 2 αω2 (∇k · ∇ω)/ω, then F1 and F23.
        let (gk, gk_b) = boundary_grad(mesh, fv, self.k_grad, &self.k);
        let (gw, gw_b) = boundary_grad(mesh, fv, self.omega_grad, &self.omega);
        let cd = |a: Vec3, b: Vec3, omega: f64| ((2.0 * c.alpha_omega2) * a.dot(b)) / omega;
        let f1 = Both {
            cells: (0..n)
                .map(|i| {
                    let get = |v: &[f64]| v.get(i).copied().unwrap_or(0.0);
                    let at = |v: &[Vec3]| v.get(i).copied().unwrap_or_default();
                    let omega = get(&self.omega.cells);
                    self.f1(
                        get(&self.k.cells),
                        omega,
                        get(&self.y_cells),
                        get(&self.nu),
                        cd(at(&gk), at(&gw), omega),
                    )
                })
                .collect(),
            faces: (0..self.k.faces.len())
                .map(|b| {
                    let get = |v: &[Option<f64>]| v.get(b).copied().flatten();
                    let at = |v: &[Option<Vec3>]| v.get(b).copied().flatten();
                    let omega = get(&self.omega.faces)?;
                    Some(self.f1(
                        get(&self.k.faces)?,
                        omega,
                        get(&self.y_faces)?,
                        get(&self.nu_b)?,
                        cd(at(&gk_b)?, at(&gw_b)?, omega),
                    ))
                })
                .collect(),
        };
        let cd_cells: Vec<f64> = (0..n)
            .map(|i| {
                cd(
                    gk.get(i).copied().unwrap_or_default(),
                    gw.get(i).copied().unwrap_or_default(),
                    self.omega.cells.get(i).copied().unwrap_or(0.0),
                )
            })
            .collect();
        let f23 = self.f23_field();

        // ω: (div − laplacian) − (ρ γ GbyNu − SuSp − Sp − SuSp).
        let gamma = self.diffusivity(mesh, fv, &f1, c.alpha_omega1, c.alpha_omega2);
        let mut omega_eqn = transport(mesh, fv, &self.omega, &self.omega_eqn, phi, &gamma)?;
        let mut su = Vec::with_capacity(n);
        let mut susp = Vec::with_capacity(n);
        let mut sp = Vec::with_capacity(n);
        let mut cross = Vec::with_capacity(n);
        for i in 0..n {
            let get = |v: &[f64]| v.get(i).copied().unwrap_or(0.0);
            let (f1c, omega, rho) = (get(&f1.cells), get(&self.omega.cells), get(&rho));
            let gamma_c = blend(f1c, c.gamma1, c.gamma2);
            let beta_c = blend(f1c, c.beta1, c.beta2);
            let limit = (((c.c1 / c.a1) * c.beta_star) * omega)
                * (c.a1 * omega).max((c.b1 * get(&f23.cells)) * get(&s2.cells).sqrt());
            let weighted = rho * gamma_c;
            su.push(weighted * get(&g_by_nu0).min(limit));
            susp.push((((2.0 / 3.0) * rho) * gamma_c) * get(&div_u));
            sp.push((rho * beta_c) * omega);
            cross.push(((rho * (f1c - 1.0)) * get(&cd_cells)) / omega);
        }
        let mut source = rhs(mesh, fv, &su, &susp, &sp, &self.omega.cells);
        source.sub(&ScalarMatrix::su_sp(mesh, fv, &cross, &self.omega.cells));
        omega_eqn.sub(&source);
        let mut solves = vec![solve(
            mesh,
            omega_eqn,
            &mut self.omega,
            &self.omega_eqn,
            &wall_cells,
            "omega",
            log,
        )?];
        bound(mesh, fv, "omega", &mut self.omega, log);

        // k: (div − laplacian) − (ρ min(G, c1 β* k ω) − SuSp − Sp).
        self.k.update_inlets(mesh, phi, &u_b, None, c.cmu);
        let gamma = self.diffusivity(mesh, fv, &f1, c.alpha_k1, c.alpha_k2);
        let mut k_eqn = transport(mesh, fv, &self.k, &self.k_eqn, phi, &gamma)?;
        let pk: Vec<f64> = g
            .iter()
            .zip(self.k.cells.iter().zip(&self.omega.cells))
            .zip(&rho)
            .map(|((&g, (&k, &omega)), &rho)| rho * g.min(((c.c1 * c.beta_star) * k) * omega))
            .collect();
        let susp: Vec<f64> = div_u
            .iter()
            .zip(&rho)
            .map(|(&d, &rho)| ((2.0 / 3.0) * rho) * d)
            .collect();
        let sp: Vec<f64> = self
            .omega
            .cells
            .iter()
            .zip(&rho)
            .map(|(&w, &rho)| rho * (c.beta_star * w))
            .collect();
        k_eqn.sub(&rhs(mesh, fv, &pk, &susp, &sp, &self.k.cells));
        solves.push(solve(mesh, k_eqn, &mut self.k, &self.k_eqn, &[], "k", log)?);
        bound(mesh, fv, "k", &mut self.k, log);

        self.correct_nut(mesh, &s2);
        Ok(solves)
    }
}

/// `2 |symm(∇U)|²` in the cells and on the faces.
fn strain(cells: &[Tensor], faces: &[Option<Tensor>]) -> Both {
    Both {
        cells: cells.iter().map(s2).collect(),
        faces: faces.iter().map(|t| t.as_ref().map(s2)).collect(),
    }
}

/// `omegaWallFunction`'s ω and production for one wall face.
fn omega_wall(kind: &Kind, law: &LogLaw, f: &WallFace) -> (f64, f64) {
    let Kind::OmegaWall { blended, beta1 } = *kind else {
        return (0.0, 0.0);
    };
    let cmu25 = pow025(law.cmu);
    let y_plus = ((cmu25 * f.y) * f.k.sqrt()) / f.nuw;
    let omega_vis = (6.0 * f.nuw) / (beta1 * (f.y * f.y));
    let omega_log = f.k.sqrt() / ((cmu25 * law.kappa) * f.y);
    let g = ((((f.w * (f.nutw + f.nuw)) * f.mag_grad_uw) * cmu25) * f.k.sqrt()) / (law.kappa * f.y);
    if blended {
        (
            f.w * (omega_vis * omega_vis + omega_log * omega_log).sqrt(),
            g,
        )
    } else if law.y_plus_lam < y_plus {
        (f.w * omega_log, g)
    } else {
        (f.w * omega_vis, 0.0)
    }
}

/// The `Gauss linear` gradient of a model field in the cells and on the
/// boundary faces, where its normal part is the face's own normal gradient,
/// as OpenFOAM's `gaussGrad::correctBoundaryConditions` sets it.
fn boundary_grad(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    scheme: GradScheme,
    field: &ModelField,
) -> (Vec<Vec3>, Vec<Option<Vec3>>) {
    let cells = limited_grad(scheme, mesh, fv, &field.cells, &field.faces);
    let n_internal = mesh.n_internal_faces();
    let mut faces = Vec::with_capacity(field.faces.len());
    for (patch, kind) in mesh.patches().iter().zip(&field.kinds) {
        for f in patch.faces() {
            let value = field.faces.get(f - n_internal).copied().flatten();
            let (Some(vb), Some(&c)) = (value, mesh.owner().get(f)) else {
                faces.push(None);
                continue;
            };
            let gc = cells.get(c).copied().unwrap_or_default();
            let s = fv.geo.face_areas.get(f).copied().unwrap_or_default();
            let mag = fv.mag_sf.get(f).copied().unwrap_or(1.0);
            let n = Vec3::new(s.x / mag, s.y / mag, s.z / mag);
            let sn = match kind {
                Kind::ZeroGradient => 0.0,
                _ => {
                    let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                    dc * (vb - field.cells.get(c).copied().unwrap_or(0.0))
                }
            };
            faces.push(Some(gc + n * (sn - n.dot(gc))));
        }
    }
    (cells, faces)
}

/// Refuses a wall distance other than `meshWave` with its walls corrected,
/// the one the engine has.
fn check_wall_dist(case: &Case) -> Result<()> {
    let d = case.schemes.dict("wallDist").ok_or_else(|| {
        SolverError::Setup("kOmegaSST needs fvSchemes/wallDist { method meshWave; }".into())
    })?;
    if d.word("method") != Some("meshWave") {
        return Err(SolverError::Setup(format!(
            "wallDist method '{}' is not supported yet; use meshWave",
            d.word("method").unwrap_or("")
        )));
    }
    if !switch_or(d, "correctWalls", true, "wallDist")? {
        return Err(SolverError::Setup(
            "wallDist without correctWalls is not supported yet".into(),
        ));
    }
    Ok(())
}
