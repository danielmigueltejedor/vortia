#pragma once

#include "vortia/interop/fluent/FluentTypes.hpp"

#include <vtkObject.h>
#include <vtkSmartPointer.h>

class vtkMultiBlockDataSet;

namespace vortia::fluent {

/// Wraps vtkFLUENTCFFReader to open Fluent CFF (.cas.h5 / .dat.h5) files
/// for visualization. Does not convert to OpenFOAM; produces a VTK multiblock
/// dataset plus a compatibility report.
class FluentCffImporter {
public:
    struct Result {
        vtkSmartPointer<vtkMultiBlockDataSet> dataOwner;
        vtkObject* data{nullptr};
        CompatibilityReport report;
        bool ok{false};
    };

    [[nodiscard]] static Result import(const std::filesystem::path& caseFile);
};

} // namespace vortia::fluent
