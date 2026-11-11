//! A compressible gas: its equation of state, its energy and transport, and
//! the fields that follow from the temperature and pressure.
//!
//! Derived from OpenFOAM v1912's `hePsiThermo`, `heThermo`, `species::thermo`,
//! `hConstThermo`, `perfectGas`, `constTransport`, and the `fixedEnergy` and
//! `gradientEnergy` conditions (OpenFOAM, GPL-3.0-or-later), each in
//! OpenFOAM's order and arithmetic so the fields agree with OpenFOAM's to
//! rounding. The engine has the one combination the nozzle cases use: a
//! single perfect gas with constant `Cp`, `μ` and `Pr`.

use vortia_fields::{Data, ScalarField};
use vortia_foamfile::Dictionary;
use vortia_fv::{Boundary, FvGeometry, ScalarBc};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::case::{Case, check_dimensions, read_dict};
use crate::error::{Result, SolverError};

/// Avogadro's number and Boltzmann's constant, as OpenFOAM v1912 takes
/// them; the universal gas constant is their product, per kmol.
const N_A: f64 = 6.022_141_793_0e23;
const K_B: f64 = 1.380_65e-23;
/// The relative tolerance and iteration limit of the temperature's Newton
/// iteration (`species::thermo`'s `tol_` and `maxIter_`).
const T_TOL: f64 = 1.0e-4;
const T_MAX_ITER: usize = 100;

pub(crate) const TEMPERATURE: [i32; 7] = [0, 0, 0, 1, 0, 0, 0];
pub(crate) const PRESSURE: [i32; 7] = [1, -1, -2, 0, 0, 0, 0];
pub(crate) const DENSITY: [i32; 7] = [1, -3, 0, 0, 0, 0, 0];

/// Which energy the equation is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Energy {
    /// `sensibleInternalEnergy`, the field `e`.
    Internal,
    /// `sensibleEnthalpy`, the field `h`.
    Enthalpy,
}

impl Energy {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Internal => "e",
            Self::Enthalpy => "h",
        }
    }
}

/// A perfect gas with constant `Cp`, `μ` and `Pr`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Gas {
    pub(crate) energy: Energy,
    r: f64,
    cp: f64,
    mu: f64,
    r_pr: f64,
}

impl Gas {
    /// Reads `constant/thermophysicalProperties`, refusing what the engine
    /// does not have.
    pub(crate) fn read(case: &Case) -> Result<Self> {
        let d = read_dict(&case.dir.join("constant").join("thermophysicalProperties"))?;
        let thermo = d.dict("thermoType").ok_or_else(|| {
            SolverError::Setup("thermophysicalProperties has no thermoType".into())
        })?;
        for (key, want) in [
            ("type", "hePsiThermo"),
            ("mixture", "pureMixture"),
            ("transport", "const"),
            ("thermo", "hConst"),
            ("equationOfState", "perfectGas"),
            ("specie", "specie"),
        ] {
            let got = thermo.word(key).unwrap_or("");
            if got != want {
                return Err(SolverError::Setup(format!(
                    "thermoType {key} '{got}' is not supported yet; use {want}"
                )));
            }
        }
        let energy = match thermo.word("energy") {
            Some("sensibleInternalEnergy") => Energy::Internal,
            Some("sensibleEnthalpy") => Energy::Enthalpy,
            other => {
                return Err(SolverError::Setup(format!(
                    "energy '{}' is not supported yet; use sensibleInternalEnergy or \
                     sensibleEnthalpy",
                    other.unwrap_or("")
                )));
            }
        };
        let mixture = d
            .dict("mixture")
            .ok_or_else(|| SolverError::Setup("thermophysicalProperties has no mixture".into()))?;
        let get = |sub: &str, key: &str| -> Result<f64> {
            mixture
                .dict(sub)
                .and_then(|s: &Dictionary| s.scalar(key))
                .filter(|v| v.is_finite())
                .ok_or_else(|| SolverError::Setup(format!("mixture has no {sub}/{key}")))
        };
        let positive = |what: &str, v: f64| {
            if v > 0.0 {
                Ok(v)
            } else {
                Err(SolverError::Setup(format!(
                    "{what} must be positive, not {v}"
                )))
            }
        };
        let w = positive("molWeight", get("specie", "molWeight")?)?;
        let cp = positive("Cp", get("thermodynamics", "Cp")?)?;
        let mu = positive("mu", get("transport", "mu")?)?;
        let pr = positive("Pr", get("transport", "Pr")?)?;
        let rr = 1.0e3 * (N_A * K_B);
        let gas = Self {
            energy,
            r: rr / w,
            cp,
            mu,
            r_pr: 1.0 / pr,
        };
        if gas.cp - gas.r <= 0.0 {
            return Err(SolverError::Setup(format!(
                "Cp {cp} must be above the gas constant {}",
                gas.r
            )));
        }
        Ok(gas)
    }

