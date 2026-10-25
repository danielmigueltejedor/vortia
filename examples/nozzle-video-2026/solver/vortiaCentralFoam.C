/*---------------------------------------------------------------------------*\
  =========                 |
  \\      /  F ield         | OpenFOAM: The Open Source CFD Toolbox
   \\    /   O peration     |
    \\  /    A nd           | www.openfoam.com
     \\/     M anipulation  |
-------------------------------------------------------------------------------
    Copyright (C) 2011-2016 OpenFOAM Foundation
    Copyright (C) 2021-2025 OpenCFD Ltd.
-------------------------------------------------------------------------------
License
    This file is part of OpenFOAM.

    OpenFOAM is free software: you can redistribute it and/or modify it
    under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    OpenFOAM is distributed in the hope that it will be useful, but WITHOUT
    ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
    FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License
    for more details.

    You should have received a copy of the GNU General Public License
    along with OpenFOAM.  If not, see <http://www.gnu.org/licenses/>.

Application
    vortiaCentralFoam

Group
    grpCompressibleSolvers

Description
    Density-based compressible flow solver based on
    central-upwind schemes of Kurganov and Tadmor with
    support for mesh-motion and topology changes.

\*---------------------------------------------------------------------------*/

#include "fvCFD.H"
#include <cmath>
#include "dynamicFvMesh.H"
#include "psiThermo.H"
#include "turbulentFluidThermoModel.H"
#include "fixedRhoFvPatchScalarField.H"
#include "directionInterpolate.H"
#include "localEulerDdtScheme.H"
#include "fvcSmooth.H"
#include "ConvergenceMonitor.H"

// * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * //

