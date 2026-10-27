#pragma once
#include <QJsonObject>
#include <array>
class vtkDataObject;
namespace vortia {
struct SectionOptions {
    int axis{0}, normal{1};
    double position{}, referencePressure{101325}, constantDensity{1.225};
    double referenceDensity{1.225}, referenceSpeed{100}, referenceArea{1};
    bool useDensityField{true}, kinematicPressure{false}, fullAnnulus{false};
    double wedgeAngle{};
};
struct SectionAnalysis {
    double area{}, massFlow{}, forwardMassFlow{}, reverseMassFlow{}, volumeFlow{};
    double meanPressure{}, meanDensity{}, meanNormalVelocity{}, momentumFlux{}, pressureLoad{}, resultant{}, kineticPower{};
    double coefficient{}, meanCp{}, areaScale{1};
    int triangles{}, duplicateTriangles{}, ghostTriangles{}, blocks{};
    QString sampleFingerprint;
};
std::array<double,6> sectionAnalysisBounds(vtkDataObject*);
SectionAnalysis analyzeSection(vtkDataObject*,const SectionOptions&);
QJsonObject sectionOptionsJson(const SectionOptions&);
SectionOptions sectionOptionsFromJson(const QJsonObject&);
QJsonObject sectionMetricsJson(const SectionAnalysis&);
}
