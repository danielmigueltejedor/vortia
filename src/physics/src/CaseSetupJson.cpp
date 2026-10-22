#include "vortia/physics/CaseSetupJson.hpp"

#include <nlohmann/json.hpp>

namespace vortia {

namespace {

using nlohmann::json;

// --- Enum <-> string mappings (stable identifiers, part of the file format) --

std::string nameOf(PatchKind k) { return std::string(toString(k)); }

Expected<PatchKind> patchKindFrom(const std::string& s) {
    if (s == "wall") return PatchKind::Wall;
    if (s == "patch") return PatchKind::Patch;
    if (s == "empty") return PatchKind::Empty;
    if (s == "symmetry") return PatchKind::Symmetry;
    return Error::make(ErrorCode::InvalidData, "unknown patch kind '" + s + "'");
}

std::string nameOf(VelocityBc b) {
    switch (b) {
    case VelocityBc::NoSlip: return "noSlip";
    case VelocityBc::FixedValue: return "fixedValue";
    case VelocityBc::ZeroGradient: return "zeroGradient";
    case VelocityBc::Symmetry: return "symmetry";
    case VelocityBc::Empty: return "empty";
    }
    return "zeroGradient";
}

Expected<VelocityBc> velocityBcFrom(const std::string& s) {
    if (s == "noSlip") return VelocityBc::NoSlip;
    if (s == "fixedValue") return VelocityBc::FixedValue;
    if (s == "zeroGradient") return VelocityBc::ZeroGradient;
    if (s == "symmetry") return VelocityBc::Symmetry;
    if (s == "empty") return VelocityBc::Empty;
    return Error::make(ErrorCode::InvalidData, "unknown velocity BC '" + s + "'");
}

std::string nameOf(PressureBc b) {
    switch (b) {
    case PressureBc::ZeroGradient: return "zeroGradient";
    case PressureBc::FixedValue: return "fixedValue";
    case PressureBc::Symmetry: return "symmetry";
    case PressureBc::Empty: return "empty";
    }
    return "zeroGradient";
}

Expected<PressureBc> pressureBcFrom(const std::string& s) {
    if (s == "zeroGradient") return PressureBc::ZeroGradient;
    if (s == "fixedValue") return PressureBc::FixedValue;
    if (s == "symmetry") return PressureBc::Symmetry;
    if (s == "empty") return PressureBc::Empty;
    return Error::make(ErrorCode::InvalidData, "unknown pressure BC '" + s + "'");
}

std::string nameOf(TimeScheme t) {
    switch (t) {
    case TimeScheme::Euler: return "euler";
    case TimeScheme::Backward: return "backward";
    case TimeScheme::SteadyState: return "steadyState";
    }
    return "euler";
}

Expected<TimeScheme> timeSchemeFrom(const std::string& s) {
    if (s == "euler") return TimeScheme::Euler;
    if (s == "backward") return TimeScheme::Backward;
    if (s == "steadyState") return TimeScheme::SteadyState;
    return Error::make(ErrorCode::InvalidData, "unknown time scheme '" + s + "'");
}

std::string nameOf(SolverApp a) {
    switch (a) {
    case SolverApp::IcoFoam: return "icoFoam";
    case SolverApp::SimpleFoam: return "simpleFoam";
    }
    return "icoFoam";
}

Expected<SolverApp> solverAppFrom(const std::string& s) {
    if (s == "icoFoam") return SolverApp::IcoFoam;
    if (s == "simpleFoam") return SolverApp::SimpleFoam;
    return Error::make(ErrorCode::InvalidData, "unknown solver '" + s + "'");
}

json vecToJson(const Vec3& v) { return json::array({v.x, v.y, v.z}); }

Expected<Vec3> vecFromJson(const json& j, const char* key) {
    if (!j.is_array() || j.size() != 3)
        return Error::make(ErrorCode::InvalidData,
                           std::string(key) + " must be an array of 3 numbers");
    Vec3 v;
    for (int i = 0; i < 3; ++i) {
        if (!j[i].is_number())
            return Error::make(ErrorCode::InvalidData,
                               std::string(key) + " components must be numbers");
        v[i] = j[i].get<double>();
    }
    return v;
}

double num(const json& j, const char* key, double fallback) {
    return j.value(key, fallback);
}

} // namespace

std::string caseSetupToJson(const CaseSetup& s) {
    json j;
    j["domain"] = {{"min", vecToJson(s.domain.min())}, {"max", vecToJson(s.domain.max())}};
    j["mesh"] = {{"cells", {s.mesh.cells[0], s.mesh.cells[1], s.mesh.cells[2]}},
                 {"grading", {s.mesh.grading[0], s.mesh.grading[1], s.mesh.grading[2]}}};
    json patches = json::array();
    for (const auto& p : s.mesh.patches)
        patches.push_back({{"face", static_cast<int>(p.face)},
                           {"name", p.name},
                           {"kind", nameOf(p.kind)}});
    j["mesh"]["patches"] = std::move(patches);
    j["material"] = {{"name", s.material.name}, {"nu", s.material.nu.siValue()}};
    json bcs = json::array();
    for (const auto& b : s.boundaries)
        bcs.push_back({{"patch", b.patchName},
                       {"velocity", nameOf(b.velocity)},
                       {"velocityValue", vecToJson(b.velocityValue)},
                       {"pressure", nameOf(b.pressure)},
                       {"pressureValue", b.pressureValue}});
    j["boundaries"] = std::move(bcs);
    j["internalField"] = {{"velocity", vecToJson(s.internalVelocity)},
                          {"pressure", s.internalPressure}};
    j["time"] = {{"startTime", s.time.startTime},
                 {"endTime", s.time.endTime},
                 {"deltaT", s.time.deltaT},
                 {"writeInterval", s.time.writeInterval},
                 {"adjustTimeStep", s.time.adjustTimeStep},
                 {"maxCo", s.time.maxCo}};
    j["numerics"] = {{"ddt", nameOf(s.numerics.ddt)},
                     {"gradScheme", s.numerics.gradScheme},
                     {"divPhiU", s.numerics.divPhiU},
                     {"laplacian", s.numerics.laplacian},
                     {"pisoCorrectors", s.numerics.pisoCorrectors},
                     {"toleranceP", s.numerics.toleranceP},
                     {"toleranceU", s.numerics.toleranceU}};
    j["solver"] = nameOf(s.solver);
    return j.dump(2);
}

Expected<CaseSetup> caseSetupFromJson(const std::string& text) {
    json j;
    try {
        j = json::parse(text);
    } catch (const json::exception& e) {
        return Error::make(ErrorCode::ParseError, e.what());
    }

    CaseSetup s;
    try {
        // Domain
        auto min = vecFromJson(j.at("domain").at("min"), "domain.min");
        if (!min) return min.error();
        auto max = vecFromJson(j.at("domain").at("max"), "domain.max");
        if (!max) return max.error();
        auto domain = BoxDomain::fromExtents(min.value(), max.value());
        if (!domain) return domain.error();
        s.domain = domain.value();

        // Mesh
        const auto& jm = j.at("mesh");
        const auto& cells = jm.at("cells");
        const auto& grading = jm.at("grading");
        for (int i = 0; i < 3; ++i) {
            s.mesh.cells[i] = cells.at(i).get<int>();
            s.mesh.grading[i] = grading.at(i).get<double>();
        }
        for (const auto& jp : jm.at("patches")) {
            const int face = jp.at("face").get<int>();
            if (face < 0 || face > 5)
                return Error::make(ErrorCode::InvalidData, "patch face index out of range");
            auto kind = patchKindFrom(jp.at("kind").get<std::string>());
            if (!kind) return kind.error();
            s.mesh.patches[static_cast<std::size_t>(face)] =
                FacePatch{static_cast<BoxDomain::Face>(face), jp.at("name").get<std::string>(),
                          kind.value()};
        }

        // Material
        s.material.name = j.at("material").value("name", std::string{"fluid"});
        s.material.nu = KinematicViscosity(j.at("material").at("nu").get<double>());

        // Boundaries
        s.boundaries.clear();
        for (const auto& jb : j.at("boundaries")) {
            PatchBoundary b;
            b.patchName = jb.at("patch").get<std::string>();
            auto vb = velocityBcFrom(jb.at("velocity").get<std::string>());
            if (!vb) return vb.error();
            b.velocity = vb.value();
            auto vv = vecFromJson(jb.at("velocityValue"), "velocityValue");
            if (!vv) return vv.error();
            b.velocityValue = vv.value();
            auto pb = pressureBcFrom(jb.at("pressure").get<std::string>());
            if (!pb) return pb.error();
            b.pressure = pb.value();
            b.pressureValue = jb.at("pressureValue").get<double>();
            s.boundaries.push_back(std::move(b));
        }

        // Internal field
        if (j.contains("internalField")) {
            auto iv = vecFromJson(j["internalField"].at("velocity"), "internalField.velocity");
            if (!iv) return iv.error();
            s.internalVelocity = iv.value();
            s.internalPressure = j["internalField"].value("pressure", 0.0);
        }

        // Time + numerics
        const auto& jt = j.at("time");
        s.time.startTime = num(jt, "startTime", 0.0);
        s.time.endTime = jt.at("endTime").get<double>();
        s.time.deltaT = jt.at("deltaT").get<double>();
        s.time.writeInterval = jt.value("writeInterval", 100);
        s.time.adjustTimeStep = jt.value("adjustTimeStep", false);
        s.time.maxCo = jt.value("maxCo", 1.0);

        if (j.contains("numerics")) {
            const auto& jn = j["numerics"];
            auto ddt = timeSchemeFrom(jn.value("ddt", std::string{"euler"}));
            if (!ddt) return ddt.error();
            s.numerics.ddt = ddt.value();
            s.numerics.gradScheme = jn.value("gradScheme", std::string{"Gauss linear"});
            s.numerics.divPhiU = jn.value("divPhiU", std::string{"Gauss linear"});
            s.numerics.laplacian = jn.value("laplacian", std::string{"Gauss linear corrected"});
            s.numerics.pisoCorrectors = jn.value("pisoCorrectors", 2);
            s.numerics.toleranceP = jn.value("toleranceP", 1e-6);
            s.numerics.toleranceU = jn.value("toleranceU", 1e-7);
        }

        auto solver = solverAppFrom(j.value("solver", std::string{"icoFoam"}));
        if (!solver) return solver.error();
        s.solver = solver.value();
    } catch (const json::exception& e) {
        return Error::make(ErrorCode::InvalidData,
                           std::string("malformed case setup JSON: ") + e.what());
    }

    // Structural validation: refuse to load physically inconsistent setups.
    const auto issues = s.validate();
    for (const auto& issue : issues)
        if (issue.severity == Severity::Error)
            return Error::make(ErrorCode::InvalidData, "loaded setup is invalid: " + issue.message);
    return s;
}

} // namespace vortia
