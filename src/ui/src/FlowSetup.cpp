#include "vortia/ui/FlowSetup.hpp"
#include "vortia/ui/SettingsNavigation.hpp"
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
#include <QStackedWidget>
#include <QListWidget>
#include <QScrollArea>
#include <QCheckBox>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QPushButton>
#include <QTableWidget>
#include <QVBoxLayout>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QSaveFile>
#include <QCryptographicHash>
#include <QThread>
#include "vortia/solver/openfoam/Detector.hpp"
#include <QUuid>
#include <cmath>
#include <algorithm>
#include <map>
#include <tuple>

namespace vortia {
namespace {
QString tr(const char* s) { return QCoreApplication::translate("vortia::FlowSetup", s); }
}

QJsonObject editFlowSetup(QWidget* parent, const Sketch2D& shape, const QJsonObject& previous, int page, const QJsonObject& defaults) {
    QDialog dialog(parent);
    dialog.setObjectName("vortia-flow-setup");
    dialog.setWindowTitle(tr("Flow configuration"));
    dialog.resize(1120, 800);
    dialog.setStyleSheet(QStringLiteral(
        "QListWidget#flow-navigation { border: 0; background: transparent; font-size: 14px; }"
        "QListWidget#flow-navigation::item { padding: 16px 10px; border-radius: 9px; margin-bottom: 6px; }"
        "QListWidget#flow-navigation::item:selected { background: #246b8e; color: white; }"
        "QLabel#flow-title { font-size: 25px; font-weight: 600; }"
        "QGroupBox { border: 1px solid palette(mid); border-radius: 10px; margin-top: 12px; padding: 16px; }"
        "QGroupBox::title { subcontrol-origin: margin; left: 14px; padding: 0 6px; }"
        "QDoubleSpinBox, QComboBox, QLineEdit { min-height: 28px; }"));
    auto* layout = new QVBoxLayout(&dialog);
    auto* title = new QLabel(tr("Set up your flow"),&dialog);
    title->setObjectName("flow-title");layout->addWidget(title);
    auto* intro = new QLabel(tr("Configure the fluid domain drawn in the sketch. The horizontal X axis is the rotation axis; Y is the radius. Boundary conditions belong to the numbered sketch segments."), &dialog);
    intro->setWordWrap(true); layout->addWidget(intro);
    auto* body=new QHBoxLayout;layout->addLayout(body,1);
    auto* pages=new QStackedWidget(&dialog);pages->setObjectName("flow-pages");
    auto* navigation=new SettingsNavigation(&dialog,pages,"flow-navigation");body->addWidget(navigation);body->addWidget(pages,1);
    auto registerPage=[&](QWidget* widget,const char* text,const char* icon) {
        auto* scroll=new QScrollArea(pages);scroll->setWidgetResizable(true);scroll->setFrameShape(QFrame::NoFrame);
        scroll->setWidget(widget);pages->addWidget(scroll);
        const QStringList labels{tr("Mesh"),tr("Models"),tr("Boundaries"),tr("Calculation")};
        navigation->addPage(labels.at(pages->count()-1),QString(":/vortia/icons/%1.svg").arg(icon),tr(text));
    };
    struct Number { QString section, key; QDoubleSpinBox* spin; };
    std::vector<Number> numbers;
    auto addPage = [&](const char* text,const char* icon) {
        auto* widget=new QWidget(pages);auto* pageLayout=new QVBoxLayout(widget);
        auto* card=new QGroupBox(tr(text),widget);pageLayout->addWidget(card);pageLayout->addStretch();
        auto* form=new QFormLayout(card);form->setSpacing(16);form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
        registerPage(widget,text,icon);return form;
    };
    auto number = [&](QFormLayout* form, const char* section, const char* key, const char* label,
                      double fallback, double low, double high, int decimals) {
        auto* spin = new QDoubleSpinBox(&dialog);
        spin->setObjectName(QString("flow-%1-%2").arg(section,key));
        spin->setDecimals(decimals); spin->setRange(low,high);
        spin->setValue(previous.value(section).toObject().value(key).toDouble(defaults.value(section).toObject().value(key).toDouble(fallback)));
        form->addRow(tr(label),spin); numbers.push_back({section,key,spin}); return spin;
    };
    auto* mesh = addPage("Mesh","mesh");
    number(mesh,"mesh","sizeMm","Global cell size (mm)",20,0.001,1e7,3);
    number(mesh,"mesh","wallSizeMm","Cell size near walls (mm)",2,0.001,1e7,3);
    number(mesh,"mesh","wedgeAngle","Axisymmetric wedge angle (degrees)",5,0.1,5,2);
    auto* meshInfo = new QLabel(tr("Generate a new mesh from the actual lines and circular arcs of the sketch. One closed fluid region above Y=0 is supported. Existing mesh and result revisions are retained."),&dialog);
    meshInfo->setWordWrap(true); mesh->addRow(meshInfo);
    auto* material = addPage("Models and material","material");
    material->addRow(new QLabel(tr("Ideal gas · energy enabled · axisymmetric without swirl · adiabatic walls"), &dialog));
    auto* turbulence = new QComboBox(&dialog); turbulence->setObjectName("flow-turbulence");
    turbulence->addItem(tr("Spalart–Allmaras"),"SpalartAllmaras");turbulence->addItem(tr("Laminar"),"laminar");
    turbulence->addItem(tr("Inviscid (Euler · slip walls)"),"inviscid");
    turbulence->setCurrentIndex(std::max(0,turbulence->findData(previous.value("material").toObject().value("turbulence").toString("SpalartAllmaras"))));
    material->addRow(tr("Flow model"),turbulence);
    number(material,"material","molecularWeight","Molecular weight (kg/kmol)",28.966,0.01,10000,5);
    number(material,"material","Cp","Specific heat Cp (J/kg/K)",1005,0.01,1e7,3);
    auto* viscosity=number(material,"material","mu","Dynamic viscosity (Pa s)",1.8e-5,1e-12,100,10);
    auto* conductivity=number(material,"material","conductivity","Thermal conductivity (W/m/K)",0.025,1e-9,10000,7);
    auto* modelInfo=new QLabel(tr("Inviscid solves Euler flow with slip walls and zero viscous or thermal diffusion. Laminar and Spalart–Allmaras retain viscosity and no-slip walls. Transport inputs are retained but inactive in inviscid mode."),&dialog);
    modelInfo->setWordWrap(true);material->addRow(modelInfo);
    auto transportState=[=](){const bool viscous=turbulence->currentData().toString()!="inviscid";viscosity->setEnabled(viscous);conductivity->setEnabled(viscous);};
    QObject::connect(turbulence,qOverload<int>(&QComboBox::currentIndexChanged),&dialog,transportState);transportState();
    auto* boundaryPage = new QWidget(pages); auto* boundaryLayout = new QVBoxLayout(boundaryPage);
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
    registerPage(boundaryPage,"Boundary conditions","boundary");
    auto* calculation=addPage("Initialization and calculation","solver");
    number(calculation,"initial","pressure","Initial absolute pressure (Pa)",101325,0.001,1e9,1);
    number(calculation,"initial","temperature","Initial temperature (K)",300,0.001,100000,2);
    const auto convergence=previous.value("solver").toObject().value("convergence").toObject();
    auto* automatic=new QCheckBox(tr("Stop automatically when the flow stabilizes"),&dialog);
    automatic->setObjectName("flow-convergence-enabled");automatic->setChecked(convergence.value("enabled").toBool(true));
    calculation->addRow(automatic);
    auto* automaticInfo=new QLabel(tr("Vortia checks global and local mass conservation, pressure, temperature, density, velocity and turbulence stability. It saves and stops after consecutive successful checks. Reaching the safety limit is reported as not converged."),&dialog);
    automaticInfo->setWordWrap(true);calculation->addRow(automaticInfo);
    number(calculation,"solver","iterations","Safety iteration limit",50000,1,200000,0);
    number(calculation,"solver","writeInterval","Save every N iterations",100,1,200000,0);
    number(calculation,"solver","maxCo","Acoustic Courant limit",0.1,0.001,0.5,3);
    number(calculation,"solver","logInterval","Diagnostic output every N iterations",10,1,1000,0);
    auto* cpu=number(calculation,"solver","processes","CPU processes (1 = serial)",0,0,8,0);
    cpu->setSpecialValueText(tr("Automatic (mesh size)"));
    auto* cpuInfo=new QLabel(tr("Parallel runs use an isolated working folder and retain recovery files. The latest complete checkpoint is reconstructed into this case. Small meshes may run faster with fewer processes."),&dialog);
    cpuInfo->setWordWrap(true);calculation->addRow(cpuInfo);
    auto* method = new QLabel(tr("Experimental compressible OpenFOAM v2606 backend. Automatic checks assess numerical convergence on this mesh. Model suitability and mesh independence require separate validation."),&dialog);
    method->setWordWrap(true); calculation->addRow(method);
    auto* advanced=new QGroupBox(tr("Advanced convergence criteria"),&dialog);
    advanced->setObjectName("flow-convergence-advanced");
    auto* advancedLayout=new QFormLayout(advanced);
    struct Criterion {const char* key;QDoubleSpinBox* spin;};std::vector<Criterion> criteria;
    auto criterion=[&](const char* key,const char* text,double fallback,double low,double high,int decimals) {
        auto* spin=new QDoubleSpinBox(advanced);spin->setObjectName(QString("flow-convergence-%1").arg(key));
        spin->setDecimals(decimals);spin->setRange(low,high);spin->setValue(convergence.value(key).toDouble(fallback));
        advancedLayout->addRow(tr(text),spin);criteria.push_back({key,spin});
    };
    criterion("interval","Check every N iterations",50,1,10000,0);
    criterion("minimumIterations","Minimum iterations before acceptance",500,1,200000,0);
    criterion("stableSamples","Consecutive successful checks",5,2,100,0);
    criterion("massTolerance","Relative mass error limit",.001,.00000001,.01,8);
    criterion("fieldTolerance","Relative field change limit",.001,.00000001,.01,8);
    criterion("equationTolerance","Normalized update rate limit",1e-5,1e-8,1e-3,8);
    criterion("absoluteMassTolerance","Absolute mass floor (kg/s)",1e-9,1e-15,1e-6,15);
    auto* showAdvanced=new QCheckBox(tr("Show advanced criteria"),&dialog);
    calculation->addRow(showAdvanced);calculation->addRow(advanced);advanced->setVisible(false);
    QObject::connect(showAdvanced,&QCheckBox::toggled,advanced,&QWidget::setVisible);
    navigation->selectPage(std::clamp(page,0,pages->count()-1));
    auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);layout->addWidget(buttons);
    QObject::connect(buttons,&QDialogButtonBox::accepted,&dialog,[&](){
        if(criteria[1].spin->value()<criteria[0].spin->value()) {
            QMessageBox::warning(&dialog,tr("Convergence criteria"),tr("The minimum iterations must be at least the check interval."));return;
        }
        dialog.accept();
    });
    QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    if(dialog.exec()!=QDialog::Accepted)return {};
    QJsonObject result;
    for(const auto& item:numbers){auto object=result.value(item.section).toObject();object.insert(item.key,item.spin->value());result.insert(item.section,object);}
    auto mat=result.value("material").toObject();mat.insert("turbulence",turbulence->currentData().toString());result.insert("material",mat);
    QJsonObject convergenceResult;convergenceResult.insert("enabled",automatic->isChecked());
    for(const auto& c:criteria)convergenceResult.insert(c.key,c.spin->value());
    auto solverResult=result.value("solver").toObject();solverResult.insert("convergence",convergenceResult);result.insert("solver",solverResult);
    QJsonArray boundaries;
    for(int row=0;row<table->rowCount();++row){QJsonObject bc;
        bc.insert("name",static_cast<QLineEdit*>(table->cellWidget(row,1))->text().trimmed());
        bc.insert("type",static_cast<QComboBox*>(table->cellWidget(row,2))->currentData().toString());
        for(int col=3;col<6;++col)bc.insert(col==3?"pressure":col==4?"temperature":"viscosityRatio",static_cast<QDoubleSpinBox*>(table->cellWidget(row,col))->value());
        boundaries.append(bc);
    }
    result.insert("boundaries",boundaries);return result;
}

