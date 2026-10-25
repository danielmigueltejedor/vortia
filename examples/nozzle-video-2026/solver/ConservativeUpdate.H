// Algebraic solution of static-mesh localEuler diagonal conservative equations.
// Preserve fvMatrix's Vsc scaling and arithmetic order, and its boundary update.
// This changes allocation/dispatch cost, not the fluxes, pseudo-step or physics.
#pragma once
template<class Type>
void directConservativeUpdate
(
    Foam::GeometricField<Type,Foam::fvPatchField,Foam::volMesh>& field,
    const Foam::volScalarField& reciprocalStep,
    const Foam::GeometricField<Type,Foam::fvPatchField,Foam::volMesh>& divergence
)
{
    const auto& old=field.oldTime().primitiveField();
    const auto volumeStorage=field.mesh().Vsc();
    const auto& volumes=volumeStorage();
    auto& current=field.primitiveFieldRef();
    forAll(current,i) {
        const auto diagonal=reciprocalStep[i]*volumes[i];
        current[i]=(reciprocalStep[i]*old[i]*volumes[i]-divergence[i]*volumes[i])/diagonal;
    }
    field.correctBoundaryConditions();
}
template<class Type>
void directConservativeUpdate
(
    Foam::GeometricField<Type,Foam::fvPatchField,Foam::volMesh>& field,
    const Foam::volScalarField& reciprocalStep,
    const Foam::GeometricField<Type,Foam::fvPatchField,Foam::volMesh>& divergence,
    const Foam::GeometricField<Type,Foam::fvPatchField,Foam::volMesh>& viscousDivergence
)
{
    const auto& old=field.oldTime().primitiveField();
    const auto volumeStorage=field.mesh().Vsc();
    const auto& volumes=volumeStorage();
    auto& current=field.primitiveFieldRef();
    forAll(current,i) {
        const auto diagonal=reciprocalStep[i]*volumes[i];
        current[i]=(reciprocalStep[i]*old[i]*volumes[i]-divergence[i]*volumes[i]+viscousDivergence[i]*volumes[i])/diagonal;
    }
    field.correctBoundaryConditions();
}

// Reuse field/patch allocations; coefficient dictionaries and patch types stay
// fixed during these generated static-domain runs. Values are copied each step.
template<class Type>
class ConservativeSnapshot {
    using Field=Foam::GeometricField<Type,Foam::fvPatchField,Foam::volMesh>;
    Foam::word name_;bool reuse_;Foam::autoPtr<Field> field_;
public:
    ConservativeSnapshot(const Foam::word& name,bool reuse):name_(name),reuse_(reuse){}
    const Field& capture(const Field& source) {
        if(reuse_&&field_.valid())field_.ref()=source;
        else {field_.clear();field_.reset(new Field(name_,source));}
        return field_();
    }
};
