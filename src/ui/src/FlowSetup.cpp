#include "vortia/ui/FlowSetup.hpp"
#include <QCoreApplication>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QProcess>
#include <QProcessEnvironment>
#include <QProgressDialog>
#include <QEventLoop>
#include <QStandardPaths>
#include <QTabWidget>
#include <QTableWidget>
#include <QVBoxLayout>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QSaveFile>
#include "vortia/solver/openfoam/Detector.hpp"
#include <QUuid>
#include <cmath>
#include <algorithm>
#include <map>

namespace vortia {
namespace {
QString tr(const char* s) { return QCoreApplication::translate("vortia::FlowSetup", s); }
}

QJsonObject editFlowSetup(QWidget* parent, const Sketch2D& shape, const QJsonObject& previous, int page) {
    QDialog dialog(parent);
    dialog.setObjectName("vortia-flow-setup");
    dialog.setWindowTitle(tr("Flow configuration"));
    dialog.resize(1050, 720);
    auto* layout = new QVBoxLayout(&dialog);
    auto* intro = new QLabel(tr("Configure the fluid domain drawn in the sketch. The horizontal X axis is the rotation axis; Y is the radius. Boundary conditions belong to the numbered sketch segments."), &dialog);
    intro->setWordWrap(true); layout->addWidget(intro);
    auto* tabs = new QTabWidget(&dialog); layout->addWidget(tabs);
    struct Number { QString section, key; QDoubleSpinBox* spin; };
    std::vector<Number> numbers;
    auto addPage = [&](const char* title) {
        auto* widget = new QWidget(tabs); auto* form = new QFormLayout(widget);
        tabs->addTab(widget, tr(title)); return form;
    };
    auto number = [&](QFormLayout* form, const char* section, const char* key, const char* label,
                      double fallback, double low, double high, int decimals) {
        auto* spin = new QDoubleSpinBox(&dialog);
        spin->setObjectName(QString("flow-%1-%2").arg(section,key));
        spin->setDecimals(decimals); spin->setRange(low,high);
        spin->setValue(previous.value(section).toObject().value(key).toDouble(fallback));
        form->addRow(tr(label),spin); numbers.push_back({section,key,spin}); return spin;
    };
    auto* mesh = addPage("Mesh");
    number(mesh,"mesh","sizeMm","Global cell size (mm)",20,0.001,1e7,3);
    number(mesh,"mesh","wallSizeMm","Cell size near walls (mm)",2,0.001,1e7,3);
    number(mesh,"mesh","wedgeAngle","Axisymmetric wedge angle (degrees)",5,0.1,5,2);
    auto* meshInfo = new QLabel(tr("Generate a new mesh from the actual lines and circular arcs of the sketch. One closed fluid region above Y=0 is supported. Existing mesh and result revisions are retained."),&dialog);
    meshInfo->setWordWrap(true); mesh->addRow(meshInfo);
    auto* material = addPage("Models and material");
    material->addRow(new QLabel(tr("Ideal gas · energy enabled · axisymmetric without swirl · adiabatic no-slip walls"), &dialog));
    auto* turbulence = new QComboBox(&dialog); turbulence->setObjectName("flow-turbulence");
    turbulence->addItem(tr("Spalart–Allmaras"),"SpalartAllmaras"); turbulence->addItem(tr("Laminar"),"laminar");
    turbulence->setCurrentIndex(std::max(0,turbulence->findData(previous.value("material").toObject().value("turbulence").toString("SpalartAllmaras"))));
    material->addRow(tr("Turbulence model"),turbulence);
    number(material,"material","molecularWeight","Molecular weight (kg/kmol)",28.966,0.01,10000,5);
    number(material,"material","Cp","Specific heat Cp (J/kg/K)",1005,0.01,1e7,3);
    number(material,"material","mu","Dynamic viscosity (Pa s)",1.8e-5,1e-12,100,10);
    number(material,"material","conductivity","Thermal conductivity (W/m/K)",0.025,1e-9,10000,7);
    auto* boundaryPage = new QWidget(tabs); auto* boundaryLayout = new QVBoxLayout(boundaryPage);
    auto* hint = new QLabel(tr("Pressures are absolute Pa. Inlet pressure and temperature are total values. Outlet pressure is static; its temperature applies to backflow. Axis segments must lie on Y=0. Reuse a region name only when all its settings match."),boundaryPage);
    hint->setWordWrap(true); boundaryLayout->addWidget(hint);
    auto* table = new QTableWidget(static_cast<int>(shape.segmentCount()),6,boundaryPage);
    table->setObjectName("flow-boundaries");
    table->setHorizontalHeaderLabels({tr("Segment"),tr("Region name"),tr("Type"),tr("Pressure (Pa)"),tr("Temperature (K)"),tr("Viscosity ratio")});
    table->horizontalHeader()->setSectionResizeMode(QHeaderView::ResizeToContents);
    table->setMinimumHeight(280); boundaryLayout->addWidget(table);
    const auto oldBoundaries = previous.value("boundaries").toArray();
    for (int row=0;row<table->rowCount();++row) {
        const auto index=static_cast<std::size_t>(row);
        const auto a=shape.points()[index], b=shape.points()[(index+1)%shape.points().size()];
        const bool axis=a.yMm==0 && b.yMm==0 && shape.segmentBulge(index)==0;
        const auto old=oldBoundaries.size()==table->rowCount() ? oldBoundaries[row].toObject() : QJsonObject{};
        auto* segment=new QTableWidgetItem(QString("%1 · %2 mm").arg(row+1).arg(shape.segmentLength(index),0,'g',6));
        segment->setFlags(segment->flags()&~Qt::ItemIsEditable);table->setItem(row,0,segment);
        table->setCellWidget(row,1,new QLineEdit(old.value("name").toString(axis?QStringLiteral("axis"):QString("edge_%1").arg(row+1)),table));
        auto* type=new QComboBox(table);
        for (const auto& item:std::vector<std::pair<QString,QString>>{{tr("Wall"),"wall"},{tr("Axis"),"axis"},{tr("Total-pressure inlet"),"inlet"},{tr("Pressure outlet"),"outlet"},{tr("Open reservoir"),"reservoir"}})type->addItem(item.first,item.second);
        type->setCurrentIndex(std::max(0,type->findData(old.value("type").toString(axis?"axis":"wall"))));table->setCellWidget(row,2,type);
        for (int col=3;col<6;++col) {
            const char* key=col==3?"pressure":col==4?"temperature":"viscosityRatio";
            auto* value=new QDoubleSpinBox(table);value->setDecimals(col==3?1:3);value->setRange(0.001,1e9);
            if(col==5)value->setMaximum(100000);
            value->setValue(old.value(key).toDouble(col==3?101325:col==4?300:10));table->setCellWidget(row,col,value);
        }
        auto updateBoundaryEditors=[table,row,type]() {
            const bool open=type->currentData().toString()!="wall" && type->currentData().toString()!="axis";
            for(int col=3;col<6;++col)table->cellWidget(row,col)->setEnabled(open);
        };
        QObject::connect(type,qOverload<int>(&QComboBox::currentIndexChanged),&dialog,updateBoundaryEditors);
        updateBoundaryEditors();
        segment->setToolTip(QString("(%1, %2) → (%3, %4) mm").arg(a.xMm).arg(a.yMm).arg(b.xMm).arg(b.yMm));
    }
    tabs->addTab(boundaryPage,tr("Boundary conditions"));
    auto* calculation=addPage("Initialization and calculation");
    number(calculation,"initial","pressure","Initial absolute pressure (Pa)",101325,0.001,1e9,1);
    number(calculation,"initial","temperature","Initial temperature (K)",300,0.001,100000,2);
    number(calculation,"solver","iterations","Maximum pseudo-iterations",1000,1,200000,0);
    number(calculation,"solver","writeInterval","Save every N iterations",100,1,200000,0);
    number(calculation,"solver","maxCo","Acoustic Courant limit",0.1,0.001,0.5,3);
    auto* method = new QLabel(tr("Experimental compressible OpenFOAM v2606 backend. Kurganov/Minmod local time stepping seeks a steady flow. The iteration limit does not establish convergence; mass balance, field evolution and mesh dependence must be checked."),&dialog);
    method->setWordWrap(true); calculation->addRow(method);
    tabs->setCurrentIndex(std::clamp(page,0,tabs->count()-1));
    auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);layout->addWidget(buttons);
    QObject::connect(buttons,&QDialogButtonBox::accepted,&dialog,&QDialog::accept);
    QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    if(dialog.exec()!=QDialog::Accepted)return {};
    QJsonObject result;
    for(const auto& item:numbers){auto object=result.value(item.section).toObject();object.insert(item.key,item.spin->value());result.insert(item.section,object);}
    auto mat=result.value("material").toObject();mat.insert("turbulence",turbulence->currentData().toString());result.insert("material",mat);
    QJsonArray boundaries;
    for(int row=0;row<table->rowCount();++row){QJsonObject bc;
        bc.insert("name",static_cast<QLineEdit*>(table->cellWidget(row,1))->text().trimmed());
        bc.insert("type",static_cast<QComboBox*>(table->cellWidget(row,2))->currentData().toString());
        for(int col=3;col<6;++col)bc.insert(col==3?"pressure":col==4?"temperature":"viscosityRatio",static_cast<QDoubleSpinBox*>(table->cellWidget(row,col))->value());
        boundaries.append(bc);
    }
    result.insert("boundaries",boundaries);return result;
}