QString stageFlowBackend(const QString& caseDir) {
    QFile marker(caseDir+"/.vortia-flow-case");
    if(!marker.open(QIODevice::ReadOnly)||marker.readAll().trimmed()!="vortia-axisymmetric-flow-v1")
        return tr("Only a generated flow case can receive the bundled backend.");
    QCryptographicHash hash(QCryptographicHash::Sha256);
    std::vector<std::pair<QString,QByteArray>> files;
    for(const auto& name:QStringList{"vortiaCentralFoam.C","ConservativeUpdate.H","ConvergencePolicy.H","ConvergenceMonitor.H","centralCourantNo.H","createFieldRefs.H","createFields.H","directionInterpolate.H","readFluxScheme.H","setRDeltaT.H","fixedRhoFvPatchScalarField.H","Make/files","Make/options","README.md","LICENSE.md"}) {
        QFile f(QString(":/vortia/openfoam/solver/")+name);
        if(!f.open(QIODevice::ReadOnly))return f.errorString();
        auto bytes=f.readAll();hash.addData(name.toUtf8());hash.addData(QByteArray(1,'\0'));hash.addData(bytes);
        files.emplace_back(name,std::move(bytes));
    }
    const auto revision=QString::fromLatin1(hash.result().toHex());
    const auto parent=caseDir+"/solver-backends",destination=parent+"/"+revision;
    if(!QDir().mkpath(parent))return tr("Cannot stage the bundled flow backend.");
    if(!QFileInfo::exists(destination)) {
        QTemporaryDir stage(parent+"/.stage-XXXXXX");
        if(!stage.isValid()||!QDir().mkpath(stage.filePath("Make")))return tr("Cannot stage the bundled flow backend.");
        for(const auto& [name,bytes]:files) {
            QFile f(stage.filePath(name));
            if(!f.open(QIODevice::WriteOnly)||f.write(bytes)!=bytes.size())return f.errorString();
        }
        if(!QDir().rename(stage.path(),destination))return tr("Cannot stage the bundled flow backend.");
        stage.setAutoRemove(false);
    } else {
        // Refuse a modified cache instead of silently trusting its name.
        for(const auto& [name,bytes]:files) {
            QFile f(destination+"/"+name);
            if(!f.open(QIODevice::ReadOnly)||f.readAll()!=bytes)return tr("The staged flow backend was modified; use a new mesh revision.");
        }
    }
    QSaveFile selected(caseDir+"/.vortia-solver-revision");
    if(!selected.open(QIODevice::WriteOnly)||selected.write(revision.toUtf8()+"\n")!=65||!selected.commit())return selected.errorString();
    return {};
}

