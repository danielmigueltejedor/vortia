#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/physics/Physics.hpp"

#include <string>

namespace vortia {

/// JSON (de)serialization of the full case setup stored in the project
/// manifest. Parsing is strict: unknown enum values, missing required keys
/// and type mismatches are errors — a silently mis-loaded case is worse
/// than a refused one.
[[nodiscard]] std::string caseSetupToJson(const CaseSetup& setup);
[[nodiscard]] Expected<CaseSetup> caseSetupFromJson(const std::string& json);

} // namespace vortia
