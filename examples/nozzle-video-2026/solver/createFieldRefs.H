volScalarField& p = thermo.p();
const volScalarField& T = thermo.T();
const volScalarField& psi = thermo.psi();

bool inviscid(true);
if (gMax(thermo.mu().cref().primitiveField()) > 0.0)
{
    inviscid = false;
}