QString updateFlowRunControls(const QString& caseDir,const QJsonObject& solver,
        const std::vector<std::pair<std::string,std::string>>& environment) {
    const double iterations=solver.value("iterations").toDouble(-1);
    const double interval=solver.value("writeInterval").toDouble(-1);
    const double courant=solver.value("maxCo").toDouble(-1);
    if(!std::isfinite(iterations)||!std::isfinite(interval)||!std::isfinite(courant)||
       iterations<1||iterations>200000||std::floor(iterations)!=iterations||
       interval<1||interval>iterations||std::floor(interval)!=interval||courant<=0||courant>.5)
        return tr("Invalid iteration limit, output interval or Courant number.");
    QFile marker(caseDir+"/.vortia-flow-case");
    if(!marker.open(QIODevice::ReadOnly)||marker.readAll().trimmed()!="vortia-axisymmetric-flow-v1")
        return tr("Only a generated flow case can be resumed with these controls.");
    QFile original(caseDir+"/system/controlDict");
    if(!original.open(QIODevice::ReadOnly))return original.errorString();
    const auto bytes=original.readAll();original.close();
    QTemporaryFile staged(caseDir+"/system/.vortia-controls-XXXXXX");
    if(!staged.open()||staged.write(bytes)!=bytes.size()||!staged.flush())return staged.errorString();
    staged.close();
    const auto executable=openfoam::Detector::resolveExecutable("foamDictionary",environment);
    if(!executable)return tr("OpenFOAM foamDictionary is required to update calculation controls.");
    QProcessEnvironment captured;
    for(const auto& [key,value]:environment)captured.insert(QString::fromStdString(key),QString::fromStdString(value));
    for(const auto& entry:std::vector<std::pair<QString,double>>{{"endTime",iterations},{"writeInterval",interval},{"maxCo",courant}}) {
        QProcess command;command.setProcessEnvironment(captured);command.setWorkingDirectory(caseDir);
        command.start(QString::fromStdString(executable->string()),
            {staged.fileName(),"-entry",entry.first,"-set",QString::number(entry.second,'g',15)});
        if(!command.waitForFinished(10000)||command.exitStatus()!=QProcess::NormalExit||command.exitCode()!=0) {
            command.kill();command.waitForFinished(1000);
            return tr("Calculation controls could not be updated; the existing case was preserved.")+"\n"+QString::fromUtf8(command.readAllStandardError());
        }
    }
    if(!staged.open())return staged.errorString();
    const auto updated=staged.readAll();
    if(updated.isEmpty())return tr("The updated calculation dictionary is empty.");
    QSaveFile output(caseDir+"/system/controlDict");
    if(!output.open(QIODevice::WriteOnly)||output.write(updated)!=updated.size()||!output.commit())return output.errorString();
    return {};
}

