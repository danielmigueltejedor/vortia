// Numerical steady-state acceptance policy. Independent of OpenFOAM for testing.
#pragma once
#include <cmath>
namespace vortiaConvergence {
struct Limits {
    int interval=50, minimumIterations=500, stableSamples=5;
    double massTolerance=1e-3, fieldTolerance=1e-3, absoluteMassTolerance=1e-9, equationTolerance=1e-5;
    bool valid() const {
        return interval>=1 && interval<=10000 && minimumIterations>=interval && minimumIterations<=200000
            && stableSamples>=2 && stableSamples<=100 && std::isfinite(massTolerance)
            && massTolerance>=1e-8 && massTolerance<=0.01 && std::isfinite(fieldTolerance)
            && fieldTolerance>=1e-8 && fieldTolerance<=0.01 && std::isfinite(absoluteMassTolerance)
            && std::isfinite(equationTolerance) && equationTolerance>=1e-8 && equationTolerance<=1e-3
            && absoluteMassTolerance>=1e-15 && absoluteMassTolerance<=1e-6;
    }
};
struct Metrics {
    double globalMass=0, localMass=0, fieldChange=0, equationError=0;
    bool physical=true, openBoundary=true;
};
class Policy {
    Limits limits_;
    int stable_=0;
public:
    explicit Policy(Limits limits):limits_(limits) {}
    int stable() const { return stable_; }
    bool sample(int elapsed, const Metrics& m) {
        const bool finite=std::isfinite(m.globalMass)&&std::isfinite(m.localMass)&&std::isfinite(m.fieldChange)&&std::isfinite(m.equationError);
        const bool good=limits_.valid()&&m.physical&&m.openBoundary&&finite
            && m.globalMass>=0&&m.localMass>=0&&m.fieldChange>=0
            && m.globalMass<=limits_.massTolerance&&m.localMass<=limits_.massTolerance
            && m.fieldChange<=limits_.fieldTolerance && m.equationError>=0 && m.equationError<=limits_.equationTolerance;
        stable_=(elapsed>=limits_.minimumIterations && good)?stable_+1:0;
        return stable_>=limits_.stableSamples;
    }
};
}
