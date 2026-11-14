//! The app's own programmed boundary conditions: `codedFixedValue` and
//! `codedMixed` entries whose `code` is C++ that OpenFOAM compiles when the
//! case runs. The engine compiles nothing: it knows the three the nozzle case
//! writes (`prepare.py` of the nozzle example) by their `name`, takes their
//! constants from the `code`, and refuses a `code` that differs from the one
//! it knows, so an edited condition never runs as something else.
//!
//! The formulas are those of the code, evaluated where OpenFOAM would
//! evaluate them.

use vortia_core::Vec3;
use vortia_fields::{ScalarField, VectorField};
use vortia_foamfile::Dictionary;
use vortia_fv::{Boundary, FvGeometry, ScalarBc, VectorBc, VectorBoundary};
use vortia_mesh::PolyMesh;

use crate::error::{Result, SolverError};

/// `reservoirCharacteristicVelocity` on U.
const RESERVOIR: &str = "
 const scalar gamma = @GAMMA@, gasR = @R@, totalT = @T0@;
 const auto& T = patch().lookupPatchField<volScalarField>(\"T\");
 const scalarField ti(T.patchInternalField());
 const vectorField ui(this->patchInternalField());
 const vectorField n(patch().nf());
 vectorField ub(ui);
 const scalar cstar = sqrt(2*gamma*gasR*totalT/(gamma+1));
 forAll(ub,i) {
  const scalar un = ui[i]&n[i];
  if (un <= 0) {
   const scalar ri = un+2*sqrt(gamma*gasR*max(ti[i],scalar(1)))/(gamma-1);
   const scalar disc = -2*(gamma-1)*sqr(ri)+4*(gamma+1)*gamma*gasR*totalT/(gamma-1);
   const scalar q = min(cstar,max(scalar(0),(-(gamma-1)*ri+sqrt(max(disc,scalar(0))))/(gamma+1)));
   ub[i] = -n[i]*q;
  }
 }
 operator==(ub);
";

/// `machAwarePressureOutlet` on p.
const PRESSURE_OUTLET: &str = "
 const auto& u = patch().lookupPatchField<volVectorField>(\"U\");
 const auto& t = patch().lookupPatchField<volScalarField>(\"T\");
 const vectorField n(patch().nf());
 this->refValue() = @P@;
 this->refGrad() = 0;
 forAll(this->valueFraction(),i)
 {
  const scalar c = sqrt(@GAMMA@*@R@*max(t[i],scalar(1)));
  this->valueFraction()[i] = ((u[i]&n[i]) >= c) ? 0 : 1;
 }
";

/// `viscosityRatioNuTilda` on nuTilda.
const VISCOSITY_RATIO: &str = "
 const auto& u = patch().lookupPatchField<volVectorField>(\"U\");
 const auto& rho = patch().lookupPatchField<volScalarField>(\"rho\");
 const vectorField n(patch().nf());
 this->refGrad() = 0;
 forAll(this->valueFraction(),i)
 {
  this->refValue()[i] = @CHI_MU@/max(rho[i],scalar(1e-12));
  this->valueFraction()[i] = ((u[i]&n[i]) <= 0) ? 1 : 0;
 }
";

/// The numbers `code` holds in place of the `@NAME@` of `template`, in
/// order of first appearance, or `None` where it differs in anything else
/// (runs of white space count as one space).
fn numbers(template: &str, code: &str) -> Option<Vec<(String, f64)>> {
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let (template, code) = (squash(template), squash(code));
    let mut found: Vec<(String, f64)> = Vec::new();
    let mut rest = code.as_str();
    let mut parts = template.split('@');
    rest = rest.strip_prefix(parts.next()?)?;
    // Alternating: a name, then the literal text that follows it.
    while let Some(name) = parts.next() {
        let literal = parts.next()?;
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')))
            .unwrap_or(rest.len());
        let value: f64 = rest
            .get(..end)?
            .parse()
            .ok()
            .filter(|v: &f64| v.is_finite())?;
        match found.iter().find(|(n, _)| n == name) {
            Some((_, v)) if *v != value => return None,
            Some(_) => {}
            None => found.push((name.to_string(), value)),
        }
        rest = rest.get(end..)?.strip_prefix(literal)?;
    }
    rest.is_empty().then_some(found)
}

/// `gamma`, the gas constant and the total temperature of the reservoir.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reservoir {
    gamma: f64,
    gas_r: f64,
    total_t: f64,
}

/// The outlet whose pressure is fixed unless the flow is supersonic.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PressureOutlet {
    p_far: f64,
    gamma: f64,
    gas_r: f64,
}

/// What one programmed condition is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Program {
    Reservoir(Reservoir),
    PressureOutlet(PressureOutlet),
    /// `ν̃ = chi μ / ρ` where the flow comes in; the product `chi μ`.
    ViscosityRatio(f64),
}

/// The program a `codedFixedValue` or `codedMixed` entry holds, or an error
/// saying what is wrong with it.
pub(crate) fn read(settings: &Dictionary, what: &str) -> Result<Program> {
    let name = settings
        .word("name")
        .ok_or_else(|| SolverError::Setup(format!("{what} has no 'name'")))?;
    let code = settings
        .value("code")
        .and_then(|v| v.as_code())
        .ok_or_else(|| SolverError::Setup(format!("{what} has no 'code' block")))?;
    let known = |template: &str| numbers(template, code);
    let get =
        |found: &[(String, f64)], key: &str| found.iter().find(|(n, _)| n == key).map(|(_, v)| *v);
    let program = match name {
        "reservoirCharacteristicVelocity" => known(RESERVOIR).and_then(|f| {
            Some(Program::Reservoir(Reservoir {
                gamma: get(&f, "GAMMA")?,
                gas_r: get(&f, "R")?,
                total_t: get(&f, "T0")?,
            }))
        }),
        "machAwarePressureOutlet" => known(PRESSURE_OUTLET).and_then(|f| {
            Some(Program::PressureOutlet(PressureOutlet {
                p_far: get(&f, "P")?,
                gamma: get(&f, "GAMMA")?,
                gas_r: get(&f, "R")?,
            }))
        }),
        "viscosityRatioNuTilda" => {
            known(VISCOSITY_RATIO).and_then(|f| Some(Program::ViscosityRatio(get(&f, "CHI_MU")?)))
        }
        other => {
            return Err(SolverError::Setup(format!(
                "{what}: the programmed condition '{other}' is not one the engine knows"
            )));
        }
    };
    let program = program.ok_or_else(|| {
        SolverError::Setup(format!(
            "{what}: the code of '{name}' differs from the one the engine runs; \
             regenerate the case"
        ))
    })?;
    let valid = match &program {
        Program::Reservoir(r) => r.gamma > 1.0 && r.gas_r > 0.0 && r.total_t > 0.0,
        Program::PressureOutlet(o) => o.gamma > 1.0 && o.gas_r > 0.0 && o.p_far > 0.0,
        Program::ViscosityRatio(m) => *m >= 0.0,
    };
    if valid {
        Ok(program)
    } else {
        Err(SolverError::Setup(format!(
            "{what}: the constants of '{name}' are out of range"
        )))
    }
}

/// `min(hi, max(lo, x))` of the code, which gives `lo` for a NaN.
#[allow(clippy::manual_clamp)]
fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    x.max(lo).min(hi)
}

/// The unit normal of a boundary face (zero on a degenerate one).
pub(crate) fn unit_normal(fv: &FvGeometry, f: usize) -> Vec3 {
    let sf = fv.geo.face_areas.get(f).copied().unwrap_or_default();
    let area = fv.mag_sf.get(f).copied().unwrap_or(0.0);
    if area > 0.0 { sf * (1.0 / area) } else { sf }
}

/// The programmed conditions of the pressure and the velocity.
#[derive(Default)]
pub(crate) struct Programmed {
    /// Patch index and program of each `machAwarePressureOutlet`.
    outlets: Vec<(usize, PressureOutlet)>,
    /// Patch index and program of each `reservoirCharacteristicVelocity`.
    reservoirs: Vec<(usize, Reservoir)>,
}

impl Programmed {
    pub(crate) fn read(p: &ScalarField, u: &VectorField, mesh: &PolyMesh) -> Result<Self> {
        let mut out = Self::default();
        for (i, patch) in mesh.patches().iter().enumerate() {
            if let Some(pf) = p.patch_field(patch)
                && pf.kind == "codedMixed"
            {
                match read(&pf.settings, &format!("p: codedMixed on '{}'", patch.name))? {
                    Program::PressureOutlet(o) => out.outlets.push((i, o)),
                    _ => {
                        return Err(SolverError::Setup(format!(
                            "p: '{}' is not a condition the pressure takes",
                            patch.name
                        )));
                    }
                }
            }
            if let Some(pf) = u.patch_field(patch)
                && pf.kind == "codedFixedValue"
            {
                match read(
                    &pf.settings,
                    &format!("U: codedFixedValue on '{}'", patch.name),
                )? {
                    Program::Reservoir(r) => out.reservoirs.push((i, r)),
                    _ => {
                        return Err(SolverError::Setup(format!(
                            "U: '{}' is not a condition the velocity takes",
                            patch.name
                        )));
                    }
                }
            }
        }
        Ok(out)
    }

    /// `p.correctBoundaryConditions()` of the outlets: the pressure outside,
    /// from the velocity and the temperature on the boundary, unless the flow
    /// leaves at the speed of sound or faster.
    pub(crate) fn update_pressure(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &mut Boundary,
        u_b: &[Option<Vec3>],
        t_b: &[Option<f64>],
    ) {
        let n_internal = mesh.n_internal_faces();
        for (index, program) in &self.outlets {
            let (
                Some(patch),
                Some(ScalarBc::Mixed {
                    fraction,
                    ref_value,
                    ..
                }),
            ) = (mesh.patches().get(*index), bc.conditions.get_mut(*index))
            else {
                continue;
            };
            for (i, f) in patch.faces().enumerate() {
                let k = f.saturating_sub(n_internal);
                let u = u_b.get(k).copied().flatten().unwrap_or_default();
                let t = t_b.get(k).copied().flatten().unwrap_or(0.0);
                let c = (program.gamma * program.gas_r * t.max(1.0)).sqrt();
                if let Some(w) = fraction.get_mut(i) {
                    *w = if u.dot(unit_normal(fv, f)) >= c {
                        0.0
                    } else {
                        1.0
                    };
                }
                if let Some(r) = ref_value.get_mut(i) {
                    *r = program.p_far;
                }
            }
        }
    }

    /// `U.correctBoundaryConditions()` of the reservoir faces: the outgoing
    /// Riemann invariant of the cell next to the face, and the speed that
    /// keeps the total temperature of the reservoir, where the flow comes in.
    pub(crate) fn update_velocity(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &mut VectorBoundary,
        (u, t): (&[Vec3], &[f64]),
    ) {
        for (index, r) in &self.reservoirs {
            let (Some(patch), Some(VectorBc::FixedValue(values))) =
                (mesh.patches().get(*index), bc.conditions.get_mut(*index))
            else {
                continue;
            };
            let (g, gas_r) = (r.gamma, r.gas_r);
            let cstar = (2.0 * g * gas_r * r.total_t / (g + 1.0)).sqrt();
            for (i, f) in patch.faces().enumerate() {
                let Some(cell) = mesh.owner().get(f).copied() else {
                    continue;
                };
                let ui = u.get(cell).copied().unwrap_or_default();
                let ti = t.get(cell).copied().unwrap_or(1.0);
                let n = unit_normal(fv, f);
                let un = ui.dot(n);
                let value = if un <= 0.0 {
                    let ri = un + 2.0 * (g * gas_r * ti.max(1.0)).sqrt() / (g - 1.0);
                    let disc = -2.0 * (g - 1.0) * ri * ri
                        + 4.0 * (g + 1.0) * g * gas_r * r.total_t / (g - 1.0);
                    let q = (-(g - 1.0) * ri + disc.max(0.0).sqrt()) / (g + 1.0);
                    n * -clamp(q, 0.0, cstar)
                } else {
                    ui
                };
                if let Some(v) = values.get_mut(i) {
                    *v = value;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(name: &str, code: &str) -> Dictionary {
        let src = format!("name {name}; code\n#{{\n{code}\n#}};");
        Dictionary::parse(
            &mut vortia_foamfile::Tokenizer::new(src.as_bytes()),
            false,
            0,
        )
        .unwrap()
    }

    fn filled(template: &str, pairs: &[(&str, &str)]) -> String {
        pairs.iter().fold(template.to_string(), |t, (k, v)| {
            t.replace(&format!("@{k}@"), v)
        })
    }

    #[test]
    fn the_three_programs_are_read_with_their_constants() {
        let code = filled(
            RESERVOIR,
            &[("GAMMA", "1.3"), ("R", "346.5"), ("T0", "300.0")],
        );
        let program = read(&settings("reservoirCharacteristicVelocity", &code), "U").unwrap();
        assert_eq!(
            program,
            Program::Reservoir(Reservoir {
                gamma: 1.3,
                gas_r: 346.5,
                total_t: 300.0
            })
        );
        let code = filled(
            PRESSURE_OUTLET,
            &[("P", "30000.0"), ("GAMMA", "1.3"), ("R", "346.5")],
        );
        let program = read(&settings("machAwarePressureOutlet", &code), "p").unwrap();
        assert_eq!(
            program,
            Program::PressureOutlet(PressureOutlet {
                p_far: 30000.0,
                gamma: 1.3,
                gas_r: 346.5
            })
        );
        let code = filled(VISCOSITY_RATIO, &[("CHI_MU", "0.0002155")]);
        let program = read(&settings("viscosityRatioNuTilda", &code), "nuTilda").unwrap();
        assert_eq!(program, Program::ViscosityRatio(0.0002155));
    }

    #[test]
    fn a_program_that_differs_or_is_unknown_is_refused() {
        // Another expression, a constant that is not a number, an unknown
        // name and a pressure that disagrees with itself.
        let edited = filled(
            PRESSURE_OUTLET,
            &[("P", "30000.0"), ("GAMMA", "1.3"), ("R", "346.5")],
        )
        .replace(">= c", "> c");
        let e = read(&settings("machAwarePressureOutlet", &edited), "p").unwrap_err();
        assert!(e.to_string().contains("differs"), "{e}");
        let code = filled(VISCOSITY_RATIO, &[("CHI_MU", "abc")]);
        assert!(read(&settings("viscosityRatioNuTilda", &code), "n").is_err());
        let e = read(&settings("somethingElse", "x"), "p").unwrap_err();
        assert!(e.to_string().contains("somethingElse"), "{e}");
        let code = filled(
            RESERVOIR,
            &[("GAMMA", "0.9"), ("R", "346.5"), ("T0", "300.0")],
        );
        let e = read(&settings("reservoirCharacteristicVelocity", &code), "U").unwrap_err();
        assert!(e.to_string().contains("out of range"), "{e}");
    }

    fn one_cell() -> (PolyMesh, FvGeometry) {
        let bounds = vortia_core::Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let mesh = vortia_mesh::blocks::structured_box(bounds, [1, 1, 1]).unwrap();
        let fv = FvGeometry::new(&mesh);
        (mesh, fv)
    }

    /// A cell at rest at the reservoir's total temperature gives the face no
    /// velocity: the Riemann invariant and the energy balance meet at zero.
    /// Flow leaving the domain is left as the cell has it, and the speed that
    /// comes in never passes the critical speed.
    #[test]
    fn the_reservoir_velocity_follows_the_characteristics() {
        let (mesh, fv) = one_cell();
        let (gamma, gas_r, total_t) = (1.3, 346.0, 300.0);
        let program = Programmed {
            outlets: Vec::new(),
            reservoirs: vec![(
                0,
                Reservoir {
                    gamma,
                    gas_r,
                    total_t,
                },
            )],
        };
        let mut bc = VectorBoundary {
            conditions: vec![VectorBc::FixedValue(vec![Vec3::default()]); 6],
            assignable: vec![false; 6],
        };
        let value = |bc: &VectorBoundary| match &bc.conditions[0] {
            VectorBc::FixedValue(v) => v[0],
            other => panic!("{other:?}"),
        };
        // Patch 0 is xMin: its outward normal is -x.
        program.update_velocity(&mesh, &fv, &mut bc, (&[Vec3::default()], &[total_t]));
        assert!(value(&bc).norm() < 1e-9, "{:?}", value(&bc));
        // Leaving: the cell's own velocity.
        let leaving = Vec3::new(-40.0, 3.0, 0.0);
        program.update_velocity(&mesh, &fv, &mut bc, (&[leaving], &[total_t]));
        assert_eq!(value(&bc), leaving);
        // Coming in fast from a cold cell: along +x (against the normal), and
        // never above the critical speed.
        let cstar = (2.0 * gamma * gas_r * total_t / (gamma + 1.0)).sqrt();
        for (ux, t) in [(30.0, 250.0), (200.0, 150.0), (600.0, 100.0)] {
            program.update_velocity(&mesh, &fv, &mut bc, (&[Vec3::new(ux, 7.0, 0.0)], &[t]));
            let v = value(&bc);
            assert!(
                v.x >= 0.0 && v.y == 0.0 && v.x <= cstar + 1e-9,
                "{v:?} {cstar}"
            );
        }
        // A middle value, checked against the formulas on their own.
        program.update_velocity(
            &mesh,
            &fv,
            &mut bc,
            (&[Vec3::new(50.0, 0.0, 0.0)], &[280.0]),
        );
        let ri = -50.0 + 2.0 * (gamma * gas_r * 280.0_f64).sqrt() / (gamma - 1.0);
        let disc = -2.0 * (gamma - 1.0) * ri * ri
            + 4.0 * (gamma + 1.0) * gamma * gas_r * total_t / (gamma - 1.0);
        let q = (-(gamma - 1.0) * ri + disc.sqrt()) / (gamma + 1.0);
        assert!(
            (value(&bc).x - q).abs() < 1e-9 * q.abs().max(1.0),
            "{} {q}",
            value(&bc).x
        );
    }

    /// The outlet takes its pressure from outside unless the flow leaves at
    /// the speed of sound or faster.
    #[test]
    fn the_pressure_outlet_lets_go_of_the_pressure_when_the_flow_is_sonic() {
        let (mesh, fv) = one_cell();
        let program = Programmed {
            outlets: vec![(
                1,
                PressureOutlet {
                    p_far: 30000.0,
                    gamma: 1.3,
                    gas_r: 346.0,
                },
            )],
            reservoirs: Vec::new(),
        };
        let mut bc = Boundary {
            conditions: vec![ScalarBc::ZeroGradient; 6],
        };
        bc.conditions[1] = ScalarBc::Mixed {
            fraction: vec![1.0],
            ref_value: vec![0.0],
            ref_gradient: Vec::new(),
        };
        let c = (1.3_f64 * 346.0 * 300.0).sqrt();
        // A box of one cell has no internal faces, so the boundary faces
        // are in patch order and xMax, with its normal along +x, is the second.
        let mut u_b = vec![Some(Vec3::default()); 6];
        let mut t_b = vec![Some(300.0); 6];
        for (speed, want) in [(0.5 * c, 1.0), (1.01 * c, 0.0), (c, 0.0)] {
            u_b[1] = Some(Vec3::new(speed, 0.0, 0.0));
            program.update_pressure(&mesh, &fv, &mut bc, &u_b, &t_b);
            let ScalarBc::Mixed {
                fraction,
                ref_value,
                ..
            } = &bc.conditions[1]
            else {
                panic!("not mixed");
            };
            assert_eq!((fraction[0], ref_value[0]), (want, 30000.0), "{speed}");
        }
        // The temperature is floored at 1 K, as in the code.
        t_b[1] = Some(-5.0);
        u_b[1] = Some(Vec3::new(30.0, 0.0, 0.0));
        program.update_pressure(&mesh, &fv, &mut bc, &u_b, &t_b);
        assert_eq!(
            bc.conditions[1],
            ScalarBc::Mixed {
                fraction: vec![0.0],
                ref_value: vec![30000.0],
                ref_gradient: Vec::new(),
            }
        );
    }

    /// The nozzle example writes the code itself; what it writes is what the
    /// templates match, whatever the white space.
    #[test]
    fn the_code_the_nozzle_example_writes_is_the_code_the_engine_knows() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/nozzle-video-2026/prepare.py");
        let text = std::fs::read_to_string(source).unwrap();
        let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = squash(&text);
        for template in [RESERVOIR, PRESSURE_OUTLET, VISCOSITY_RATIO] {
            // Each template's body, as the Python source holds it: with the
            // quotes it escapes and the placeholders it replaces.
            let body = squash(template);
            assert!(text.contains(&body), "prepare.py no longer holds: {body}");
        }
    }
}
