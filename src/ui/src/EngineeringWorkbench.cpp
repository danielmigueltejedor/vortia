#include "vortia/ui/EngineeringWorkbench.hpp"
#include "vortia/ui/EngineeringReport.hpp"
#include "vortia/visualization/SectionAnalysis.hpp"
#include "vortia/visualization/LineSampling.hpp"
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFile>
#include <QFileDialog>
#include <QFormLayout>
#include <QHeaderView>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QLocale>
#include <QPushButton>
#include <QSaveFile>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QSplitter>
#include <QTabWidget>
#include <QTableWidget>
#include <QTextBrowser>
#include <QVBoxLayout>
#include <vtkDataObject.h>
#include <cmath>
#include <algorithm>
namespace vortia {
namespace {
QString workTr(const char* text){return QCoreApplication::translate("vortia::Engineering",text);}
QLabel* note(const QString& text,QWidget* parent){auto* label=new QLabel(text,parent);label->setWordWrap(true);return label;}
void setupTable(QTableWidget* table,const QStringList& headers){table->setColumnCount(headers.size());table->setHorizontalHeaderLabels(headers);table->verticalHeader()->hide();table->setEditTriggers(QAbstractItemView::NoEditTriggers);table->setSelectionBehavior(QAbstractItemView::SelectRows);table->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);table->setAlternatingRowColors(true);}
}
EngineeringWorkbench::EngineeringWorkbench(vtkDataObject* data,double angle,const QJsonObject& source,const QJsonObject& saved,bool canSave,QWidget* parent)
    :QDialog(parent),data_(data),wedgeAngle_(angle),source_(source){
    setObjectName("vortia-engineering-workbench");setWindowTitle(workTr("Engineering analysis"));resize(1120,800);
    auto* layout=new QVBoxLayout(this);layout->addWidget(note(workTr("Measure saved flow fields, compare variants and export a traceable report. Analysis does not change the CFD case."),this));
    auto* tabs=new QTabWidget(this);tabs->setObjectName("engineering-tabs");layout->addWidget(tabs,1);
    auto* analysis=new QWidget(tabs);auto* row=new QHBoxLayout(analysis);auto* formHost=new QWidget(analysis);auto* form=new QFormLayout(formHost);form->setRowWrapPolicy(QFormLayout::WrapAllRows);
    auto* scroll=new QScrollArea(analysis);scroll->setWidgetResizable(true);scroll->setFrameShape(QFrame::NoFrame);scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);scroll->setMinimumWidth(290);scroll->setMaximumWidth(380);scroll->setWidget(formHost);row->addWidget(scroll);
    const auto fieldRow=[form,formHost](const QString& title,QWidget* input){auto* label=note(title,formHost);form->addRow(label);form->addRow(input);};
    const auto previous=sectionOptionsFromJson(saved.value("options").toObject());
    name_=new QLineEdit(saved.value("name").toString(workTr("Section analysis")),formHost);name_->setObjectName("engineering-name");name_->setMaxLength(160);fieldRow(workTr("Analysis name"),name_);
    axis_=new QComboBox(formHost);axis_->setObjectName("engineering-axis");axis_->addItems({"X","Y","Z"});axis_->setCurrentIndex(std::clamp(previous.axis,0,2));fieldRow(workTr("Section normal axis"),axis_);
    normal_=new QComboBox(formHost);normal_->setObjectName("engineering-normal");normal_->addItem(workTr("Positive axis"),1);normal_->addItem(workTr("Negative axis"),-1);normal_->setCurrentIndex(previous.normal==-1?1:0);fieldRow(workTr("Positive flow direction"),normal_);
    const auto number=[&](const char* key,const char* label,double value,double low,double high,int decimals){auto* spin=new QDoubleSpinBox(formHost);spin->setObjectName(QString("engineering-%1").arg(key));spin->setDecimals(decimals);spin->setRange(low,high);spin->setValue(value);spin->setKeyboardTracking(false);fieldRow(workTr(label),spin);return spin;};
    position_=number("position","Plane position (m)",previous.position,-1e12,1e12,9);
    densityMode_=new QComboBox(formHost);densityMode_->setObjectName("engineering-density-mode");densityMode_->addItem(workTr("Stored density field rho"));densityMode_->addItem(workTr("Explicit constant density"));densityMode_->setCurrentIndex(previous.useDensityField?0:1);fieldRow(workTr("Density interpretation"),densityMode_);
    density_=number("density","Constant density (kg/m³)",previous.constantDensity,1e-9,1e9,9);density_->setEnabled(densityMode_->currentIndex()==1);
    pressureMode_=new QComboBox(formHost);pressureMode_->setObjectName("engineering-pressure-mode");pressureMode_->addItem(workTr("Pressure p in Pa"));pressureMode_->addItem(workTr("Kinematic pressure p/rho (m²/s²)"));pressureMode_->setCurrentIndex(previous.kinematicPressure?1:0);fieldRow(workTr("Stored pressure convention"),pressureMode_);
    referencePressure_=number("reference-pressure","Reference pressure (Pa)",previous.referencePressure,-1e12,1e12,3);
    referenceDensity_=number("reference-density","Reference density (kg/m³)",previous.referenceDensity,1e-9,1e9,9);
    referenceSpeed_=number("reference-speed","Reference speed (m/s)",previous.referenceSpeed,1e-6,1e7,6);
    referenceArea_=number("reference-area","Reference area (m²)",previous.referenceArea,1e-12,1e12,12);
    annulus_=new QCheckBox(workTr("Report full 360° annulus"),formHost);annulus_->setObjectName("engineering-annulus");annulus_->setChecked(previous.fullAnnulus&&angle>0&&axis_->currentIndex()==0);annulus_->setEnabled(angle>0&&axis_->currentIndex()==0);form->addRow(annulus_);
    form->addRow(note(workTr("Full-annulus totals use 360/wedge-angle only for an axisymmetric X section. Otherwise totals refer to the original calculated volume. Pressure units and reference values are explicit inputs."),formHost));
    auto* evaluateButton=new QPushButton(workTr("Evaluate section"),formHost);evaluateButton->setObjectName("engineering-evaluate");
    auto* resultHost=new QWidget(analysis);auto* resultLayout=new QVBoxLayout(resultHost);resultLayout->addWidget(evaluateButton);
    values_=new QTableWidget(resultHost);values_->setObjectName("engineering-values");setupTable(values_,{workTr("Quantity"),workTr("Value")});values_->horizontalHeader()->setSectionResizeMode(1,QHeaderView::ResizeToContents);resultLayout->addWidget(values_,1);
    status_=note(workTr("Choose a section and evaluate the saved fields."),resultHost);status_->setObjectName("engineering-status");resultLayout->addWidget(status_);
    resultLayout->addWidget(note(workTr("The section resultant combines normal momentum flux and the pressure term. It is not a complete aircraft force, engine thrust or viscous wall-load report. Numerical convergence, model suitability and mesh independence require separate evidence."),resultHost));
    add_=new QPushButton(workTr("Add snapshot to comparison"),resultHost);add_->setObjectName("engineering-add-comparison");add_->setEnabled(false);resultLayout->addWidget(add_);row->addWidget(resultHost,1);tabs->addTab(analysis,workTr("Sections"));
    auto* comparePage=new QWidget(tabs);auto* compareLayout=new QVBoxLayout(comparePage);auto* choices=new QHBoxLayout;
    baseline_=new QComboBox(comparePage);baseline_->setObjectName("engineering-baseline");metric_=new QComboBox(comparePage);metric_->setObjectName("engineering-comparison-metric");for(const auto& key:engineeringMetricKeys())metric_->addItem(engineeringMetricTitle(key),key);metric_->setCurrentIndex(1);
    choices->addWidget(note(workTr("Baseline"),comparePage));choices->addWidget(baseline_,1);choices->addWidget(metric_,2);compareLayout->addLayout(choices);
    comparison_=new QTableWidget(comparePage);comparison_->setObjectName("engineering-comparison");setupTable(comparison_,{workTr("Analysis"),workTr("Value"),workTr("Change from baseline (%)"),workTr("Source / sample fingerprint")});compareLayout->addWidget(comparison_,1);
    comparisonStatus_=note({},comparePage);comparisonStatus_->setObjectName("engineering-comparison-status");compareLayout->addWidget(comparisonStatus_);
    auto* compareButtons=new QHBoxLayout;auto* import=new QPushButton(workTr("Import report JSON…"),comparePage);import->setObjectName("engineering-import");compareButtons->addWidget(import);auto* remove=new QPushButton(workTr("Remove selected snapshot"),comparePage);remove->setObjectName("engineering-remove");compareButtons->addWidget(remove);auto* exportComparison=new QPushButton(workTr("Export comparison CSV…"),comparePage);exportComparison->setObjectName("engineering-export-comparison");compareButtons->addWidget(exportComparison);
    connect(exportComparison,&QPushButton::clicked,this,[this](){if(comparisons_.isEmpty())return;const auto path=QFileDialog::getSaveFileName(this,workTr("Export comparison"),{},"CSV (*.csv)");if(path.isEmpty())return;const auto bytes=engineeringComparisonCsv(comparisons_,metric_->currentData().toString(),baseline_->currentIndex());QSaveFile file(path);if(!file.open(QIODevice::WriteOnly)||file.write(bytes)!=bytes.size()||!file.commit())comparisonStatus_->setText(workTr("The engineering report could not be saved."));});
    compareButtons->addStretch();compareLayout->addLayout(compareButtons);tabs->addTab(comparePage,workTr("Comparisons"));
    auto* reportPage=new QWidget(tabs);auto* reportLayout=new QVBoxLayout(reportPage);reportPreview_=new QTextBrowser(reportPage);reportPreview_->setObjectName("engineering-report-preview");reportPreview_->setOpenExternalLinks(false);reportLayout->addWidget(reportPreview_,1);
    export_=new QPushButton(workTr("Export current report…"),reportPage);export_->setObjectName("engineering-export");export_->setEnabled(false);reportLayout->addWidget(export_);tabs->addTab(reportPage,workTr("Report"));
    auto* buttons=new QDialogButtonBox(QDialogButtonBox::Close,this);save_=buttons->addButton(workTr("Save analysis workspace to project"),QDialogButtonBox::ActionRole);save_->setObjectName("engineering-save-project");save_->setEnabled(canSave);layout->addWidget(buttons);connect(buttons,&QDialogButtonBox::rejected,this,&QDialog::reject);
    for(auto* label:formHost->findChildren<QLabel*>())label->setWordWrap(true);
    for(auto* combo:{axis_,normal_,densityMode_,pressureMode_}){combo->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);combo->setMinimumContentsLength(10);combo->setToolTip(combo->currentText());}
    referencePressure_->setToolTip(workTr("Use the same absolute or gauge convention as the stored pressure after conversion to Pa."));
    const auto updatePlane=[this](){try{const auto bounds=sectionAnalysisBounds(data_);const int a=axis_->currentIndex();QSignalBlocker blocked(position_);position_->setRange(bounds[2*a],bounds[2*a+1]);if(position_->value()<=bounds[2*a]||position_->value()>=bounds[2*a+1])position_->setValue((bounds[2*a]+bounds[2*a+1])/2);position_->setSingleStep(std::max(1e-9,(bounds[2*a+1]-bounds[2*a])/100));}catch(const std::exception&){}annulus_->setEnabled(wedgeAngle_>0&&axis_->currentIndex()==0);if(!annulus_->isEnabled())annulus_->setChecked(false);invalidate();};
    connect(axis_,qOverload<int>(&QComboBox::currentIndexChanged),this,[updatePlane](int){updatePlane();});updatePlane();
    for(auto* spin:{position_,density_,referencePressure_,referenceDensity_,referenceSpeed_,referenceArea_})connect(spin,qOverload<double>(&QDoubleSpinBox::valueChanged),this,[this](double){invalidate();});
    for(auto* combo:{normal_,densityMode_,pressureMode_})connect(combo,qOverload<int>(&QComboBox::currentIndexChanged),this,[this](int){density_->setEnabled(densityMode_->currentIndex()==1);invalidate();});
    connect(name_,&QLineEdit::textChanged,this,[this](){invalidate();});connect(annulus_,&QCheckBox::toggled,this,[this](){invalidate();});connect(evaluateButton,&QPushButton::clicked,this,[this](){evaluate();});
    connect(add_,&QPushButton::clicked,this,[this](){if(report_.isEmpty()||comparisons_.size()>=64)return;comparisons_.append(report_);refreshComparison();});
    connect(import,&QPushButton::clicked,this,[this](){importReports();});connect(remove,&QPushButton::clicked,this,[this](){const int row=comparison_->currentRow();if(row>=0&&row<comparisons_.size()){comparisons_.removeAt(row);refreshComparison();}});
    connect(baseline_,qOverload<int>(&QComboBox::currentIndexChanged),this,[this](int){refreshComparison();});connect(metric_,qOverload<int>(&QComboBox::currentIndexChanged),this,[this](int){refreshComparison();});
    connect(export_,&QPushButton::clicked,this,[this](){exportReport();});connect(save_,&QPushButton::clicked,this,[this](){if(saveToProject)saveToProject(workspaceState());});
    for(const auto& report:saved.value("comparisons").toArray())if(comparisons_.size()<64&&validEngineeringReport(report.toObject()))comparisons_.append(report);refreshComparison();
}
QJsonObject EngineeringWorkbench::workspaceState() const {
    SectionOptions o;o.axis=axis_->currentIndex();o.normal=normal_->currentData().toInt();o.position=position_->value();o.useDensityField=densityMode_->currentIndex()==0;o.constantDensity=density_->value();o.kinematicPressure=pressureMode_->currentIndex()==1;o.referencePressure=referencePressure_->value();o.referenceDensity=referenceDensity_->value();o.referenceSpeed=referenceSpeed_->value();o.referenceArea=referenceArea_->value();o.fullAnnulus=annulus_->isChecked();o.wedgeAngle=wedgeAngle_;
    return {{"options",sectionOptionsJson(o)},{"name",name_->text()},{"comparisons",comparisons_}};
}
void EngineeringWorkbench::invalidate(){report_={};add_->setEnabled(false);export_->setEnabled(false);values_->setRowCount(0);reportPreview_->clear();status_->setText(workTr("Inputs changed. Evaluate the section to produce a new snapshot."));}
void EngineeringWorkbench::evaluate(){
    invalidate();try{const auto o=sectionOptionsFromJson(workspaceState().value("options").toObject());const auto r=analyzeSection(data_,o);report_=makeEngineeringReport(o,r,source_,name_->text());const auto keys=engineeringMetricKeys();const auto metrics=sectionMetricsJson(r);values_->setRowCount(keys.size());for(int i=0;i<keys.size();++i){values_->setItem(i,0,new QTableWidgetItem(engineeringMetricTitle(keys[i])));values_->setItem(i,1,new QTableWidgetItem(QLocale().toString(metrics.value(keys[i]).toDouble(),'g',11)));}
        values_->resizeRowsToContents();status_->setText(workTr("%1 triangles · %2 volume blocks · scale %3 · reverse flow %4 kg/s\nSample SHA-256: %5").arg(r.triangles).arg(r.blocks).arg(r.areaScale,0,'g',8).arg(r.reverseMassFlow,0,'g',8).arg(r.sampleFingerprint));add_->setEnabled(comparisons_.size()<64);export_->setEnabled(true);reportPreview_->setHtml(engineeringReportHtml(report_));
    }catch(const std::exception& e){invalidate();status_->setText(workTr(e.what()));}
}
void EngineeringWorkbench::refreshComparison(){
    int selected=baseline_->currentIndex();{QSignalBlocker blocked(baseline_);baseline_->clear();for(const auto& value:comparisons_)baseline_->addItem(value.toObject().value("payload").toObject().value("label").toString());baseline_->setCurrentIndex(std::clamp(selected,0,std::max(0,static_cast<int>(comparisons_.size())-1)));}
    comparison_->setRowCount(comparisons_.size());QStringList warnings;const auto key=metric_->currentData().toString();const auto baseline=baseline_->currentIndex()>=0?comparisons_[baseline_->currentIndex()].toObject():QJsonObject{};const double reference=baseline.value("payload").toObject().value("metrics").toObject().value(key).toDouble();
    for(int i=0;i<comparisons_.size();++i){const auto item=comparisons_[i].toObject(),payload=item.value("payload").toObject();const double value=payload.value("metrics").toObject().value(key).toDouble();const auto warning=engineeringComparisonWarning(baseline,item);if(!warning.isEmpty()&&!warnings.contains(warning))warnings.append(warning);comparison_->setItem(i,0,new QTableWidgetItem(payload.value("label").toString()));comparison_->setItem(i,1,new QTableWidgetItem(QLocale().toString(value,'g',11)));comparison_->setItem(i,2,new QTableWidgetItem(reference!=0?QLocale().toString(100*(value-reference)/std::abs(reference),'g',8):workTr("Undefined: zero baseline")));comparison_->setItem(i,3,new QTableWidgetItem(payload.value("source").toObject().value("project").toString()+" · "+payload.value("sampleFingerprint").toString().left(16)));}
    comparisonStatus_->setText(warnings.isEmpty()?workTr("Snapshots retain their original definitions. A percentage change does not establish mesh independence or physical validity."):warnings.join('\n'));add_->setEnabled(!report_.isEmpty()&&comparisons_.size()<64);
}
void EngineeringWorkbench::importReports(){
    const auto paths=QFileDialog::getOpenFileNames(this,workTr("Import engineering reports"),{},"JSON (*.json)");int invalid=0;
    for(const auto& path:paths){QFile file(path);if(comparisons_.size()>=64||file.size()>4*1024*1024||!file.open(QIODevice::ReadOnly)){++invalid;continue;}const auto parsed=QJsonDocument::fromJson(file.readAll());if(!parsed.isObject()||!validEngineeringReport(parsed.object())){++invalid;continue;}comparisons_.append(parsed.object());}
    refreshComparison();if(invalid)comparisonStatus_->setText(workTr("%1 reports were rejected: invalid schema, changed digest, excessive size or comparison limit.").arg(invalid));
}
void EngineeringWorkbench::exportReport(){
    if(report_.isEmpty())return;QString filter;const auto path=QFileDialog::getSaveFileName(this,workTr("Export engineering report"),{},"HTML (*.html);;JSON (*.json);;CSV (*.csv)",&filter);if(path.isEmpty())return;
    const QByteArray bytes=filter.startsWith("JSON")?QJsonDocument(report_).toJson(QJsonDocument::Indented):filter.startsWith("CSV")?engineeringReportCsv(report_):engineeringReportHtml(report_).toUtf8();QSaveFile file(path);if(!file.open(QIODevice::WriteOnly)||file.write(bytes)!=bytes.size()||!file.commit())status_->setText(workTr("The engineering report could not be saved."));
}
}
