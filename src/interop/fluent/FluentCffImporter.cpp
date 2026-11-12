#include "vortia/interop/fluent/FluentCffImporter.hpp"

#include "vortia/core/Logger.hpp"

#include <vtkFLUENTCFFReader.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkVersionMacros.h>

#include <filesystem>

namespace vortia::fluent {

FluentCffImporter::Result FluentCffImporter::openCase(const std::filesystem::path& caseFile) {
    Result result;
    result.report.format = FluentFormat::CaseHdf5;
    result.report.filePath = caseFile.string();

    if (!std::filesystem::is_regular_file(caseFile)) {
        result.report.level = CompatibilityLevel::Incompatible;
        result.report.warnings.push_back("case file not found");
        return result;
    }

    vtkSmartPointer<vtkFLUENTCFFReader> reader = vtkSmartPointer<vtkFLUENTCFFReader>::New();
    reader->SetFileName(caseFile.string());
#if VTK_VERSION_NUMBER >= VTK_VERSION_CHECK(9, 7, 0)
    // Readable field names ("pressure" rather than "SV_P"). VTK 9.6, which Homebrew still ships,
    // has no switch and keeps Fluent's own names; the fields are the same either way.
    reader->SetRenameArrays(true);
#endif
    reader->Update();

    auto* output = reader->GetOutput();
    if (output == nullptr) {
        result.report.level = CompatibilityLevel::Incompatible;
        result.report.warnings.push_back("VTK reader produced no output");
        return result;
    }

    result.dataOwner = vtkMultiBlockDataSet::SafeDownCast(output);
    if (result.dataOwner == nullptr) {
        result.dataOwner = vtkSmartPointer<vtkMultiBlockDataSet>::New();
    }
    result.data = result.dataOwner.Get();

    result.report.level = CompatibilityLevel::VisualizationOnly;
    result.report.notes.push_back("Opened with vtkFLUENTCFFReader. "
                                  "Geometry and result fields are available for visualization.");
    result.report.notes.push_back("Conversion to an OpenFOAM runnable case from CFF is not "
                                  "supported by the MVP; use the traditional .msh path for simulation.");
    result.ok = true;

    log(LogLevel::Info, "Imported Fluent CFF case: " + caseFile.string());
    return result;
}

} // namespace vortia::fluent
