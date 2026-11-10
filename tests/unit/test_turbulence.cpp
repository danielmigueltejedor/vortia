#include <gtest/gtest.h>

#include "vortia/physics/CaseSetupJson.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/TurbulentFlatPlate.hpp"

#include <nlohmann/json.hpp>

#include <cmath>

using namespace vortia;
using namespace vortia::openfoam;

namespace {

Capabilities esi() {
    Installation i;
    i.distribution = Distribution::Esi;
    i.version = Version::parse("v2412").value();
    return capabilitiesFor(i);
}

bool hasError(const CaseSetup& s, const std::string& fragment) {
    for (const auto& issue : s.validate())
        if (issue.severity == Severity::Error && issue.message.find(fragment) != std::string::npos)
            return true;
    return false;
}

bool hasAnyError(const CaseSetup& s) {
    for (const auto& issue : s.validate())
        if (issue.severity == Severity::Error)
            return true;
    return false;
}

} // namespace

TEST(TurbulenceQuantities, MatchTheirDefinitions) {
    // U = 10 m/s, I = 5 %: k = 1.5 * 0.5^2 = 0.375 m2/s2.
    EXPECT_DOUBLE_EQ(turbulentKineticEnergy(10.0, 0.05), 0.375);
    // omega = sqrt(k) / (0.09^0.25 * L).
    EXPECT_NEAR(specificDissipationRate(0.375, 0.01),
                std::sqrt(0.375) / (std::pow(0.09, 0.25) * 0.01), 1e-9);
    EXPECT_DOUBLE_EQ(turbulentKineticEnergy(0.0, 0.05), 0.0);
    EXPECT_TRUE(std::isnan(turbulentKineticEnergy(-1.0, 0.05)));
    EXPECT_TRUE(std::isnan(specificDissipationRate(0.375, 0.0)));
    EXPECT_TRUE(std::isnan(specificDissipationRate(-0.1, 0.01)));
}

TEST(TurbulenceValidation, PlateCaseIsValidAndLaminarDefaultIsUnchanged) {
    EXPECT_FALSE(hasAnyError(verification::makeTurbulentPlateCase({})));
    const CaseSetup laminar = verification::makeGhiaCavityCase();
    EXPECT_EQ(laminar.turbulence.model, TurbulenceModel::Laminar);
    EXPECT_FALSE(hasAnyError(laminar));
}

TEST(TurbulenceValidation, RejectsUnsupportedCombinationsAndBadInputs) {
    auto s = verification::makeTurbulentPlateCase({});
    s.turbulence.intensity = 0.0;
    EXPECT_TRUE(hasError(s, "intensity"));
    s.turbulence.intensity = 1.5;
    EXPECT_TRUE(hasError(s, "intensity"));
    s = verification::makeTurbulentPlateCase({});
    s.turbulence.lengthScale = -1.0;
    EXPECT_TRUE(hasError(s, "length scale"));
    s = verification::makeTurbulentPlateCase({});
    s.turbulence.referenceSpeed = -2.0;
    EXPECT_TRUE(hasError(s, "reference speed"));
    // Transient icoFoam is laminar only.
    s = verification::makeTurbulentPlateCase({});
    s.solver = SolverApp::IcoFoam;
    s.numerics.ddt = TimeScheme::Euler;
    EXPECT_TRUE(hasError(s, "steady simpleFoam"));
    // No velocity anywhere to derive k from.
    s = verification::makeTurbulentPlateCase({});
    s.internalVelocity = {};
    for (auto& bc : s.boundaries)
        if (bc.velocity == VelocityBc::FixedValue) bc.velocityValue = {};
    EXPECT_TRUE(hasError(s, "prescribed velocity"));
    s.turbulence.referenceSpeed = 10.0;
    EXPECT_FALSE(hasError(s, "prescribed velocity"));
}

TEST(TurbulenceWriter, LaminarCasesGetNoTurbulenceFieldsOrSchemes) {
    const CaseSetup s = verification::makeGhiaCavityCase();
    EXPECT_TRUE(CaseWriter::fieldK(s).empty());
    EXPECT_TRUE(CaseWriter::fieldOmega(s).empty());
    EXPECT_TRUE(CaseWriter::fieldNut(s).empty());
    EXPECT_EQ(CaseWriter::fvSchemes(s).find("div(phi,k)"), std::string::npos);
    EXPECT_EQ(CaseWriter::fvSolution(s).find("omega"), std::string::npos);
    EXPECT_NE(CaseWriter::turbulenceProperties(s, esi()).find("simulationType  laminar;"),
              std::string::npos);
}

