{
    volScalarField& rDeltaT = trDeltaT.ref();
    const scalarField previousRDeltaT(rDeltaT.primitiveField());

    scalar rDeltaTSmoothingCoeff
    (
        runTime.controlDict().getOrDefault<scalar>
        (
            "rDeltaTSmoothingCoeff",
            0.02
        )
    );

    // Set the reciprocal time-step from the local Courant number
    rDeltaT.ref() = max
    (
        1/dimensionedScalar("maxDeltaT", dimTime, maxDeltaT),
        fvc::surfaceSum(amaxSf)()()
       /((2*maxCo)*mesh.V())
    );

    // Vortia: also bound explicit viscous work and thermal diffusion.
    // The acoustic Courant restriction alone does not bound these terms.
    if (!inviscid)
    {
        const scalar maxDiffusionCo = runTime.controlDict().getOrDefault<scalar>("maxDiffusionCo", 0.1);
        if (!(maxDiffusionCo > 0) || !std::isfinite(maxDiffusionCo))
        {
            FatalErrorInFunction << "maxDiffusionCo must be positive and finite" << exit(FatalError);
        }
        const volScalarField diffusivity
        (
            "timeStepDiffusivity",
            max((4.0/3.0)*turbulence->muEff()/rho,
                (thermo.Cp()/thermo.Cv())*turbulence->alphaEff()/rho)
        );
        const scalarField diffusionRate
        (
            fvc::surfaceSum(fvc::interpolate(diffusivity)*mesh.magSf()*mesh.deltaCoeffs())()().field()/mesh.V()
        );
        rDeltaT.primitiveFieldRef() = max(rDeltaT.primitiveField(), diffusionRate/maxDiffusionCo);
    }

    // Update tho boundary values of the reciprocal time-step
    rDeltaT.correctBoundaryConditions();

    fvc::smooth(rDeltaT, rDeltaTSmoothingCoeff);
    if (runTime.timeIndex() > runTime.startTimeIndex()+1)
    {
        rDeltaT.primitiveFieldRef() = max(rDeltaT.primitiveField(), 0.9*previousRDeltaT);
        rDeltaT.correctBoundaryConditions();
    }

    if(logThisIteration)
    {
        auto limits = gMinMax(rDeltaT.primitiveField());
        limits.reset(1/(limits.max()+VSMALL), 1/(limits.min()+VSMALL));

        Info<< "Flow time scale min/max = "
            << limits.min() << ", " << limits.max() << endl;
    }
}
