#include "vortia/visualization/SectionAnalysis.hpp"
#include "vortia/visualization/VtkViewport.hpp"
#include "vortia/ui/EngineeringReport.hpp"
#include "vortia/ui/EngineeringWorkbench.hpp"
#include "vortia/ui/ThemeManager.hpp"
#include <gtest/gtest.h>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDir>
#include <QFile>
#include <QDoubleSpinBox>
#include <QJsonDocument>
#include <QLineEdit>
#include <QPushButton>
#include <QTabWidget>
#include <QTableWidget>
#include <QTranslator>
#include <vtkCellData.h>
#include <vtkDoubleArray.h>
#include <vtkUnsignedCharArray.h>
#include <vtkImageData.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkNew.h>
#include <vtkPointData.h>
#include <vtkPlaneSource.h>
#include <limits>
using namespace vortia;
namespace {
vtkSmartPointer<vtkImageData> box(bool points=false,double y=0){
    auto data=vtkSmartPointer<vtkImageData>::New();data->SetDimensions(3,2,2);data->SetOrigin(0,y,0);data->SetSpacing(1,2,3);
    const auto count=points?data->GetNumberOfPoints():data->GetNumberOfCells();
    vtkNew<vtkDoubleArray> p;p->SetName("p");p->SetNumberOfTuples(count);p->FillComponent(0,120000);
    vtkNew<vtkDoubleArray> rho;rho->SetName("rho");rho->SetNumberOfTuples(count);rho->FillComponent(0,2);
    vtkNew<vtkDoubleArray> u;u->SetName("U");u->SetNumberOfComponents(3);u->SetNumberOfTuples(count);for(vtkIdType i=0;i<count;++i)u->SetTuple3(i,10,3,0);
    auto* fields=points?static_cast<vtkDataSetAttributes*>(data->GetPointData()):static_cast<vtkDataSetAttributes*>(data->GetCellData());fields->AddArray(p);fields->AddArray(rho);fields->AddArray(u);return data;
}
SectionOptions options(){SectionOptions o;o.position=.5;o.referencePressure=100000;o.referenceDensity=2;o.referenceSpeed=10;o.referenceArea=6;return o;}
}
TEST(EngineeringSection, UniformFluxPressureCoefficientsAndReversedNormal){
    const auto source=box();auto o=options();const auto r=analyzeSection(source,o);
    EXPECT_NEAR(r.area,6,1e-12);
    EXPECT_NEAR(r.massFlow,120,1e-10);
    EXPECT_NEAR(r.volumeFlow,60,1e-10);
    EXPECT_NEAR(r.forwardMassFlow,120,1e-10);
    EXPECT_DOUBLE_EQ(r.reverseMassFlow,0);
    EXPECT_NEAR(r.meanPressure,120000,1e-8);
    EXPECT_NEAR(r.meanDensity,2,1e-12);
    EXPECT_NEAR(r.momentumFlux,1200,1e-9);
    EXPECT_NEAR(r.pressureLoad,120000,1e-7);
    EXPECT_NEAR(r.resultant,121200,1e-7);
    EXPECT_NEAR(r.coefficient,202,1e-9);
    EXPECT_NEAR(r.meanCp,200,1e-9);
    EXPECT_NEAR(r.kineticPower,6540,1e-8);
    o.normal=-1;const auto reverse=analyzeSection(source,o);
    EXPECT_NEAR(reverse.massFlow,-120,1e-10);
    EXPECT_NEAR(reverse.reverseMassFlow,120,1e-10);
    EXPECT_DOUBLE_EQ(reverse.forwardMassFlow,0);
    EXPECT_NEAR(reverse.momentumFlux,r.momentumFlux,1e-10);
    EXPECT_NEAR(reverse.kineticPower,-r.kineticPower,1e-9);
    EXPECT_NE(reverse.sampleFingerprint,r.sampleFingerprint);
    EXPECT_DOUBLE_EQ(source->GetCellData()->GetArray("p")->GetComponent(0,0),120000);
}
TEST(EngineeringSection, PointFieldProductsUseQuadratureRatherThanProductsOfMeans){
    auto source=box(true);source->SetSpacing(1,1,1);
    for(vtkIdType i=0;i<source->GetNumberOfPoints();++i){double xyz[3];source->GetPoint(i,xyz);const double v=1+xyz[1];source->GetPointData()->GetArray("rho")->SetComponent(i,0,v);source->GetPointData()->GetArray("U")->SetTuple3(i,v,0,0);source->GetPointData()->GetArray("p")->SetComponent(i,0,100000+100*xyz[1]);}
    const auto r=analyzeSection(source,options());
    EXPECT_NEAR(r.area,1,1e-12);
    EXPECT_NEAR(r.massFlow,7./3,1e-12);
    EXPECT_NEAR(r.momentumFlux,15./4,1e-12);
    EXPECT_NEAR(r.kineticPower,31./10,1e-12);
    EXPECT_NEAR(r.meanPressure,100050,1e-8);
}
TEST(EngineeringSection, DensityConventionAnnulusAndInvalidInputsAreExplicit){
    auto source=box();auto o=options();o.useDensityField=false;o.constantDensity=4;o.kinematicPressure=true;const auto original=analyzeSection(source,o);
    EXPECT_NEAR(original.massFlow,240,1e-10);
    EXPECT_NEAR(original.meanPressure,480000,1e-8);
    o.fullAnnulus=true;o.wedgeAngle=5;const auto full=analyzeSection(source,o);
    EXPECT_NEAR(full.massFlow,72*original.massFlow,1e-8);
    EXPECT_NEAR(full.area,72*original.area,1e-8);
    EXPECT_NEAR(full.meanPressure,original.meanPressure,1e-8);
    o.axis=1;
    EXPECT_THROW(analyzeSection(source,o),std::runtime_error);o.axis=0;o.wedgeAngle=0;
    EXPECT_THROW(analyzeSection(source,o),std::runtime_error);
    o.fullAnnulus=false;o.referenceArea=0;
    EXPECT_THROW(analyzeSection(source,o),std::runtime_error);o=options();o.position=100;
    EXPECT_THROW(analyzeSection(source,o),std::runtime_error);
    source->GetCellData()->GetArray("rho")->SetComponent(0,0,-1);
    EXPECT_THROW(analyzeSection(source,options()),std::runtime_error);source->GetCellData()->GetArray("rho")->SetComponent(0,0,2);source->GetCellData()->GetArray("p")->SetComponent(0,0,std::numeric_limits<double>::quiet_NaN());
    EXPECT_THROW(analyzeSection(source,options()),std::runtime_error);
}
TEST(EngineeringSection, AllFluidVolumeBlocksParticipateAndPatchesDoNot){
    vtkNew<vtkMultiBlockDataSet> data;auto a=box(),b=box(false,3);vtkNew<vtkPlaneSource> patch;patch->Update();data->SetBlock(0,patch->GetOutput());data->SetBlock(1,a);data->SetBlock(2,b);
    auto r=analyzeSection(data,options());
    EXPECT_EQ(r.blocks,2);
    EXPECT_NEAR(r.area,12,1e-10);
    EXPECT_NEAR(r.massFlow,240,1e-9);
    auto duplicate=box();data->SetBlock(2,duplicate);r=analyzeSection(data,options());
    EXPECT_NEAR(r.area,6,1e-10);
    EXPECT_GT(r.duplicateTriangles,0);
    auto o=options();o.position=1;
    EXPECT_NEAR(analyzeSection(a,o).area,6,1e-10);
}
TEST(EngineeringSection, CoincidentConflictsAndGhostCellsAreHandledExplicitly){
    vtkNew<vtkMultiBlockDataSet> composite;auto a=box(),b=box();b->GetCellData()->GetArray("p")->SetComponent(0,0,90000);composite->SetBlock(0,a);composite->SetBlock(1,b);
    EXPECT_THROW(analyzeSection(composite,options()),std::runtime_error);
    vtkNew<vtkUnsignedCharArray> ghost;ghost->SetName(vtkDataSetAttributes::GhostArrayName());ghost->SetNumberOfTuples(b->GetNumberOfCells());ghost->FillComponent(0,1);b->GetCellData()->AddArray(ghost);
    EXPECT_NEAR(analyzeSection(composite,options()).massFlow,120,1e-9);
    a->GetCellData()->RemoveArray("rho");
    EXPECT_THROW(analyzeSection(a,options()),std::runtime_error);
    auto constant=options();constant.useDensityField=false;
    EXPECT_NO_THROW(analyzeSection(a,constant));
}