TEST(TurbulenceWriter, SstCaseDeclaresModelSchemesSolversAndWallFunctions) {
    const verification::TurbulentPlate plate;
    const CaseSetup s = verification::makeTurbulentPlateCase(plate);
    const std::string model = CaseWriter::turbulenceProperties(s, esi());
    EXPECT_NE(model.find("simulationType  RAS;"), std::string::npos);
    EXPECT_NE(model.find("RASModel        kOmegaSST;"), std::string::npos);

    const std::string schemes = CaseWriter::fvSchemes(s);
    EXPECT_NE(schemes.find("div(phi,k)      bounded Gauss upwind;"), std::string::npos);
    EXPECT_NE(schemes.find("div(phi,omega)  bounded Gauss upwind;"), std::string::npos);
    EXPECT_NE(schemes.find("wallDist"), std::string::npos);
    const std::string solution = CaseWriter::fvSolution(s);
    EXPECT_NE(solution.find("\"(k|omega)\""), std::string::npos);

    const std::string k = CaseWriter::fieldK(s);
    // k = 1.5 (10 * 0.01)^2 = 0.015
    EXPECT_NE(k.find("internalField   uniform 0.015;"), std::string::npos);
    EXPECT_NE(k.find("kqRWallFunction"), std::string::npos);
    EXPECT_NE(k.find("fixedValue"), std::string::npos);   // inlet
    EXPECT_NE(k.find("zeroGradient"), std::string::npos); // outlet / far field
    EXPECT_NE(k.find("empty"), std::string::npos);
    EXPECT_NE(CaseWriter::fieldOmega(s).find("omegaWallFunction"), std::string::npos);
    const std::string nut = CaseWriter::fieldNut(s);
    EXPECT_NE(nut.find("nutkWallFunction"), std::string::npos);
    EXPECT_NE(nut.find("calculated"), std::string::npos);
}

TEST(TurbulenceWriter, FoundationUsesMomentumTransportDictionary) {
    Installation i;
    i.distribution = Distribution::Foundation;
    i.version = Version::parse("11").value();
    const auto caps = capabilitiesFor(i);
    EXPECT_EQ(caps.turbulenceDict, "momentumTransport");
    const auto text = CaseWriter::turbulenceProperties(verification::makeTurbulentPlateCase({}), caps);
    EXPECT_NE(text.find("model           kOmegaSST;"), std::string::npos);
    EXPECT_EQ(esi().turbulenceDict, "turbulenceProperties");
}

TEST(TurbulenceJson, RoundTripsAndOldProjectsLoadAsLaminar) {
    const CaseSetup s = verification::makeTurbulentPlateCase({});
    const std::string text = caseSetupToJson(s);
    const auto loaded = caseSetupFromJson(text);
    ASSERT_TRUE(loaded.hasValue()) << loaded.error().describe();
    EXPECT_EQ(loaded->turbulence.model, TurbulenceModel::KOmegaSST);
    EXPECT_DOUBLE_EQ(loaded->turbulence.intensity, s.turbulence.intensity);
    EXPECT_DOUBLE_EQ(loaded->turbulence.lengthScale, s.turbulence.lengthScale);

    // A project saved before 0.3.0 has no "turbulence" object.
    auto old = nlohmann::json::parse(caseSetupToJson(verification::makeGhiaCavityCase()));
    ASSERT_EQ(old.erase("turbulence"), 1U);
    const auto legacy = caseSetupFromJson(old.dump());
    ASSERT_TRUE(legacy.hasValue()) << legacy.error().describe();
    EXPECT_EQ(legacy->turbulence.model, TurbulenceModel::Laminar);

    auto unknown = nlohmann::json::parse(text);
    unknown["turbulence"]["model"] = "kEpsilonDreams";
    EXPECT_FALSE(caseSetupFromJson(unknown.dump()).hasValue());
}

TEST(TurbulentPlateReference, SchlichtingLawValues) {
    // log10(1e7) = 7: C_D = 0.455 / 7^2.58
    EXPECT_NEAR(verification::schlichtingDragCoefficient(1.0e7), 0.455 / std::pow(7.0, 2.58), 1e-15);
    EXPECT_NEAR(verification::schlichtingDragCoefficient(1.0e7), 0.00300, 2e-5);
    EXPECT_TRUE(std::isnan(verification::schlichtingDragCoefficient(100.0)));
    const verification::TurbulentPlate plate;
    EXPECT_DOUBLE_EQ(verification::plateReynolds(plate), 1.0e7);
    EXPECT_NEAR(verification::schlichtingPlateForce(plate),
                verification::schlichtingDragCoefficient(1.0e7) * 0.5 * 100.0 * 10.0 * 0.1, 1e-15);
}

TEST(SteadyControlDict, HonoursTheIterationBudgetOfTheSetup) {
    auto s = verification::makeTurbulentPlateCase({});
    const std::string dict = CaseWriter::controlDict(s, esi());
    EXPECT_NE(dict.find("endTime         3000;"), std::string::npos);
    EXPECT_NE(dict.find("writeInterval   3000;"), std::string::npos);
    s.time.endTime = 0.5; // a transient default, meaningless as iterations
    EXPECT_NE(CaseWriter::controlDict(s, esi()).find("endTime         1000;"), std::string::npos);
}
