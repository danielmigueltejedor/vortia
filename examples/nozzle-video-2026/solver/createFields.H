#include "createRDeltaT.H"

Info<< "Reading thermophysical properties\n" << endl;

autoPtr<psiThermo> pThermo
(
    psiThermo::New(mesh)
);
psiThermo& thermo = pThermo();

volScalarField& e = thermo.he();

Info<< "Reading field U\n" << endl;
volVectorField U
(
    IOobject
    (
        "U",
        runTime.timeName(),
        mesh,
        IOobject::MUST_READ,
        IOobject::AUTO_WRITE
    ),
    mesh
);

// The tutorial selects axisymmetric flow WITHOUT swirl. OpenFOAM wedge
// geometry still permits three solution components (including swirl).
// Reject incompatible restarts rather than silently reusing their energy.
const auto directions = mesh.geometricD();
if (directions.x() != 1 || directions.y() != 1 || directions.z() != -1
    || !runTime.controlDict().getOrDefault<Switch>("noSwirl", false))
{
    FatalErrorInFunction << "This nozzle backend requires an x-axis wedge in the xy mid-plane and noSwirl true"
        << exit(FatalError);
}
if (gMax(mag(U.primitiveField().component(vector::Z))) > 1e-8)
{
    FatalErrorInFunction << "The restart contains azimuthal velocity; initialize a fresh no-swirl case"
        << exit(FatalError);
}
U.primitiveFieldRef().replace(vector::Z, scalar(0));
Info<< "Axisymmetric no-swirl constraint: mid-plane Uz = 0" << endl;

volScalarField rho
(
    IOobject
    (
        "rho",
        runTime.timeName(),
        mesh,
        IOobject::NO_READ,
        IOobject::AUTO_WRITE
    ),
    thermo.rho()
);

volVectorField rhoU
(
    IOobject
    (
        "rhoU",
        runTime.timeName(),
        mesh,
        IOobject::NO_READ,
        IOobject::NO_WRITE
    ),
    rho*U
);

volScalarField rhoE
(
    IOobject
    (
        "rhoE",
        runTime.timeName(),
        mesh,
        IOobject::NO_READ,
        IOobject::NO_WRITE
    ),
    rho*(e + 0.5*magSqr(U))
);

surfaceScalarField pos
(
    IOobject
    (
        "pos",
        runTime.timeName(),
        mesh
    ),
    mesh,
    dimensionedScalar("pos", dimless, 1.0)
);

surfaceScalarField neg
(
    IOobject
    (
        "neg",
        runTime.timeName(),
        mesh
    ),
    mesh,
    dimensionedScalar("neg", dimless, -1.0)
);

surfaceScalarField phi("phi", fvc::flux(rhoU));

Info<< "Creating turbulence model\n" << endl;
autoPtr<compressible::turbulenceModel> turbulence
(
    compressible::turbulenceModel::New
    (
        rho,
        U,
        phi,
        thermo
    )
);