QString updateFlowRunControls(const QString& caseDir,const QJsonObject& solver,
        const std::vector<std::pair<std::string,std::string>>& environment) {
    const double iterations=solver.value("iterations").toDouble(-1);
    const double interval=solver.value("writeInterval").toDouble(-1);
    const double courant=solver.value("maxCo").toDouble(-1);
    const double diagnostic=solver.value("logInterval").toDouble(10);
    if(!std::isfinite(diagnostic)||diagnostic<1||diagnostic>1000||std::floor(diagnostic)!=diagnostic)return tr("Invalid diagnostic output interval.");
    if(!std::isfinite(iterations)||!std::isfinite(interval)||!std::isfinite(courant)||
       iterations<1||iterations>200000||std::floor(iterations)!=iterations||
       interval<1||interval>iterations||std::floor(interval)!=interval||courant<=0||courant>.5)
        return tr("Invalid iteration limit, output interval or Courant number.");
    const auto c=solver.value("convergence").toObject();
    if(solver.contains("convergence")&&!solver.value("convergence").isObject())return tr("Invalid convergence criteria.");
    const std::vector<std::tuple<QString,double,double,double,bool>> defaults{
        {"interval",50,1,10000,true},{"minimumIterations",500,1,200000,true},{"stableSamples",5,2,100,true},
        {"massTolerance",.001,1e-8,.01,false},{"fieldTolerance",.001,1e-8,.01,false},
        {"equationTolerance",1e-5,1e-8,1e-3,false},{"absoluteMassTolerance",1e-9,1e-15,1e-6,false}};
    QString convergenceText="{ enabled "+QString(c.value("enabled").toBool(true)?"true":"false")+"; ";
    for(const auto& [key,fallback,low,high,integer]:defaults) {
        if(c.contains(key)&&!c.value(key).isDouble())return tr("Invalid convergence criteria.");
        const auto v=c.value(key).toDouble(fallback);
        if(!std::isfinite(v)||v<low||v>high||(integer&&std::floor(v)!=v))return tr("Invalid convergence criteria.");
        convergenceText+=key+" "+QString::number(v,'g',16)+"; ";
    }
    if((c.contains("enabled")&&!c.value("enabled").isBool())||c.value("minimumIterations").toDouble(500)<c.value("interval").toDouble(50))return tr("Invalid convergence criteria.");
    convergenceText+="}";
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
    for(const auto& entry:std::vector<std::pair<QString,QString>>{
        {"endTime",QString::number(iterations,'g',15)},{"writeInterval",QString::number(interval,'g',15)},
        {"maxCo",QString::number(courant,'g',15)},{"vortiaLogInterval",QString::number(diagnostic,'g',15)},{"vortiaConvergence",convergenceText}}) {
        QProcess command;command.setProcessEnvironment(captured);command.setWorkingDirectory(caseDir);
        command.start(QString::fromStdString(executable->string()),
            {staged.fileName(),"-entry",entry.first,"-set",entry.second});
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
                         const QString& projectRoot,const std::vector<std::pair<std::string,std::string>>& environment,const QString& caseStorage){
    QString parentDir=QDir(projectRoot).filePath("cases");
#ifdef Q_OS_MACOS
    // Keep solver writes/builds off iCloud-managed Documents/Desktop folders.
    // The small editable project retains the absolute reference to its results.
    parentDir=QDir::homePath()+"/Library/Vortia/cases";
#endif
    if(!caseStorage.isEmpty()) {
        if(!QDir::isAbsolutePath(caseStorage)||std::any_of(caseStorage.begin(),caseStorage.end(),[](QChar c){return c.isSpace();})) {
            QMessageBox::warning(parent,tr("Calculation storage"),tr("Choose an absolute calculation path without spaces in Preferences."));return {};
        }
        parentDir=caseStorage;
    }
    if(std::any_of(parentDir.cbegin(),parentDir.cend(),[](QChar c){return c.isSpace();}))
        parentDir=QDir::homePath()+"/Library/Vortia/cases";
    if(!QDir().mkpath(parentDir)) {
        QMessageBox::warning(parent,tr("Calculation storage"),tr("The calculation storage folder could not be created. Check its location and permissions."));return {};
    }
    QTemporaryDir stage(parentDir+"/.build-XXXXXX");if(!stage.isValid())return {};
    auto write=[&](const QString& name,const QByteArray& bytes){QFile f(stage.filePath(name));return f.open(QIODevice::WriteOnly)&&f.write(bytes)==bytes.size();};
    for(const auto& name:{"prepare.py","boundaries.py"}){QFile f(QString(":/vortia/flow/")+name);if(!f.open(QIODevice::ReadOnly)||!write(name,f.readAll()))return {};}
    auto settings=cfg;settings.remove("activeCase");settings.remove("preparedHash");settings.remove("preparedEngine");
    if(!write("settings.json",QJsonDocument(settings).toJson())||!QDir().mkpath(stage.filePath("solver/Make")))return {};
    for(const auto& name:QStringList{"vortiaCentralFoam.C","ConservativeUpdate.H","ConvergencePolicy.H","ConvergenceMonitor.H","centralCourantNo.H","createFieldRefs.H","createFields.H","directionInterpolate.H","readFluxScheme.H","setRDeltaT.H","fixedRhoFvPatchScalarField.H","Make/files","Make/options","README.md","LICENSE.md"}){
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
