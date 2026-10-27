#include "vortia/ui/ApplicationPreferences.hpp"
#include "vortia/ui/SettingsNavigation.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/ui/OpenFoamRuntime.hpp"
#include <QSettings>
#include <QThread>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QSpinBox>
#include <QComboBox>
#include <QCheckBox>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QStackedWidget>
#include <QScrollArea>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QGroupBox>
#include <QPushButton>
#include <QFileDialog>
#include <QFileInfo>
#include <QDir>
#include <QMessageBox>
#include <algorithm>
#include <cmath>
namespace vortia {
namespace {
QString tr(const char* s){return QCoreApplication::translate("vortia::ApplicationPreferences",s);}
int integer(QSettings& s,const char* key,int fallback,int low,int high) {
    bool ok=false;const int v=s.value(key,fallback).toInt(&ok);return ok&&v>=low&&v<=high?v:fallback;
}
double number(QSettings& s,const char* key,double fallback,double low,double high) {
    bool ok=false;const double v=s.value(key,fallback).toDouble(&ok);return ok&&std::isfinite(v)&&v>=low&&v<=high?v:fallback;
}
}
ApplicationPreferences ApplicationPreferences::load(QSettings& s) {
    ApplicationPreferences p;
    const auto choice=[&](const char* key,const QString& fallback,const QStringList& values){const auto v=s.value(key,fallback).toString();return values.contains(v)?v:fallback;};
    p.language=choice("language",p.language,{"system","en","es"});p.theme=choice("theme",p.theme,{"dark","light"});
    p.density=choice("ui/density",p.density,{"comfortable","compact"});
    p.interfaceScale=integer(s,"ui/scale",100,90,140);
    p.showToolLabels=s.value("ui/showToolLabels",true).toBool();
    p.automaticMeshPreview=s.value("mesh/automaticPreview",true).toBool();
    p.defaultProcesses=integer(s,"calculation/processes",0,0,8);
    p.checkpointInterval=integer(s,"calculation/checkpointInterval",500,1,200000);
    p.iterationLimit=integer(s,"calculation/iterationLimit",50000,1,200000);
    p.checkpointInterval=std::min(p.checkpointInterval,p.iterationLimit);
    p.consoleLines=integer(s,"console/lines",10000,1000,100000);
    p.zoomSensitivity=number(s,"sketch/zoomSensitivity",1,.25,3);
    p.cfdZoomSensitivity=number(s,"viewport/zoomSensitivity",1,.25,3);
    p.cfdPanSensitivity=number(s,"viewport/panSensitivity",1,.25,3);
    p.snapSpacing=number(s,"sketch/snapSpacing",1,.001,1000);
    p.snapToGrid=s.value("sketch/snap",true).toBool();p.showGrid=s.value("sketch/showGrid",true).toBool();
    p.showDimensions=s.value("sketch/showDimensions",true).toBool();p.rememberLayout=s.value("window/rememberLayout",true).toBool();
    p.showConsoleOnRun=s.value("calculation/showConsole",false).toBool();
    p.showAssessmentOnRun=s.value("calculation/showAssessment",true).toBool();
    p.automaticPreview=s.value("results/automaticPreview",true).toBool();
    p.backendRoot=s.value("backend/root").toString();p.caseStorage=s.value("storage/cases").toString();
    return p;
}
void ApplicationPreferences::save(QSettings& s) const {
    s.setValue("language",language);s.setValue("theme",theme);s.setValue("ui/density",density);s.setValue("ui/scale",interfaceScale);
    s.setValue("ui/showToolLabels",showToolLabels);s.remove("ui/showStageNumbers");
    s.setValue("mesh/automaticPreview",automaticMeshPreview);
    s.setValue("calculation/processes",defaultProcesses);s.setValue("calculation/checkpointInterval",checkpointInterval);
    s.setValue("calculation/iterationLimit",iterationLimit);s.setValue("console/lines",consoleLines);
    s.setValue("sketch/zoomSensitivity",zoomSensitivity);s.setValue("sketch/snapSpacing",snapSpacing);
    s.setValue("viewport/zoomSensitivity",cfdZoomSensitivity);s.setValue("viewport/panSensitivity",cfdPanSensitivity);
    s.setValue("sketch/snap",snapToGrid);s.setValue("sketch/showGrid",showGrid);s.setValue("sketch/showDimensions",showDimensions);
    s.setValue("window/rememberLayout",rememberLayout);s.setValue("calculation/showConsole",showConsoleOnRun);
    s.setValue("calculation/showAssessment",showAssessmentOnRun);s.setValue("results/automaticPreview",automaticPreview);
    s.setValue("backend/root",backendRoot);s.setValue("storage/cases",caseStorage);s.sync();
}
QJsonObject ApplicationPreferences::flowDefaults() const {
    return {{"solver",QJsonObject{{"processes",defaultProcesses},
        {"writeInterval",checkpointInterval},{"iterations",iterationLimit}}}};
}
std::optional<ApplicationPreferences> editApplicationPreferences(QWidget* parent,const ApplicationPreferences& previous,bool running) {
    QDialog dialog(parent);dialog.setObjectName("vortia-preferences");dialog.setWindowTitle(tr("Vortia preferences"));dialog.resize(980,740);

    auto* layout=new QVBoxLayout(&dialog);auto* title=new QLabel(tr("Make Vortia yours"),&dialog);title->setStyleSheet("font-size:20px; font-weight:500;");layout->addWidget(title);
    auto* body=new QHBoxLayout;layout->addLayout(body,1);
    auto* pages=new QStackedWidget(&dialog);pages->setObjectName("preferences-pages");
    auto* nav=new SettingsNavigation(&dialog,pages,"preferences-navigation");body->addWidget(nav);body->addWidget(pages,1);
    auto page=[&](const char* text,const char* icon,const char* shortTitle){
        auto* scroll=new QScrollArea(pages);scroll->setWidgetResizable(true);scroll->setFrameShape(QFrame::NoFrame);
        auto* widget=new QWidget(scroll);auto* box=new QVBoxLayout(widget);auto* card=new QGroupBox(tr(text),widget);box->addWidget(card);box->addStretch();
        auto* form=new QFormLayout(card);form->setSpacing(15);form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
        scroll->setWidget(widget);pages->addWidget(scroll);nav->addPage(tr(shortTitle),QString(":/vortia/icons/%1.svg").arg(icon),tr(text));return form;
    };
    auto info=[&](QFormLayout* f,const char* text){auto* label=new QLabel(tr(text),&dialog);label->setWordWrap(true);f->addRow(label);};
    auto check=[&](QFormLayout* f,const char* key,const char* text,bool value){auto* w=new QCheckBox(tr(text),&dialog);w->setObjectName(QString("preference-%1").arg(key));w->setChecked(value);f->addRow(w);return w;};
    auto spin=[&](QFormLayout* f,const char* key,const char* text,int value,int low,int high){auto* w=new QSpinBox(&dialog);w->setObjectName(QString("preference-%1").arg(key));w->setRange(low,high);w->setValue(value);f->addRow(tr(text),w);return w;};
    auto real=[&](QFormLayout* f,const char* key,const char* text,double value,double low,double high){auto* w=new QDoubleSpinBox(&dialog);w->setObjectName(QString("preference-%1").arg(key));w->setDecimals(3);w->setRange(low,high);w->setValue(value);f->addRow(tr(text),w);return w;};
    auto* appearance=page("Appearance and workspace","settings","Appearance");
    auto* language=new QComboBox(&dialog);language->setObjectName("preference-language");language->addItem(tr("System language"),"system");language->addItem("Español","es");language->addItem("English","en");language->setCurrentIndex(std::max(0,language->findData(previous.language)));appearance->addRow(tr("Language"),language);
    auto* theme=new QComboBox(&dialog);theme->setObjectName("preference-theme");theme->addItem(tr("Dark"),"dark");theme->addItem(tr("Light"),"light");theme->setCurrentIndex(std::max(0,theme->findData(previous.theme)));appearance->addRow(tr("Theme"),theme);
    auto* density=new QComboBox(&dialog);density->setObjectName("preference-density");density->addItem(tr("Comfortable"),"comfortable");density->addItem(tr("Compact"),"compact");density->setCurrentIndex(std::max(0,density->findData(previous.density)));appearance->addRow(tr("Tool density"),density);
    auto* scale=spin(appearance,"scale","Interface size (%)",previous.interfaceScale,90,140);scale->setSingleStep(10);
    auto* remember=check(appearance,"remember-layout","Remember window and panel layout",previous.rememberLayout);
    auto* labels=check(appearance,"tool-labels","Show labels beside tool icons",previous.showToolLabels);
    info(appearance,"Appearance and navigation changes apply immediately. Language changes apply after restarting.");
    auto* sketch=page("Sketch and navigation","sketch","Sketch");
    auto* grid=check(sketch,"grid","Show construction grid",previous.showGrid);
    auto* dimensions=check(sketch,"dimensions","Show dimension annotations",previous.showDimensions);
    auto* snap=check(sketch,"snap","Snap drawing and dragging to grid",previous.snapToGrid);
    auto* spacing=real(sketch,"snap-spacing","Snap spacing (mm)",previous.snapSpacing,.001,1000);
    auto* zoom=real(sketch,"zoom","Zoom sensitivity",previous.zoomSensitivity,.25,3);
    auto* cfdZoom=real(sketch,"cfd-zoom","CFD zoom sensitivity",previous.cfdZoomSensitivity,.25,3);
    auto* cfdPan=real(sketch,"cfd-pan","CFD pan sensitivity",previous.cfdPanSensitivity,.25,3);
    auto* meshPreview=check(sketch,"mesh-preview","Update mesh preview when entering Mesh",previous.automaticMeshPreview);
    info(sketch,"These options change interaction and display. They do not alter saved geometry or numerical units.");
    auto* calculation=page("Calculation and results","solver","Calculation");
    auto* cpu=spin(calculation,"processes","Default CPU processes",previous.defaultProcesses,0,8);cpu->setSpecialValueText(tr("Automatic"));
    auto* limit=spin(calculation,"iterations","Default safety iteration limit",previous.iterationLimit,1,200000);
    auto* interval=spin(calculation,"checkpoint","Default checkpoint interval",previous.checkpointInterval,1,200000);
    info(calculation,"CPU and checkpoint defaults apply only to new flow configurations. Existing projects keep their own controls. A longer checkpoint interval reduces disk writes but can lose more progress after cancellation.");
    auto* preview=check(calculation,"preview","Prepare result visualization automatically",previous.automaticPreview);
    info(calculation,"Turn off automatic visualization to finish calculations sooner. Use View latest result when you need it; numerical checkpoints are always retained.");
    auto* assessment=check(calculation,"assessment","Open numerical assessment when a run starts",previous.showAssessmentOnRun);
    auto* console=check(calculation,"console","Show detailed console when a run starts",previous.showConsoleOnRun);
    auto* lines=spin(calculation,"console-lines","Console history (lines)",previous.consoleLines,1000,100000);lines->setSingleStep(1000);
    auto* storage=page("Storage and OpenFOAM","open","Engine and storage");
    auto folder=[&](const char* key,const char* label,const QString& value){auto* row=new QWidget(&dialog);auto* h=new QHBoxLayout(row);h->setContentsMargins(0,0,0,0);auto* text=new QLineEdit(value,row);text->setObjectName(QString("preference-%1").arg(key));text->setPlaceholderText(tr("Automatic (recommended)"));auto* browse=new QPushButton(tr("Choose…"),row);h->addWidget(text,1);h->addWidget(browse);storage->addRow(tr(label),row);QObject::connect(browse,&QPushButton::clicked,&dialog,[&dialog,text](){const auto path=QFileDialog::getExistingDirectory(&dialog,tr("Choose folder"),text->text());if(!path.isEmpty())text->setText(path);});return text;};
    auto* cases=folder("storage","New calculation storage",previous.caseStorage);
    auto* backend=folder("backend","OpenFOAM installation",previous.backendRoot);backend->parentWidget()->setEnabled(!running);cases->parentWidget()->setEnabled(!running);
    backend->setPlaceholderText(tr("Integrated engine (recommended)"));
    info(storage,"Vortia uses its integrated OpenFOAM engine automatically when available. Choose an external installation only to override it.");
    info(storage,"Leave paths empty for automatic locations. New storage does not move existing cases. OpenFOAM calculation folders must have absolute paths without spaces. Changing the engine or storage is disabled during a calculation.");
    auto* detect=new QPushButton(tr("Find installed OpenFOAM"),&dialog);storage->addRow(detect);detect->setEnabled(!running);
    QObject::connect(detect,&QPushButton::clicked,&dialog,[&dialog,backend](){
        PosixProcessRunner runner;
        const auto found=openfoam::Detector::discover(runner,{},bundledOpenFoamRoot());
        if(!found){QMessageBox::warning(&dialog,tr("OpenFOAM"),QString::fromStdString(found.error().describe()));return;}
        if(found->empty()){QMessageBox::information(&dialog,tr("OpenFOAM"),tr("No installation found. Choose its folder manually."));return;}
        backend->setText(QString::fromStdString(found->front().rootDir.string()));
    });
    nav->selectPage(0);auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);layout->addWidget(buttons);
    QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    QObject::connect(buttons,&QDialogButtonBox::accepted,&dialog,[&](){
        if(interval->value()>limit->value()){QMessageBox::warning(&dialog,tr("Calculation defaults"),tr("The checkpoint interval cannot exceed the safety limit."));return;}
        const auto path=cases->text().trimmed();
        if(!path.isEmpty()&&(!QDir::isAbsolutePath(path)||std::any_of(path.begin(),path.end(),[](QChar c){return c.isSpace();}))){QMessageBox::warning(&dialog,tr("Storage"),tr("Choose an absolute calculation path without spaces."));return;}
        if(!backend->text().trimmed().isEmpty()) {
            const auto found=openfoam::Detector::probe(backend->text().trimmed().toStdString());
            if(!found){QMessageBox::warning(&dialog,tr("OpenFOAM"),tr("The selected folder is not a recognized OpenFOAM installation."));return;}
        }
        dialog.accept();
    });
    if(dialog.exec()!=QDialog::Accepted)return {};
    auto p=previous;p.language=language->currentData().toString();p.theme=theme->currentData().toString();p.density=density->currentData().toString();p.interfaceScale=scale->value();
    p.showToolLabels=labels->isChecked();
    p.automaticMeshPreview=meshPreview->isChecked();
    p.rememberLayout=remember->isChecked();p.showGrid=grid->isChecked();p.showDimensions=dimensions->isChecked();p.snapToGrid=snap->isChecked();p.snapSpacing=spacing->value();p.zoomSensitivity=zoom->value();
    p.cfdZoomSensitivity=cfdZoom->value();p.cfdPanSensitivity=cfdPan->value();
    p.defaultProcesses=cpu->value();p.iterationLimit=limit->value();p.checkpointInterval=interval->value();p.consoleLines=lines->value();p.automaticPreview=preview->isChecked();p.showAssessmentOnRun=assessment->isChecked();p.showConsoleOnRun=console->isChecked();
    p.caseStorage=cases->text().trimmed();p.backendRoot=backend->text().trimmed();return p;
}
}
