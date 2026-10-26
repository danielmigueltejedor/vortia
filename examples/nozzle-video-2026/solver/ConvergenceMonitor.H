// Native field and conservative-flux checks for steady localEuler calculations.
// Linear-system diagonal residuals are deliberately not acceptance criteria.
#pragma once
#include "ConvergencePolicy.H"
#include <sstream>
#include <iomanip>
#include <fstream>
#include <cstdio>
class VortiaConvergenceMonitor {
    Foam::Time& time_;
    const Foam::fvMesh& mesh_;
    const Foam::volScalarField &p_, &T_, &rho_;
    const Foam::volVectorField& U_;
    vortiaConvergence::Limits limits_;
    bool automatic_;
    vortiaConvergence::Policy policy_;
    Foam::scalarField oldP_, oldT_, oldRho_, oldNu_;
    Foam::vectorField oldU_;
    int elapsed_=0;
    double checkedIteration_=0;
    double massIncoming_=0,massOutgoing_=0;
    bool reached_=false, sampled_=false;
    vortiaConvergence::Metrics metrics_;
    static vortiaConvergence::Limits read(const Foam::dictionary& d) {
        vortiaConvergence::Limits v;
        v.interval=d.getOrDefault<Foam::label>("interval",50);
        v.minimumIterations=d.getOrDefault<Foam::label>("minimumIterations",500);
        v.stableSamples=d.getOrDefault<Foam::label>("stableSamples",5);
        v.massTolerance=d.getOrDefault<Foam::scalar>("massTolerance",1e-3);
        v.fieldTolerance=d.getOrDefault<Foam::scalar>("fieldTolerance",1e-3);
        v.equationTolerance=d.getOrDefault<Foam::scalar>("equationTolerance",1e-5);
        v.absoluteMassTolerance=d.getOrDefault<Foam::scalar>("absoluteMassTolerance",1e-9);
        if(!v.valid()) FatalErrorInFunction << "Invalid Vortia convergence controls" << Foam::exit(Foam::FatalError);
        return v;
    }
    double scalarChange(const Foam::volScalarField& f,Foam::scalarField& old,double floor,bool positive) {
        double change=0;
        forAll(f.primitiveField(),i) {
            const double x=f[i];
            if(!std::isfinite(x)||(positive&&x<=0))metrics_.physical=false;
            change=std::max(change,std::abs(x-old[i])/std::max(floor,std::max(std::abs(x),std::abs(old[i]))));
        }
        forAll(f.boundaryField(),patchi) forAll(f.boundaryField()[patchi],i) {
            const auto x=f.boundaryField()[patchi][i];
            if(!std::isfinite(x)||(positive&&x<=0))metrics_.physical=false;
        }
        old=f.primitiveField();
        return change;
    }
public:
    VortiaConvergenceMonitor(Foam::Time& t,const Foam::fvMesh& m,const Foam::volScalarField& p,
        const Foam::volScalarField& T,const Foam::volScalarField& rho,const Foam::volVectorField& U)
      :time_(t),mesh_(m),p_(p),T_(T),rho_(rho),U_(U),
       limits_(read(t.controlDict().subOrEmptyDict("vortiaConvergence"))),
       automatic_(t.controlDict().subOrEmptyDict("vortiaConvergence").getOrDefault<bool>("enabled",true)),
       policy_(limits_),oldP_(p.primitiveField()),oldT_(T.primitiveField()),oldRho_(rho.primitiveField()),oldU_(U.primitiveField()) {
        if(mesh_.foundObject<Foam::volScalarField>("nuTilda"))
            oldNu_=mesh_.lookupObject<Foam::volScalarField>("nuTilda").primitiveField();
        // Replace any previous assessment immediately; a resumed run has a new stability window.
        report("warming");
    }
    bool check(const Foam::surfaceScalarField& phi,const Foam::volScalarField& p0,
        const Foam::volScalarField& T0,const Foam::volScalarField& rho0,const Foam::volVectorField& U0,
        const Foam::volScalarField& rDeltaT,const Foam::volScalarField& soundSpeed) {
        ++elapsed_;
        if(elapsed_%limits_.interval)return false;
        metrics_=vortiaConvergence::Metrics{};
        double incoming=0,outgoing=0;int patches=0;
        forAll(mesh_.boundary(),patchi) {
            // Generated open boundaries are generic patches. Ignore wall, axis,
            // wedge and MPI interfaces, including empty per-rank patches.
            if(mesh_.boundaryMesh()[patchi].type()!="patch")continue;
            patches+=mesh_.boundary()[patchi].size()>0;
            forAll(phi.boundaryField()[patchi],i) {
                const double f=phi.boundaryField()[patchi][i];
                if(!std::isfinite(f))metrics_.physical=false;
                incoming+=std::max(-f,0.0);outgoing+=std::max(f,0.0);
            }
        }
        Foam::reduce(incoming,Foam::sumOp<double>());Foam::reduce(outgoing,Foam::sumOp<double>());
        massIncoming_=incoming;massOutgoing_=outgoing;
        Foam::reduce(patches,Foam::sumOp<int>());metrics_.openBoundary=patches>0;
        const double scale=std::max(std::max(incoming,outgoing),limits_.absoluteMassTolerance/limits_.massTolerance);
        metrics_.globalMass=std::abs(outgoing-incoming)/scale;
        const Foam::volScalarField divPhi(Foam::fvc::div(phi));
        double local=0;
        forAll(divPhi.primitiveField(),i) {
            const double flux=std::abs(divPhi[i])*mesh_.V()[i];
            if(!std::isfinite(flux))metrics_.physical=false;
            local+=flux;
        }
        Foam::reduce(local,Foam::sumOp<double>());metrics_.localMass=local/scale;
        metrics_.fieldChange=std::max(scalarChange(p_,oldP_,1.0,true),scalarChange(T_,oldT_,1.0,true));
        metrics_.fieldChange=std::max(metrics_.fieldChange,scalarChange(rho_,oldRho_,1e-9,true));
        if(oldNu_.size()) metrics_.fieldChange=std::max(metrics_.fieldChange,
            scalarChange(mesh_.lookupObject<Foam::volScalarField>("nuTilda"),oldNu_,1e-8,false));
        forAll(U_.primitiveField(),i) {
            const auto& x=U_[i];
            for(int c=0;c<3;++c)if(!std::isfinite(x[c]))metrics_.physical=false;
            metrics_.fieldChange=std::max(metrics_.fieldChange,Foam::mag(x-oldU_[i])/std::max(1.0,std::max(Foam::mag(x),Foam::mag(oldU_[i]))));
        }
        forAll(U_.boundaryField(),patchi)forAll(U_.boundaryField()[patchi],i)
            for(int c=0;c<3;++c)if(!std::isfinite(U_.boundaryField()[patchi][i][c]))metrics_.physical=false;
        // Divide conservative-step changes by the accepted local pseudo-step,
        // then normalize with an acoustic cell crossing time. Tiny Courant or
        // diffusion-limited steps alone cannot create a false steady plateau.
        forAll(U_.primitiveField(),i) {
            const double speed=std::max(1.0,soundSpeed[i]+Foam::mag(U_[i]));
            const double crossing=std::cbrt(mesh_.V()[i])/speed;
            const double rate=rDeltaT[i]*crossing;
            const double u=Foam::mag(U_[i]-U0[i])/speed;
            const double p=std::abs(p_[i]-p0[i])/std::max(1.0,std::abs(p_[i]));
            const double T=std::abs(T_[i]-T0[i])/std::max(1.0,std::abs(T_[i]));
            const double rho=std::abs(rho_[i]-rho0[i])/std::max(1e-9,std::abs(rho_[i]));
            const double error=rate*std::max(std::max(u,p),std::max(T,rho));
            if(!std::isfinite(error)||rate<=0)metrics_.physical=false;
            metrics_.equationError=std::max(metrics_.equationError,error);
        }
        Foam::reduce(metrics_.equationError,Foam::maxOp<double>());
        oldU_=U_.primitiveField();
        Foam::reduce(metrics_.fieldChange,Foam::maxOp<double>());
        Foam::reduce(metrics_.physical,Foam::andOp<bool>());
        checkedIteration_=time_.value();
        reached_=policy_.sample(elapsed_,metrics_);sampled_=true;
        report(!metrics_.physical?"invalid":reached_?"converged":elapsed_<limits_.minimumIterations?"warming":policy_.stable()?"stabilizing":"checking");
        if(!metrics_.physical) FatalErrorInFunction << "Automatic checks detected invalid numerical fields" << Foam::exit(Foam::FatalError);
        return automatic_&&reached_;
    }
    bool advanced() const {return elapsed_>0;}
    void finish() { report(reached_&&elapsed_%limits_.interval==0?"converged":metrics_.physical?"limit":"invalid"); }
    void report(const char* status) const {
        if(!Foam::Pstream::master())return;
        auto finite=[](double x){return std::isfinite(x)?x:1e300;};
        std::ostringstream s;s<<std::setprecision(12);
        s<<"{\"schema\":1,\"status\":\""<<status<<"\",\"iteration\":"<<time_.value()
         <<",\"checkedIteration\":"<<checkedIteration_<<",\"sampled\":"<<(sampled_?"true":"false")<<",\"automatic\":"<<(automatic_?"true":"false")
         <<",\"globalMass\":"<<finite(metrics_.globalMass)<<",\"localMass\":"<<finite(metrics_.localMass)
         <<",\"massIncoming\":"<<finite(massIncoming_)<<",\"massOutgoing\":"<<finite(massOutgoing_)
         <<",\"equationError\":"<<finite(metrics_.equationError)<<",\"equationTolerance\":"<<limits_.equationTolerance
         <<",\"fieldChange\":"<<finite(metrics_.fieldChange)<<",\"stableSamples\":"<<policy_.stable()
         <<",\"requiredSamples\":"<<limits_.stableSamples<<",\"massTolerance\":"<<limits_.massTolerance
         <<",\"fieldTolerance\":"<<limits_.fieldTolerance<<",\"interval\":"<<limits_.interval<<"}";
        Foam::Info<<"VORTIA_CONVERGENCE "<<s.str().c_str()<<Foam::endl;
        const std::string path=(time_.globalPath()/"vortia-convergence.json").c_str();
        std::ofstream out(path+".tmp");out<<s.str()<<"\n";out.close();
        if(out)std::rename((path+".tmp").c_str(),path.c_str());
    }
};