QString generateFlowCase(QWidget* parent,const QString& sketchPath,const QJsonObject& cfg,
                         const QString& projectRoot,const std::vector<std::pair<std::string,std::string>>& environment){
    QString parentDir=QDir(projectRoot).filePath("cases");
#ifdef Q_OS_MACOS
    // Keep solver writes/builds off iCloud-managed Documents/Desktop folders.
    // The small editable project retains the absolute reference to its results.
    parentDir=QDir::homePath()+"/Library/Vortia/cases";
#endif
    if(std::any_of(parentDir.cbegin(),parentDir.cend(),[](QChar c){return c.isSpace();}))
        parentDir=QDir::homePath()+"/Library/Vortia/cases";
    if(!QDir().mkpath(parentDir))return {};
    QTemporaryDir stage(parentDir+"/.build-XXXXXX");if(!stage.isValid())return {};
    auto write=[&](const QString& name,const QByteArray& bytes){QFile f(stage.filePath(name));return f.open(QIODevice::WriteOnly)&&f.write(bytes)==bytes.size();};
    for(const auto& name:{"prepare.py","boundaries.py"}){QFile f(QString(":/vortia/flow/")+name);if(!f.open(QIODevice::ReadOnly)||!write(name,f.readAll()))return {};}
    auto settings=cfg;settings.remove("activeCase");settings.remove("preparedHash");
    if(!write("settings.json",QJsonDocument(settings).toJson())||!QDir().mkpath(stage.filePath("solver/Make")))return {};
    for(const auto& name:QStringList{"vortiaCentralFoam.C","centralCourantNo.H","createFieldRefs.H","createFields.H","directionInterpolate.H","readFluxScheme.H","setRDeltaT.H","fixedRhoFvPatchScalarField.H","Make/files","Make/options","README.md","LICENSE.md"}){
        QFile f(QString(":/vortia/openfoam/solver/")+name);if(!f.open(QIODevice::ReadOnly)||!write("solver/"+name,f.readAll()))return {};
    }
    QString python;
    for(const auto& candidate:QStringList{"/opt/homebrew/bin/python3","/usr/local/bin/python3",QStandardPaths::findExecutable("python3")})
        if(QFileInfo(candidate).isExecutable()){python=candidate;break;}
    const QString runtime=qEnvironmentVariable("VORTIA_GMSH_RUNTIME",QCoreApplication::applicationDirPath()+"/../Resources/gmsh");
    if(python.isEmpty()||!QFileInfo::exists(runtime+"/gmsh.py")){
        QMessageBox::warning(parent,tr("Mesher unavailable"),tr("This installation is missing Python 3 or the bundled Gmsh runtime."));return {};
    }
    QProcess process;QProcessEnvironment env=QProcessEnvironment::systemEnvironment();
    for(const auto& [key,value]:environment)env.insert(QString::fromStdString(key),QString::fromStdString(value));
    process.setProcessEnvironment(env);process.setProcessChannelMode(QProcess::MergedChannels);
    QProgressDialog progress(tr("Generating the mesh from the sketch…"),tr("Cancel"),0,0,parent);progress.setWindowModality(Qt::WindowModal);progress.setMinimumDuration(0);
    QEventLoop loop;bool finished=false;QObject::connect(&process,qOverload<int,QProcess::ExitStatus>(&QProcess::finished),&loop,[&](){finished=true;loop.quit();});
    QObject::connect(&process,&QProcess::errorOccurred,&loop,&QEventLoop::quit);
    QObject::connect(&progress,&QProgressDialog::canceled,&process,&QProcess::kill);
    process.start(python,{stage.filePath("prepare.py"),"--sketch",sketchPath,"--config",stage.filePath("settings.json"),"--case",stage.filePath("case"),"--solver-sources",stage.filePath("solver"),"--gmsh-runtime",runtime});
    loop.exec();progress.close();
    if(process.state()!=QProcess::NotRunning){process.kill();process.waitForFinished(3000);}
    const auto output=process.readAll();
    if(!finished||process.exitStatus()!=QProcess::NormalExit||process.exitCode()!=0){
        QByteArray diagnostic=output;
        QFile converter(stage.filePath("case/log.gmshToFoam"));if(converter.open(QIODevice::ReadOnly))diagnostic+="\n"+converter.readAll();
        QDir().mkpath(QDir(projectRoot).filePath("logs"));
        QFile saved(QDir(projectRoot).filePath("logs/mesh-generation.log"));if(saved.open(QIODevice::WriteOnly))saved.write(diagnostic);
        QMessageBox failure(QMessageBox::Warning,tr("Mesh generation failed"),tr("Check the contour, boundary assignments and mesh sizes. Details are saved in the project's logs folder."),QMessageBox::Ok,parent);
        failure.setDetailedText(QString::fromUtf8(diagnostic).right(30000));failure.exec();return {};
    }
    QFile log(stage.filePath("case/log.prepare"));if(log.open(QIODevice::WriteOnly))log.write(output);
    const QString target=QDir(parentDir).filePath("run_"+QUuid::createUuid().toString(QUuid::WithoutBraces));
    if(!QDir().rename(stage.filePath("case"),target))return {};
    return target;
}
}