TEST(EngineeringReport, IntegrityEscapingRoundTripAndReferenceComparison){
    auto o=options();const auto r=analyzeSection(box(),o);const auto report=makeEngineeringReport(o,r,{{"project","<script>unsafe</script>"}},"=1+2 <b>variant</b>");
    ASSERT_TRUE(validEngineeringReport(report));
    const auto bytes=QJsonDocument(report).toJson();
    EXPECT_TRUE(validEngineeringReport(QJsonDocument::fromJson(bytes).object()));const auto html=engineeringReportHtml(report);
    EXPECT_FALSE(html.contains("<script>unsafe"));
    EXPECT_TRUE(html.contains("&lt;script&gt;unsafe"));
    EXPECT_TRUE(engineeringReportCsv(report).contains("\"'=1+2"));
    const auto comparison=engineeringComparisonCsv(QJsonArray{report},"massFlow_kg_s",0);
    EXPECT_TRUE(comparison.contains("baseline_sha256"));
    EXPECT_FALSE(comparison.contains("nan"));
    auto modified=report;auto payload=modified.value("payload").toObject();payload.insert("label","changed");modified.insert("payload",payload);
    EXPECT_FALSE(validEngineeringReport(modified));
    EXPECT_THROW(engineeringReportHtml(modified),std::runtime_error);
    EXPECT_TRUE(engineeringComparisonWarning(report,report).isEmpty());o.referenceArea=12;
    EXPECT_FALSE(engineeringComparisonWarning(report,makeEngineeringReport(o,analyzeSection(box(),o),{},"B")).isEmpty());
}
TEST(EngineeringReport, ZeroFlowBaselineExportsAnUndefinedPercentageWithoutInfinity){
    auto data=box();auto* velocity=data->GetCellData()->GetArray("U");for(vtkIdType i=0;i<velocity->GetNumberOfTuples();++i)velocity->SetTuple3(i,0,0,0);
    const auto o=options();const auto report=makeEngineeringReport(o,analyzeSection(data,o),{},"zero flow");const auto csv=engineeringComparisonCsv(QJsonArray{report},"massFlow_kg_s",0);
    EXPECT_TRUE(csv.contains(",massFlow_kg_s,0,,"));EXPECT_FALSE(csv.contains("inf"));
}