    pub(crate) fn rho(&self, p: f64, t: f64) -> f64 {
        p / (self.r * t)
    }

    pub(crate) fn psi(&self, _p: f64, t: f64) -> f64 {
        1.0 / (self.r * t)
    }

    fn cp(&self) -> f64 {
        self.cp
    }

    fn cv(&self) -> f64 {
        self.cp() - self.r
    }

    /// `Cp/Cpv`: γ for internal energy, 1 for enthalpy.
    pub(crate) fn cp_by_cpv(&self) -> f64 {
        match self.energy {
            Energy::Internal => {
                let cp = self.cp();
                cp / (cp - self.r)
            }
            Energy::Enthalpy => 1.0,
        }
    }

    /// The heat capacity of the energy solved for.
    fn cpv(&self) -> f64 {
        match self.energy {
            Energy::Internal => self.cv(),
            Energy::Enthalpy => self.cp(),
        }
    }

    /// Sensible enthalpy; OpenFOAM v1912's `hConst` measures it from 0 K.
    fn hs(&self, t: f64) -> f64 {
        self.cp * t
    }

    fn es(&self, p: f64, t: f64) -> f64 {
        self.hs(t) - p / self.rho(p, t)
    }

    /// The energy solved for at `p` and `t`.
    pub(crate) fn he(&self, p: f64, t: f64) -> f64 {
        match self.energy {
            Energy::Internal => self.es(p, t),
            Energy::Enthalpy => self.hs(t),
        }
    }

    /// The temperature with energy `he` at `p`, by Newton's method from
    /// `t0`, as `species::thermo::T`.
    pub(crate) fn the(&self, he: f64, p: f64, t0: f64) -> Result<f64> {
        if t0.is_nan() || t0 < 0.0 {
            return Err(SolverError::Setup(format!(
                "negative initial temperature {t0}"
            )));
        }
        let tol = t0 * T_TOL;
        let mut t_new = t0;
        let mut iter = 0;
        loop {
            let t = t_new;
            t_new = t - (self.he(p, t) - he) / self.cpv();
            if !t_new.is_finite() {
                return Err(SolverError::Setup(format!(
                    "the temperature diverged from energy {he} at pressure {p}"
                )));
            }
            if iter > T_MAX_ITER {
                return Err(SolverError::Setup(format!(
                    "the temperature did not converge from energy {he} at pressure {p}"
                )));
            }
            iter += 1;
            if (t_new - t).abs() <= tol {
                return Ok(t_new);
            }
        }
    }

    pub(crate) fn mu(&self) -> f64 {
        self.mu
    }

    /// `α = κ/Cp = μ/Pr`.
    pub(crate) fn alphah(&self) -> f64 {
        self.mu * self.r_pr
    }
}

/// How each patch treats the temperature.
#[derive(Debug, Clone, PartialEq)]
enum TKind {
    Fixed,
    ZeroGradient,
    Empty,
}

/// The temperature, the energy and the properties that follow from them,
/// in every cell and on every boundary face (`None` on empty faces).
pub(crate) struct Thermo {
    pub(crate) gas: Gas,
    file: ScalarField,
    kinds: Vec<TKind>,
    pub(crate) t: Vec<f64>,
    pub(crate) t_b: Vec<Option<f64>>,
    pub(crate) he: Vec<f64>,
    pub(crate) he_b: Vec<Option<f64>>,
    pub(crate) psi: Vec<f64>,
    pub(crate) psi_b: Vec<Option<f64>>,
    pub(crate) mu: Vec<f64>,
    pub(crate) mu_b: Vec<Option<f64>>,
    pub(crate) alpha: Vec<f64>,
    pub(crate) alpha_b: Vec<Option<f64>>,
}

