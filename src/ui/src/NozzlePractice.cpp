#include "vortia/ui/NozzlePractice.hpp"
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QSpinBox>
#include <QFormLayout>
#include <QVBoxLayout>
#include <QLabel>
#include <QTabWidget>
#include <QScrollArea>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QFileDialog>
#include <QInputDialog>
#include <QLineEdit>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMessageBox>
#include <QProcess>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <vector>
#include <algorithm>

namespace vortia {
namespace {
QString tr(const char* s) { return QCoreApplication::translate("vortia::NozzlePractice",s); }
struct Parameter { const char* key; const char* label; double factor; int decimals; };
}

QString createNozzlePractice(QWidget* parent) {
    QFile ref(QStringLiteral(":/vortia/nozzle-video/reference.json"));
    if (!ref.open(QIODevice::ReadOnly)) return {};
    const auto reference = ref.readAll();
    auto values = QJsonDocument::fromJson(reference).object().value("values").toObject();
    QDialog dialog(parent);
    dialog.setWindowTitle(tr("Nozzle tutorial — geometry and compressible flow"));
    dialog.resize(680,740);
    auto* layout = new QVBoxLayout(&dialog);
    auto* intro = new QLabel(tr("Prepare the tutorial nozzle, exterior domain and an axisymmetric OpenFOAM case. "
        "The displayed fillet radii are retained while solving the final dimensions. "
        "The original CAD was not provided. A new case is created for every preparation."),&dialog);
    intro->setWordWrap(true); layout->addWidget(intro);
    auto* tabs = new QTabWidget(&dialog); layout->addWidget(tabs,1);
    std::vector<std::pair<Parameter,QDoubleSpinBox*>> editors;
    const auto page = [&](const QString& title, std::initializer_list<Parameter> parameters) {
        auto* content = new QWidget;
        auto* form = new QFormLayout(content);
        for (const auto& p : parameters) {
            auto* spin = new QDoubleSpinBox(content);
            spin->setObjectName(QString::fromUtf8(p.key));
            spin->setDecimals(p.decimals); spin->setRange(0.000000001,1e9);
            if (QString::fromUtf8(p.key).contains(QStringLiteral("Gauge"))) spin->setMinimum(-1e9);
            if (QString::fromUtf8(p.key)==QStringLiteral("operatingPressure")) spin->setMinimum(0);
            spin->setValue(values.value(p.key).toDouble()*p.factor);
            form->addRow(tr(p.label),spin); editors.emplace_back(p,spin);
        }
        auto* scroll = new QScrollArea; scroll->setWidgetResizable(true); scroll->setWidget(content);
        tabs->addTab(scroll,title);
        return form;
    };
    page(tr("Geometry"),{
        {"inletHeight","Inlet height (mm)",1000,3},
        {"throatHeight","Throat height after rounding (mm)",1000,3},
        {"outletHeight","Nozzle exit height (mm)",1000,3},
        {"inletPlateau","Final inlet plateau (mm)",1000,3},
        {"shoulderRadius","Shoulder fillet radius (mm)",1000,3},
        {"throatRadius","Throat fillet radius (mm)",1000,3},
        {"straightDivergingLength","Straight divergent horizontal length (mm)",1000,3},
        {"nozzleExitX","Nozzle exit X (mm)",1000,3},
        {"exteriorLength","Exterior length from nozzle exit (mm)",1000,3},
        {"exteriorHeightAboveExit","Exterior height above nozzle exit (mm)",1000,3}
    });
    auto* physics = page(tr("Physics and calculation"),{
        {"operatingPressure","Operating pressure (Pa)",1,1},
        {"inletGaugeTotalPressure","Inlet total gauge pressure (Pa)",1,1},
        {"exteriorGaugePressure","Exterior gauge pressure (Pa)",1,1},
        {"inletTotalTemperature","Inlet total temperature (K)",1,2},
        {"exteriorTotalTemperature","Exterior total temperature (K)",1,2},
        {"Cp","Specific heat Cp (J/kg/K)",1,3},
        {"molecularWeight","Molecular weight (kg/kmol)",1,4},
        {"mu","Dynamic viscosity (Pa s)",1,9},
        {"conductivity","Thermal conductivity (W/m/K)",1,6},
        {"turbulentViscosityRatio","Turbulent viscosity ratio",1,3}
    });
    auto* refinement = new QSpinBox(&dialog); refinement->setRange(1,3); refinement->setValue(1);
    physics->addRow(tr("Mesh refinement (1 / 2 / 3)"),refinement);
    auto* iterations = new QSpinBox(&dialog); iterations->setRange(100,200000); iterations->setValue(50000);
    physics->addRow(tr("Maximum pseudo-iterations"),iterations);
    auto* method = new QLabel(tr("Gas: ideal · energy: enabled · turbulence: Spalart–Allmaras. "
        "Density-based local time stepping seeks a steady solution. "
        "A wedge mesh replaces the tutorial's triangular mesh. Convergence must be checked after calculation."),&dialog);
    method->setWordWrap(true); layout->addWidget(method);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);
    layout->addWidget(buttons);
    QObject::connect(buttons,&QDialogButtonBox::accepted,&dialog,&QDialog::accept);
    QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    if (dialog.exec()!=QDialog::Accepted) return {};
    for (const auto& [p,spin] : editors) values.insert(p.key,spin->value()/p.factor);
    const QString parentDir=QFileDialog::getExistingDirectory(parent,tr("Choose a parent folder for the new case"));
    if (parentDir.isEmpty()) return {};
    const QString name=QInputDialog::getText(parent,tr("New nozzle case"),tr("Case name:"),QLineEdit::Normal,QStringLiteral("tobera-video"));
    if (name.isEmpty()) return {};
    if (name==QStringLiteral(".") || name==QStringLiteral("..") || name.contains('/') || name.contains('\\')) {
        QMessageBox::warning(parent,tr("Preparation failed"),tr("Use a case name without path separators.")); return {};
    }
    const QString target=QDir(parentDir).filePath(name);
    if (std::any_of(target.cbegin(),target.cend(),[](QChar c) { return c.isSpace(); })) {
        QMessageBox::warning(parent,tr("Preparation failed"),
            tr("Choose a parent folder and case name without spaces. OpenFOAM's build tools require this.")); return {};
    }
    if (QFileInfo::exists(target)) {
        QMessageBox::warning(parent,tr("Preparation failed"),tr("That case already exists. Choose a new name to preserve it.")); return {};
    }
    const QString python=QStandardPaths::findExecutable(QStringLiteral("python3"));
    if (python.isEmpty()) {
        QMessageBox::warning(parent,tr("Preparation failed"),tr("Python 3 is required to prepare this case.")); return {};
    }
    QTemporaryDir stage(QDir(parentDir).filePath(QStringLiteral(".vortia-nozzle-XXXXXX")));
    if (!stage.isValid()) return {};
    const auto write=[&](const QString& name,const QByteArray& data) {
        QFile f(stage.filePath(name)); return f.open(QIODevice::WriteOnly) && f.write(data)==data.size();
    };
    QFile script(QStringLiteral(":/vortia/nozzle-video/prepare.py"));
    if (!script.open(QIODevice::ReadOnly) || !write("prepare.py",script.readAll()) || !write("reference.json",reference)
        || !write("parameters.json",QJsonDocument(values).toJson())) return {};
    if (!QDir().mkpath(stage.filePath(QStringLiteral("solver/Make")))) return {};
    const QStringList sources={"vortiaCentralFoam.C","centralCourantNo.H","createFieldRefs.H",
        "createFields.H","directionInterpolate.H","readFluxScheme.H","setRDeltaT.H",
        "fixedRhoFvPatchScalarField.H","Make/files","Make/options","README.md","LICENSE.md"};
    for (const auto& name : sources) {
        QFile source(QStringLiteral(":/vortia/nozzle-video/solver/")+name);
        if (!source.open(QIODevice::ReadOnly) || !write(QStringLiteral("solver/")+name,source.readAll())) return {};
    }
    QProcess process;
    process.start(python,{stage.filePath("prepare.py"),stage.filePath("case"),"--parameters",stage.filePath("parameters.json"),
        "--refinement",QString::number(refinement->value()),"--end-time",QString::number(iterations->value())});
    const bool finished=process.waitForFinished(30000);
    if (!finished || process.exitStatus()!=QProcess::NormalExit || process.exitCode()!=0) {
        process.kill(); process.waitForFinished();
        QString reason=QString::fromUtf8(process.readAllStandardError());
        if (reason.isEmpty()) reason=process.errorString();
        QMessageBox::warning(parent,tr("Preparation failed"),reason); return {};
    }
    if (!QDir().rename(stage.filePath("case"),target)) {
        QMessageBox::warning(parent,tr("Preparation failed"),tr("The prepared case could not be moved into its destination.")); return {};
    }
    return target;
}
}