int main(int argc, char *argv[])
{
    argList::addNote
    (
        "Density-based compressible flow solver based on"
        " central-upwind schemes of Kurganov and Tadmor with"
        " support for mesh-motion and topology changes."
    );

    #define NO_CONTROL
    #include "postProcess.H"

    #include "addCheckCaseOptions.H"
    #include "setRootCaseLists.H"
    #include "createTime.H"
    #include "createDynamicFvMesh.H"
    #include "createFields.H"
    #include "createFieldRefs.H"
    #include "createTimeControls.H"

    turbulence->validate();
    if (!LTS)
    {
        FatalErrorInFunction << "Vortia nozzle solver requires localEuler" << exit(FatalError);
    }
    const volScalarField minimumTemperature(IOobject("minimumTemperature", runTime.timeName(), mesh,
        IOobject::NO_READ, IOobject::NO_WRITE), mesh,
        dimensionedScalar("minimumTemperature", dimTemperature, 1e-6));
    const volScalarField minimumEnergy("minimumEnergy", thermo.he(thermo.p(),minimumTemperature));

    // * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * //

    #include "readFluxScheme.H"

    const dimensionedScalar v_zero(dimVolume/dimTime, Zero);

    // Courant numbers used to adjust the time-step
    scalar CoNum = 0.0;
    scalar meanCoNum = 0.0;
    scalar minCoNum = 0.0;

    Info<< "\nStarting time loop\n" << endl;

    bool stoppedOnConvergence=false;
    VortiaConvergenceMonitor convergence(runTime,mesh,p,T,rho,U);
    while (runTime.run())
    {
        #include "readTimeControls.H"

        if (!LTS)
        {
            #include "setDeltaT.H"

            ++runTime;

            // Do any mesh changes
            mesh.update();
        }

        // --- Directed interpolation of primitive fields onto faces

        auto rhoFaces = reconstructPair(rho, pos, neg);
        surfaceScalarField rho_pos(rhoFaces.first);
        surfaceScalarField rho_neg(rhoFaces.second);

        auto momentumFaces = reconstructPair(rhoU, pos, neg, U.name());
        surfaceVectorField rhoU_pos(momentumFaces.first);
        surfaceVectorField rhoU_neg(momentumFaces.second);

        volScalarField rPsi("rPsi", 1.0/psi);
        auto rPsiFaces = reconstructPair(rPsi, pos, neg, T.name());
        surfaceScalarField rPsi_pos(rPsiFaces.first);
        surfaceScalarField rPsi_neg(rPsiFaces.second);

        auto energyFaces = reconstructPair(e, pos, neg, T.name());
        surfaceScalarField e_pos(energyFaces.first);
        surfaceScalarField e_neg(energyFaces.second);

        surfaceVectorField U_pos("U_pos", rhoU_pos/rho_pos);
        surfaceVectorField U_neg("U_neg", rhoU_neg/rho_neg);

        surfaceScalarField p_pos("p_pos", rho_pos*rPsi_pos);
        surfaceScalarField p_neg("p_neg", rho_neg*rPsi_neg);

        surfaceScalarField phiv_pos("phiv_pos", U_pos & mesh.Sf());
        // Note: extracted out the orientation so becomes unoriented
        phiv_pos.setOriented(false);
        surfaceScalarField phiv_neg("phiv_neg", U_neg & mesh.Sf());
        phiv_neg.setOriented(false);

        // Make fluxes relative to mesh-motion
        if (mesh.moving())
        {
            surfaceScalarField meshPhi(mesh.phi());
            meshPhi.setOriented(false);
            phiv_pos -= meshPhi;
            phiv_neg -= meshPhi;
        }

        volScalarField c("c", sqrt(thermo.Cp()/thermo.Cv()*rPsi));
        auto soundFaces = reconstructPair(c, pos, neg, T.name());
        surfaceScalarField cSf_pos("cSf_pos", soundFaces.first*mesh.magSf());
        surfaceScalarField cSf_neg("cSf_neg", soundFaces.second*mesh.magSf());

        surfaceScalarField ap
        (
            "ap",
            max(max(phiv_pos + cSf_pos, phiv_neg + cSf_neg), v_zero)
        );

        surfaceScalarField am
        (
            "am",
            min(min(phiv_pos - cSf_pos, phiv_neg - cSf_neg), v_zero)
        );

        surfaceScalarField a_pos("a_pos", ap/(ap - am));

        surfaceScalarField amaxSf("amaxSf", max(mag(am), mag(ap)));

        surfaceScalarField aSf("aSf", am*a_pos);

        if (fluxScheme == "Tadmor")
        {
            aSf = -0.5*amaxSf;
            a_pos = 0.5;
        }

        surfaceScalarField a_neg("a_neg", 1.0 - a_pos);

        phiv_pos *= a_pos;
        phiv_neg *= a_neg;

        surfaceScalarField aphiv_pos("aphiv_pos", phiv_pos - aSf);
        surfaceScalarField aphiv_neg("aphiv_neg", phiv_neg + aSf);

        // Reuse amaxSf for the maximum positive and negative fluxes
        // estimated by the central scheme
        amaxSf = max(mag(aphiv_pos), mag(aphiv_neg));

        if (LTS)
        {
            #include "setRDeltaT.H"

            ++runTime;
        }

        #include "centralCourantNo.H"

        Info<< "Time = " << runTime.timeName() << nl << endl;

        phi = aphiv_pos*rho_pos + aphiv_neg*rho_neg;

        surfaceVectorField phiU(aphiv_pos*rhoU_pos + aphiv_neg*rhoU_neg);
        // Note: reassembled orientation from the pos and neg parts so becomes
        // oriented
        phiU.setOriented(true);

        surfaceVectorField phiUp(phiU + (a_pos*p_pos + a_neg*p_neg)*mesh.Sf());

        surfaceScalarField phiEp
        (
            "phiEp",
            aphiv_pos*(rho_pos*(e_pos + 0.5*magSqr(U_pos)) + p_pos)
          + aphiv_neg*(rho_neg*(e_neg + 0.5*magSqr(U_neg)) + p_neg)
          + aSf*p_pos - aSf*p_neg
        );

        // Make flux for pressure-work absolute
        if (mesh.moving())
        {
            surfaceScalarField meshPhi(mesh.phi());
            meshPhi.setOriented(false);
            phiEp += meshPhi*(a_pos*p_pos + a_neg*p_neg);
        }

        volScalarField muEff("muEff", turbulence->muEff());
        volTensorField tauMC("tauMC", muEff*dev2(Foam::T(fvc::grad(U))));

        // Vortia: reject non-physical candidate updates instead of clipping
        // their temperature. Sensible energy may be negative because its
        // thermodynamic reference is arbitrary. Compare with he(p,T=0).
        const volScalarField rhoStepStart("rhoStepStart", rho);
        const volVectorField rhoUStepStart("rhoUStepStart", rhoU);
        const volScalarField rhoEStepStart("rhoEStepStart", rhoE);
        const volVectorField UStepStart("UStepStart", U);
        const volScalarField eStepStart("eStepStart", e);
        const volScalarField pStepStart("pStepStart", p);
        const volScalarField TStepStart("TStepStart", T);
        auto energyIsPhysical = [&]() {
            bool valid = true;
            forAll(e.primitiveField(),i)
                if (!std::isfinite(e[i]) || e[i] <= minimumEnergy[i]) valid = false;
            forAll(e.boundaryField(), patchi)
            {
                forAll(e.boundaryField()[patchi],i)
                    if (!std::isfinite(e.boundaryField()[patchi][i]) ||
                        e.boundaryField()[patchi][i] <= minimumEnergy.boundaryField()[patchi][i]) valid = false;
            }
            // Every rank must accept/retry together, including empty patches.
            return returnReduce(valid, andOp<bool>());
        };
        auto temperatureIsPhysical = [&]() {
            bool valid = true;
            forAll(T.primitiveField(),i)
                if (!(T[i] > 0) || !std::isfinite(T[i])) valid = false;
            forAll(T.boundaryField(), patchi)
            {
                forAll(T.boundaryField()[patchi],i)
                    if (!(T.boundaryField()[patchi][i] > 0) || !std::isfinite(T.boundaryField()[patchi][i])) valid = false;
            }
            return returnReduce(valid, andOp<bool>());
        };
        auto rejectCandidate = [&]() {
            Info<< "Vortia: rejected non-positive candidate; halving local steps" << endl;
            trDeltaT.ref().primitiveFieldRef() *= 2.0;
            trDeltaT.ref().correctBoundaryConditions();
        };
        for (label attempt=0; ; ++attempt)
        {
            if (attempt > 30)
            {
                FatalErrorInFunction << "No positive conservative update after 30 reductions" << exit(FatalError);
            }
            if (attempt)
            {
                rho = rhoStepStart; rhoU = rhoUStepStart; rhoE = rhoEStepStart;
                U = UStepStart; e = eStepStart; p = pStepStart;
                thermo.T() = TStepStart;
                thermo.correct();
            }

        // --- Solve density
        solve(fvm::ddt(rho) + fvc::div(phi));

        if (gMin(rho.primitiveField()) <= 1e-12)
        { rejectCandidate(); continue; }

        // --- Solve momentum
        solve(fvm::ddt(rhoU) + fvc::div(phiUp));

        U.ref() =
            rhoU()
           /rho();
        U.primitiveFieldRef().replace(vector::Z, scalar(0));
        rhoU.primitiveFieldRef() = rho.primitiveField()*U.primitiveField();
        U.correctBoundaryConditions();
        rhoU.boundaryFieldRef() == rho.boundaryField()*U.boundaryField();

        if (!inviscid)
        {
            solve
            (
                fvm::ddt(rho, U) - fvc::ddt(rho, U)
              - fvm::laplacian(muEff, U)
              - fvc::div(tauMC)
            );
            U.primitiveFieldRef().replace(vector::Z, scalar(0));
            U.correctBoundaryConditions();
            rhoU = rho*U;
        }

        // --- Solve energy
        surfaceScalarField sigmaDotU
        (
            "sigmaDotU",
            (
                fvc::interpolate(muEff)*mesh.magSf()*fvc::snGrad(U)
              + fvc::dotInterpolate(mesh.Sf(), tauMC)
            )
          & (a_pos*U_pos + a_neg*U_neg)
        );

        solve
        (
            fvm::ddt(rhoE)
          + fvc::div(phiEp)
          - fvc::div(sigmaDotU)
        );

        e = rhoE/rho - 0.5*magSqr(U);
        e.correctBoundaryConditions();
        if (!energyIsPhysical())
        { Info<< "Vortia rejected explicit energy" << endl; rejectCandidate(); continue; }
        thermo.correct();
        if (!temperatureIsPhysical()) { rejectCandidate(); continue; }
        rhoE.boundaryFieldRef() ==
            rho.boundaryField()*
            (
                e.boundaryField() + 0.5*magSqr(U.boundaryField())
            );

        if (!inviscid)
        {
            solve
            (
                fvm::ddt(rho, e) - fvc::ddt(rho, e)
              - fvm::laplacian(turbulence->alphaEff(), e)
            );
            if (!energyIsPhysical())
            { Info<< "Vortia rejected diffused energy" << endl; rejectCandidate(); continue; }
            thermo.correct();
            if (!temperatureIsPhysical()) { rejectCandidate(); continue; }
            rhoE = rho*(e + 0.5*magSqr(U));
        }

        p.ref() =
            rho()
           /psi();
        p.correctBoundaryConditions();
        rho.boundaryFieldRef() == psi.boundaryField()*p.boundaryField();

        turbulence->correct();
        break;
        }

        if (convergence.check(phi,pStepStart,TStepStart,rhoStepStart,UStepStart,trDeltaT(),c)) {
            stoppedOnConvergence=true;
            runTime.writeNow();
            runTime.printExecutionTime(Info);
            break;
        }
        runTime.write();

        runTime.printExecutionTime(Info);
    }

    // Retain the actual final state, including an endTime off the output interval.
    if(convergence.advanced()&&!stoppedOnConvergence)runTime.writeNow();
    convergence.finish();
    Info<< "End\n" << endl;

    return 0;
}

// ************************************************************************* //
