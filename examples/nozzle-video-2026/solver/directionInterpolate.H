// Paired reconstruction additions are derived from OpenFOAM v2606:
// Copyright (C) 2011-2016 OpenFOAM Foundation
// Copyright (C) 2016-2025 OpenCFD Ltd.
// This file is free software under GNU GPL version 3 or later. It is supplied
// without warranty; see LICENSE.md for the full license and original notices.
#include "Minmod.H"
#include "NVDTVD.H"
#include "LimitFuncs.H"
#include <utility>

namespace Foam
{

//- Interpolate field vf according to direction dir
template<class Type>
tmp<GeometricField<Type, fvsPatchField, surfaceMesh>> interpolate
(
    const GeometricField<Type, fvPatchField, volMesh>& vf,
    const surfaceScalarField& dir,
    const word& reconFieldName = word::null
)
{
    tmp<GeometricField<Type, fvsPatchField, surfaceMesh>> tsf
    (
        fvc::interpolate
        (
            vf,
            dir,
            "reconstruct("
          + (reconFieldName.empty() ? vf.name() : reconFieldName)
          + ')'
        )
    );

    GeometricField<Type, fvsPatchField, surfaceMesh>& sf = tsf.ref();

    sf.rename(vf.name() + '_' + dir.name());

    return tsf;
}

// Paired form of OpenFOAM v2606 LimitedScheme/Minmod reconstruction.
// The two directions use the same limiting field and its gradient. Keep the
// stock limiter and arithmetic, but calculate these shared quantities once.
// Derived from LimitedScheme.C and surfaceInterpolationScheme.C, copyright
// OpenFOAM Foundation / OpenCFD Ltd.; GPL-3.0-or-later (see LICENSE.md).
template<class Type>
auto reconstructPair
(
    const GeometricField<Type, fvPatchField, volMesh>& vf,
    const surfaceScalarField& pos,
    const surfaceScalarField& neg,
    const word& reconFieldName = word::null
)
{
    using Surface = GeometricField<Type, fvsPatchField, surfaceMesh>;
    using Pair = std::pair<tmp<Surface>, tmp<Surface>>;
    const fvMesh& mesh = vf.mesh();
    auto& scheme = mesh.interpolationScheme
    (
        "reconstruct(" + (reconFieldName.empty() ? vf.name() : reconFieldName) + ')'
    );
    if (!mesh.time().controlDict().getOrDefault<Switch>("pairedReconstruction", true)
        || scheme.size() != 1 || !scheme[0].isWord() || scheme[0].wordToken() != "Minmod")
    {
        return Pair(interpolate(vf, pos, reconFieldName), interpolate(vf, neg, reconFieldName));
    }

    auto limited = limitFuncs::magSqr<Type>()(vf);
    const volScalarField& lPhi = limited();
    auto gradient = fvc::grad(lPhi);
    const volVectorField& grad = gradient();
    const auto& weights = mesh.surfaceInterpolation::weights();
    const auto& owner = mesh.owner();
    const auto& neighbour = mesh.neighbour();
    const vectorField& centres = mesh.C();
    IStringStream limiterData("");
    const MinmodLimiter<NVDTVD> limiter(limiterData);
    auto positive = Surface::New(vf.name()+"_pos", mesh, vf.dimensions());
    auto negative = Surface::New(vf.name()+"_neg", mesh, vf.dimensions());
    auto& p = positive.ref().primitiveFieldRef();
    auto& n = negative.ref().primitiveFieldRef();
    forAll(p, face)
    {
        const label own = owner[face], nei = neighbour[face];
        const vector d = centres[nei] - centres[own];
        const scalar lp = limiter.limiter(weights[face], 1, lPhi[own], lPhi[nei], grad[own], grad[nei], d);
        const scalar ln = limiter.limiter(weights[face], -1, lPhi[own], lPhi[nei], grad[own], grad[nei], d);
        const scalar wp = lp*weights[face] + (1-lp);
        const scalar wn = ln*weights[face];
        p[face] = wp*(vf[own]-vf[nei])+vf[nei];
        n[face] = wn*(vf[own]-vf[nei])+vf[nei];
    }
    auto& pb = positive.ref().boundaryFieldRef();
    auto& nb = negative.ref().boundaryFieldRef();
    forAll(pb, patchi)
    {
        const auto& patch = vf.boundaryField()[patchi];
        if (!patch.coupled())
        {
            pb[patchi] = patch;
            nb[patchi] = patch;
            continue;
        }
        const scalarField lp(lPhi.boundaryField()[patchi].patchInternalField());
        const scalarField ln(lPhi.boundaryField()[patchi].patchNeighbourField());
        const vectorField gp(grad.boundaryField()[patchi].patchInternalField());
        const vectorField gn(grad.boundaryField()[patchi].patchNeighbourField());
        const vectorField d(weights.boundaryField()[patchi].patch().delta());
        const Field<Type> vp(patch.patchInternalField()), vn(patch.patchNeighbourField());
        const auto& w = weights.boundaryField()[patchi];
        forAll(pb[patchi], face)
        {
            const scalar ap = limiter.limiter(w[face], 1, lp[face], ln[face], gp[face], gn[face], d[face]);
            const scalar an = limiter.limiter(w[face], -1, lp[face], ln[face], gp[face], gn[face], d[face]);
            pb[patchi][face] = lerp(vn[face], vp[face], ap*w[face]+(1-ap));
            nb[patchi][face] = lerp(vn[face], vp[face], an*w[face]);
        }
    }
    // Opt-in native regression: compare every face against the original path.
    if (mesh.time().controlDict().getOrDefault<Switch>("verifyPairedReconstruction", false))
    {
        auto check = [&](const Surface& actual, const surfaceScalarField& direction)
        {
            const auto reference = interpolate(vf, direction, reconFieldName);
            const auto difference = mag(actual-reference());
            const auto magnitude = mag(reference());
            scalar error = max(difference().primitiveField());
            scalar scale = max(magnitude().primitiveField());
            forAll(actual.boundaryField(), patchi)
            {
                error = max(error, max(difference().boundaryField()[patchi]));
                scale = max(scale, max(magnitude().boundaryField()[patchi]));
            }
            if (returnReduce(error, maxOp<scalar>()) >
                1e-12*(1+returnReduce(scale, maxOp<scalar>())))
            {
                FatalErrorInFunction << "Paired Minmod differs from stock reconstruction for "
                    << vf.name() << exit(FatalError);
            }
        };
        check(positive(), pos); check(negative(), neg);
    }
    return Pair(std::move(positive), std::move(negative));
}

}
