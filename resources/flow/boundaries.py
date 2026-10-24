"""Ideal-gas pressure/temperature boundary translations for generated cases."""
import math

def bisect(f, lo, hi):
    if f(lo) * f(hi) > 0:
        raise ValueError("The dimensions do not admit the requested tangent geometry")
    for _ in range(80):
        mid = (lo + hi) / 2
        if f(lo) * f(mid) <= 0:
            hi = mid
        else:
            lo = mid
    return (lo + hi) / 2

def header(name, cls='dictionary'):
    return f'FoamFile {{ version 2.0; format ascii; class {cls}; object {name}; }}\n'

def reservoir_velocity(c):
    """Subsonic normal reservoir inflow using the outgoing Riemann invariant.

    Extrapolating the hot jet velocity into the 300 K reservoir can demand more
    kinetic energy than its total enthalpy. Compute the inflow speed from the
    interior outgoing characteristic and T0 instead. At choking use M=1;
    outflow extrapolates velocity. Native totalPressure and mixed total
    temperature provide the associated thermodynamic boundary values.
    See NASA AIAA 2018-0334, stagnation-properties subsonic inflow.
    This small generated OpenFOAM boundary requires its wmake toolchain.
    """
    gas_r=8314.462618/c['molecularWeight']
    gamma=c['Cp']/(c['Cp']-gas_r)
    return '''type codedFixedValue;
value uniform (0 0 0);
name reservoirCharacteristicVelocity;
code
#{
 const scalar gamma = @GAMMA@, gasR = @R@, totalT = @T0@;
 const auto& T = patch().lookupPatchField<volScalarField>("T");
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
#};'''.replace('@GAMMA@',repr(gamma)).replace('@R@',repr(gas_r)).replace('@T0@',repr(c['exteriorTotalTemperature']))

def pressure_outlet(c):
    # Fluent's pressure outlet extrapolates static pressure at supersonic
    # outflow. A fixedValue on the entire jet exit over-specifies that flow.
    gas_r=8314.462618/c['molecularWeight']
    gamma=c['Cp']/(c['Cp']-gas_r)
    pfar=c['operatingPressure']+c['exteriorGaugePressure']
    return '''type codedMixed;
name machAwarePressureOutlet;
refValue uniform @P@; refGradient uniform 0; valueFraction uniform 1; value uniform @P@;
code
#{
 const auto& u = patch().lookupPatchField<volVectorField>("U");
 const auto& t = patch().lookupPatchField<volScalarField>("T");
 const vectorField n(patch().nf());
 this->refValue() = @P@;
 this->refGrad() = 0;
 forAll(this->valueFraction(),i)
 {
  const scalar c = sqrt(@GAMMA@*@R@*max(t[i],scalar(1)));
  this->valueFraction()[i] = ((u[i]&n[i]) >= c) ? 0 : 1;
 }
#};'''.replace('@P@',repr(pfar)).replace('@GAMMA@',repr(gamma)).replace('@R@',repr(gas_r))

def viscosity_ratio_inlet(c, chi, initial):
    return '''type codedMixed;
name viscosityRatioNuTilda;
refValue uniform @INITIAL@; refGradient uniform 0; valueFraction uniform 1; value uniform @INITIAL@;
code
#{
 const auto& u = patch().lookupPatchField<volVectorField>("U");
 const auto& rho = patch().lookupPatchField<volScalarField>("rho");
 const vectorField n(patch().nf());
 this->refGrad() = 0;
 forAll(this->valueFraction(),i)
 {
  this->refValue()[i] = @CHI_MU@/max(rho[i],scalar(1e-12));
  this->valueFraction()[i] = ((u[i]&n[i]) <= 0) ? 1 : 0;
 }
#};'''.replace('@INITIAL@',repr(initial)).replace('@CHI_MU@',repr(chi*c['mu']))