TEST(EngineeringWorkbench, EditingInvalidatesCurrentReportButRetainsComparisonSnapshots){
    const auto data=box();EngineeringWorkbench workbench(data,0,{{"project","test"}},{{"options",sectionOptionsJson(options())}});
    workbench.evaluate();
    ASSERT_TRUE(validEngineeringReport(workbench.currentReport()));auto* add=workbench.findChild<QPushButton*>("engineering-add-comparison");
    ASSERT_NE(add,nullptr);add->click();
    EXPECT_EQ(workbench.findChild<QTableWidget*>("engineering-comparison")->rowCount(),1);auto* area=workbench.findChild<QDoubleSpinBox*>("engineering-reference-area");area->setValue(12);
    EXPECT_TRUE(workbench.currentReport().isEmpty());
    EXPECT_FALSE(workbench.findChild<QPushButton*>("engineering-export")->isEnabled());
    workbench.evaluate();add->click();
    EXPECT_EQ(workbench.findChild<QTableWidget*>("engineering-comparison")->rowCount(),2);const auto state=workbench.workspaceState();EngineeringWorkbench reopened(data,0,{},state);
    EXPECT_EQ(reopened.workspaceState(),state);
    EXPECT_TRUE(reopened.currentReport().isEmpty());
    QTranslator translator;
    ASSERT_TRUE(translator.load(QStringLiteral(VORTIA_TEST_QM)));qApp->installTranslator(&translator);EngineeringWorkbench spanish(data,0,{});
    EXPECT_EQ(spanish.windowTitle(),QString::fromUtf8("Análisis de ingeniería"));qApp->removeTranslator(&translator);
}
TEST(Engineering, NativeWorkbenchWhenRequested){
    const auto output=qEnvironmentVariable("VORTIA_ENGINEERING_QA");if(output.isEmpty())GTEST_SKIP()<<"Optional native engineering workbench review";
    ASSERT_TRUE(QDir().mkpath(output));
    QTranslator translator;
    ASSERT_TRUE(translator.load(QStringLiteral(VORTIA_TEST_QM)));qApp->installTranslator(&translator);
    VtkViewport viewport;const auto file=qEnvironmentVariable("VORTIA_ENGINEERING_VTM");if(!file.isEmpty())viewport.loadVtmFile(file.toStdString(),"p");else viewport.showDataSet(box(),"p");
    for(auto theme:{ThemeManager::Theme::Dark,ThemeManager::Theme::Light}){ThemeManager::apply(*qApp,theme);EngineeringWorkbench workbench(viewport.calculatedData(),viewport.sourceWedgeAngle(),{{"project",file.isEmpty()?"Analytical test field":"Stored CFD result"},{"resultFile",file}},{{"options",sectionOptionsJson(options())}});workbench.resize(1120,820);workbench.show();QApplication::processEvents();workbench.evaluate();
    ASSERT_TRUE(validEngineeringReport(workbench.currentReport()));workbench.findChild<QPushButton*>("engineering-add-comparison")->click();workbench.findChild<QLineEdit*>("engineering-name")->setText(QString::fromUtf8("Área de referencia modificada"));workbench.findChild<QDoubleSpinBox*>("engineering-reference-area")->setValue(12);workbench.evaluate();workbench.findChild<QPushButton*>("engineering-add-comparison")->click();
        const auto report=workbench.currentReport();QFile html(output+"/example-report.html");ASSERT_TRUE(html.open(QIODevice::WriteOnly));html.write(engineeringReportHtml(report).toUtf8());QFile json(output+"/example-report.json");ASSERT_TRUE(json.open(QIODevice::WriteOnly));json.write(QJsonDocument(report).toJson(QJsonDocument::Indented));QFile csv(output+"/example-report.csv");ASSERT_TRUE(csv.open(QIODevice::WriteOnly));csv.write(engineeringReportCsv(report));
        auto* tabs=workbench.findChild<QTabWidget*>("engineering-tabs");for(int tab=0;tab<3;++tab){tabs->setCurrentIndex(tab);QApplication::processEvents();
    EXPECT_TRUE(workbench.grab().save(QString("%1/%2-%3.png").arg(output,ThemeManager::toName(theme)).arg(tab)));}}
    qApp->removeTranslator(&translator);
}