impl Thermo {
    /// Reads `T` from `dir` and sets up the energy from it at pressure
    /// `p` (cells) and `p_b` (boundary faces), as the constructors of
    /// `heThermo` and `hePsiThermo` do.
    pub(crate) fn read(
        gas: Gas,
        mesh: &PolyMesh,
        dir: &std::path::Path,
        (p, p_b): (&[f64], &[Option<f64>]),
    ) -> Result<Self> {
        let file = ScalarField::read(dir.join("T"))?;
        check_dimensions("T", &file.dimensions, TEMPERATURE)?;
        vortia_fv::bc::check_solvable(&file)?;
        file.check_against(mesh)?;
        let t = file
            .internal
            .expand(mesh.n_cells())
            .ok_or_else(|| SolverError::Setup("T does not fit the mesh".into()))?;
        let mut kinds = Vec::new();
        let mut t_b = Vec::new();
        for patch in mesh.patches() {
            let pf = file.patch_field(patch).ok_or_else(|| {
                SolverError::Setup(format!("T: no condition for patch '{}'", patch.name))
            })?;
            let kind = match pf.kind.as_str() {
                "fixedValue" => TKind::Fixed,
                "zeroGradient" => TKind::ZeroGradient,
                "empty" => TKind::Empty,
                other => {
                    return Err(SolverError::Setup(format!(
                        "T: '{other}' on patch '{}' is not supported yet",
                        patch.name
                    )));
                }
            };
            if (patch.kind == PatchKind::Empty) != (kind == TKind::Empty) {
                return Err(SolverError::Setup(format!(
                    "T: patch '{}' is {} but its condition is '{}'",
                    patch.name,
                    patch.kind.name(),
                    pf.kind
                )));
            }
            let given = pf.value.as_ref().and_then(|d| d.expand(patch.size));
            if kind == TKind::Fixed && given.is_none() {
                return Err(SolverError::Setup(format!(
                    "T: patch '{}' needs a 'value' of {} items",
                    patch.name, patch.size
                )));
            }
            for (i, f) in patch.faces().enumerate() {
                let inside = mesh.owner().get(f).and_then(|&c| t.get(c)).copied();
                t_b.push(match kind {
                    TKind::Empty => None,
                    TKind::Fixed => given.as_ref().and_then(|v| v.get(i).copied()),
                    TKind::ZeroGradient => inside,
                });
            }
            kinds.push(kind);
        }
        let he: Vec<f64> = t.iter().zip(p).map(|(&t, &p)| gas.he(p, t)).collect();
        let he_b = t_b
            .iter()
            .zip(p_b)
            .map(|(t, p)| t.zip(*p).map(|(t, p)| gas.he(p, t)))
            .collect();
        let n = mesh.n_cells();
        let nb = t_b.len();
        let mut thermo = Self {
            gas,
            file,
            kinds,
            t,
            t_b,
            he,
            he_b,
            psi: vec![0.0; n],
            psi_b: vec![None; nb],
            mu: vec![0.0; n],
            mu_b: vec![None; nb],
            alpha: vec![0.0; n],
            alpha_b: vec![None; nb],
        };
        thermo.correct(mesh, p, p_b)?;
        Ok(thermo)
    }

    /// `hePsiThermo::calculate`: the temperature from the energy, then the
    /// properties, in the cells and on the boundary. A fixed temperature
    /// sets the energy on its faces; elsewhere the energy sets the
    /// temperature.
    pub(crate) fn correct(
        &mut self,
        mesh: &PolyMesh,
        p: &[f64],
        p_b: &[Option<f64>],
    ) -> Result<()> {
        let gas = self.gas;
        for (i, (&he, &p)) in self.he.iter().zip(p).enumerate() {
            let Some(t) = self.t.get_mut(i) else { continue };
            *t = gas.the(he, p, *t)?;
            let t = *t;
            if let (Some(psi), Some(mu), Some(alpha)) = (
                self.psi.get_mut(i),
                self.mu.get_mut(i),
                self.alpha.get_mut(i),
            ) {
                *psi = gas.psi(p, t);
                *mu = gas.mu();
                *alpha = gas.alphah();
            }
        }
        let mut k = 0;
        for (patch, kind) in mesh.patches().iter().zip(&self.kinds) {
            for _ in 0..patch.size {
                let (Some(t), Some(he), Some(&p)) =
                    (self.t_b.get_mut(k), self.he_b.get_mut(k), p_b.get(k))
                else {
                    k += 1;
                    continue;
                };
                if let (Some(tv), Some(hv), Some(p)) = (t.as_mut(), he.as_mut(), p) {
                    if *kind == TKind::Fixed {
                        *hv = gas.he(p, *tv);
                    } else {
                        *tv = gas.the(*hv, p, *tv)?;
                    }
                    let tv = *tv;
                    if let (Some(psi), Some(mu), Some(alpha)) = (
                        self.psi_b.get_mut(k),
                        self.mu_b.get_mut(k),
                        self.alpha_b.get_mut(k),
                    ) {
                        *psi = Some(gas.psi(p, tv));
                        *mu = Some(gas.mu());
                        *alpha = Some(gas.alphah());
                    }
                }
                k += 1;
            }
        }
        Ok(())
    }

