#include "vortia/interop/fluent/FluentCffImporter.hpp"

#include "vortia/core/Logger.hpp"

#include <vtkFLUENTCFFReader.h>
#include <vtkMultiBlockDataSet.h>

#include <filesystem>

namespace vortia::fluent {

FluentCffImporter::Result FluentCffImporter::import(const std::filesystem::path& caseFile) {
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
    reader->SetRenameArrays(true);
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
    result.report.notes.push_back("Opened with vtkFLUENTCFFReader (VTK 9.7). "
                                  "Geometry and result fields are available for visualization.");
    result.report.notes.push_back("Conversion to an OpenFOAM runnable case from CFF is not "
                                  "supported by the MVP; use the traditional .msh path for simulation.");
    result.ok = true;

    log(LogLevel::Info, "Imported Fluent CFF case: " + caseFile.string());
    return result;
}

} // namespace vortia::fluent
