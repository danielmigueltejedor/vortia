#include "../../examples/nozzle-video-2026/solver/ConvergencePolicy.H"
#include <gtest/gtest.h>
#include <limits>
using namespace vortiaConvergence;
TEST(Convergence, RequiresWarmupAndConsecutiveWholeWindows) {
    Policy p(Limits{});Metrics good;
    EXPECT_FALSE(p.sample(450,good));EXPECT_EQ(p.stable(),0);
    for(int i=500;i<700;i+=50)EXPECT_FALSE(p.sample(i,good));
    EXPECT_TRUE(p.sample(700,good));
}
TEST(Convergence, CancellingBoundaryMassErrorsCannotHideLocalContinuityErrors) {
    Policy p(Limits{});Metrics m;m.globalMass=0;m.localMass=.02;
    for(int i=500;i<=1500;i+=50)EXPECT_FALSE(p.sample(i,m));
    EXPECT_EQ(p.stable(),0);
}
TEST(Convergence, SteadyMassWithChangingFieldsIsRejectedAndResetsWindow) {
    Policy p(Limits{});Metrics m;
    for(int i=500;i<700;i+=50)EXPECT_FALSE(p.sample(i,m));
    m.fieldChange=.01;EXPECT_FALSE(p.sample(700,m));EXPECT_EQ(p.stable(),0);
    m.fieldChange=0;EXPECT_FALSE(p.sample(750,m));EXPECT_EQ(p.stable(),1);
}
TEST(Convergence, NonPhysicalNonFiniteAndClosedDomainCannotBeAccepted) {
    for(int kind=0;kind<4;++kind) {
        Policy p(Limits{});Metrics m;
        if(kind==0)m.physical=false;
        if(kind==1)m.openBoundary=false;
        if(kind==2)m.fieldChange=std::numeric_limits<double>::quiet_NaN();
        if(kind==3)m.localMass=std::numeric_limits<double>::infinity();
        for(int i=500;i<1500;i+=50)EXPECT_FALSE(p.sample(i,m));
    }
}
TEST(Convergence, RejectsUnsafeCriteria) {
    Limits c;c.stableSamples=1;EXPECT_FALSE(c.valid());
    c=Limits{};c.massTolerance=.1;EXPECT_FALSE(c.valid());
    c=Limits{};c.minimumIterations=10;EXPECT_FALSE(c.valid());
}

TEST(Convergence, TinyPseudoStepsCannotMasqueradeAsSteadyState) {
    Policy p(Limits{});Metrics m;m.fieldChange=1e-9;m.equationError=.01;
    for(int i=500;i<=1500;i+=50)EXPECT_FALSE(p.sample(i,m));
    EXPECT_EQ(p.stable(),0);
}