    /// The energy's conditions as an equation sees them, after the
    /// `updateCoeffs` of `fixedEnergy` (the energy of the fixed
    /// temperature) and `gradientEnergy` (the gradient that keeps the
    /// temperature's). Re-evaluates a zero-gradient temperature from its
    /// cell first, as `Tw.evaluate()` does.
    pub(crate) fn energy_boundary(
        &mut self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        p_b: &[Option<f64>],
    ) -> Boundary {
        let gas = self.gas;
        let n_internal = mesh.n_internal_faces();
        let mut conditions = Vec::with_capacity(self.kinds.len());
        for (patch, kind) in mesh.patches().iter().zip(&self.kinds) {
            let mut values = Vec::with_capacity(patch.size);
            for f in patch.faces() {
                let k = f - n_internal;
                let cell = mesh.owner().get(f).and_then(|&c| self.t.get(c)).copied();
                let p = p_b.get(k).copied().flatten().unwrap_or(0.0);
                if *kind == TKind::ZeroGradient
                    && let Some(t) = self.t_b.get_mut(k)
                {
                    *t = cell;
                }
                let tw = self.t_b.get(k).copied().flatten().unwrap_or(0.0);
                match kind {
                    TKind::Fixed => {
                        let he = gas.he(p, tw);
                        if let Some(v) = self.he_b.get_mut(k) {
                            *v = Some(he);
                        }
                        values.push(he);
                    }
                    TKind::ZeroGradient => {
                        let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                        let sn_grad = dc * (tw - cell.unwrap_or(tw));
                        values.push(gas.cpv() * sn_grad + dc * (gas.he(p, tw) - gas.he(p, tw)));
                    }
                    TKind::Empty => {}
                }
            }
            conditions.push(match kind {
                TKind::Fixed => ScalarBc::FixedValue(values),
                TKind::ZeroGradient => ScalarBc::FixedGradient(values),
                TKind::Empty => ScalarBc::Empty,
            });
        }
        Boundary { conditions }
    }

    /// `T` as OpenFOAM writes it: its conditions as read, the zero
    /// gradients without values.
    pub(crate) fn to_write(&self) -> ScalarField {
        let mut out = self.file.clone();
        out.internal = Data::NonUniform(self.t.clone());
        out.drop_zero_gradient_values();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn air(energy: Energy) -> Gas {
        let rr = 1.0e3 * (N_A * K_B);
        Gas {
            energy,
            r: rr / 28.9,
            cp: 1005.0,
            mu: 1.82e-5,
            r_pr: 1.0 / 0.71,
        }
    }

    #[test]
    fn air_is_a_perfect_gas() {
        let gas = air(Energy::Internal);
        assert!((gas.r - 287.7).abs() < 0.1, "{}", gas.r);
        let rho = gas.rho(1.0e5, 300.0);
        assert!((rho - 1.1586).abs() < 1e-3, "{rho}");
        assert_eq!(gas.psi(1.0e5, 300.0) * 1.0e5, rho);
        assert!((gas.cp_by_cpv() - 1.4).abs() < 0.01);
        assert_eq!(air(Energy::Enthalpy).cp_by_cpv(), 1.0);
    }

    /// The Newton iteration recovers the temperature from either energy,
    /// from a start far off, and refuses a negative start.
    #[test]
    fn the_temperature_comes_back_from_the_energy() {
        for energy in [Energy::Internal, Energy::Enthalpy] {
            let gas = air(energy);
            let he = gas.he(1.2e5, 350.0);
            let t = gas.the(he, 1.2e5, 200.0).unwrap();
            assert!((t - 350.0).abs() < 1e-9, "{energy:?}: {t}");
        }
        assert!(air(Energy::Internal).the(1.0, 1.0e5, -1.0).is_err());
        assert!(air(Energy::Internal).the(f64::NAN, 1.0e5, 300.0).is_err());
    }
}
