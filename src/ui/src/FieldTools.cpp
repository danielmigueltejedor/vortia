#include "vortia/ui/FieldTools.hpp"
#include "vortia/visualization/VtkViewport.hpp"
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QComboBox>
#include <QSlider>
#include <QCheckBox>
#include <QLabel>
#include <QPushButton>
#include <QFormLayout>
#include <QVBoxLayout>
#include <QFileDialog>
#include <QMessageBox>
#include <QSaveFile>
#include <QTextStream>
#include <QSignalBlocker>
#include <QTimer>
#include <QLocale>
#include <vtkCamera.h>
#include <array>
#include <algorithm>
#include <cmath>
namespace vortia {
namespace {
QString tr(const char* text){return QCoreApplication::translate("vortia::FieldTools",text);}
QComboBox* fields(QWidget* parent,VtkViewport* viewport,const QString& selected) {
    auto* combo=new QComboBox(parent);combo->setObjectName("field-tool-field");
    for(const auto& name:viewport->availableFields()){const auto raw=QString::fromStdString(name);combo->addItem(fieldDisplayName(raw),raw);}
    const int index=combo->findData(selected);if(index>=0)combo->setCurrentIndex(index);return combo;
}
}
QString fieldDisplayName(const QString& field) {
    if(field=="p")return tr("Pressure · p (Pa)");
    if(field=="T")return tr("Temperature · T (K)");
    if(field=="rho")return tr("Density · rho (kg/m³)");
    if(field=="U"||field=="|U|")return tr("Speed · |U| (m/s)");
    if(field=="Ux")return tr("Velocity X (m/s)");
    if(field=="Uy")return tr("Velocity Y (m/s)");
    if(field=="Uz")return tr("Velocity Z (m/s)");
    if(field=="Mach"||field=="Ma")return tr("Mach number");
    if(field=="nut")return tr("Turbulent viscosity · nut (m²/s)");
    if(field=="nuTilda")return tr("Turbulence variable · nuTilda (m²/s)");
    if(field=="yPlus")return tr("Wall resolution · y+");
    return field;
}
void showPlaneSection(QWidget* parent,VtkViewport* viewport) {
    try {
        const auto bounds=viewport->fieldBounds();const auto previous=viewport->slicePlaneState();auto camera=viewport->copyCamera();
        QDialog dialog(parent);dialog.setObjectName("vortia-plane-section");dialog.setWindowTitle(tr("Plane section"));dialog.resize(550,330);
        auto* form=new QFormLayout(&dialog);
        auto* note=new QLabel(tr("Preview an interpolated section of the loaded fluid block. Coordinates are in metres; the original fields and mesh are retained."),&dialog);note->setWordWrap(true);form->addRow(note);
        auto* axis=new QComboBox(&dialog);axis->setObjectName("section-axis");axis->addItems({tr("Normal to X · YZ plane"),tr("Normal to Y · XZ plane"),tr("Normal to Z · XY plane")});
        axis->setCurrentIndex(previous.enabled?previous.axis:2);form->addRow(tr("Orientation"),axis);
        auto* position=new QDoubleSpinBox(&dialog);position->setObjectName("section-position");position->setDecimals(9);position->setSuffix(" m");form->addRow(tr("Position"),position);
        auto* slider=new QSlider(Qt::Horizontal,&dialog);slider->setObjectName("section-slider");slider->setRange(1,999);form->addRow(slider);
        auto* orient=new QCheckBox(tr("Face the section plane"),&dialog);orient->setChecked(true);form->addRow(orient);
        auto* status=new QLabel(&dialog);status->setObjectName("section-status");status->setWordWrap(true);form->addRow(status);
        QTimer preview;preview.setSingleShot(true);preview.setInterval(60);
        const auto apply=[&](){
            try {viewport->setSlicePlane(axis->currentIndex(),position->value());
                if(orient->isChecked()){if(axis->currentIndex()==0)viewport->setViewYZ();else if(axis->currentIndex()==1)viewport->setViewXZ();else viewport->setViewXY();}
                status->setText(tr("Section preview · original data retained"));return true;
            }catch(const std::exception& error){status->setText(tr(error.what()));return false;}
        };
        QObject::connect(&preview,&QTimer::timeout,&dialog,[&](){apply();});
        const auto range=[&](){const int a=axis->currentIndex();const double lo=bounds[2*a],hi=bounds[2*a+1];
            QSignalBlocker block(position);position->setRange(lo,hi);position->setSingleStep(std::max(1e-9,(hi-lo)/1000));position->setValue(previous.enabled&&a==previous.axis?previous.position:(lo+hi)/2);
            QSignalBlocker blocked(slider);slider->setValue(hi>lo?std::clamp(static_cast<int>((position->value()-lo)/(hi-lo)*1000),1,999):500);preview.start();};
        QObject::connect(axis,qOverload<int>(&QComboBox::currentIndexChanged),&dialog,[&](int){range();});
        QObject::connect(position,qOverload<double>(&QDoubleSpinBox::valueChanged),&dialog,[&](double value){
            const int a=axis->currentIndex();const double lo=bounds[2*a],hi=bounds[2*a+1];QSignalBlocker block(slider);slider->setValue(hi>lo?std::clamp(static_cast<int>((value-lo)/(hi-lo)*1000),1,999):500);preview.start();});
        QObject::connect(slider,&QSlider::valueChanged,&dialog,[&](int value){const int a=axis->currentIndex();position->setValue(bounds[2*a]+value/1000.0*(bounds[2*a+1]-bounds[2*a]));});
        auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);form->addRow(buttons);
        auto* full=buttons->addButton(tr("Show full domain"),QDialogButtonBox::ResetRole);
        QObject::connect(full,&QPushButton::clicked,&dialog,[&](){preview.stop();viewport->clearSlicePlane();viewport->resetCamera();dialog.accept();});
        QObject::connect(buttons,&QDialogButtonBox::accepted,&dialog,[&](){preview.stop();if(apply())dialog.accept();});
        QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);range();
        const bool accepted=dialog.exec()==QDialog::Accepted;preview.stop();
        if(!accepted){if(previous.enabled)viewport->setSlicePlane(previous.axis,previous.position);else viewport->clearSlicePlane();viewport->restoreCamera(camera);}
    }catch(const std::exception& error){QMessageBox::information(parent,tr("Section unavailable"),tr(error.what()));}
}
void showPointProbe(QWidget* parent,VtkViewport* viewport,const QString& selected) {
    try {
        const auto bounds=viewport->fieldBounds();QDialog dialog(parent);dialog.setObjectName("vortia-point-probe");dialog.setWindowTitle(tr("Probe a point"));dialog.resize(520,380);
        auto* form=new QFormLayout(&dialog);auto* note=new QLabel(tr("Interpolate a saved field at a point in metres. Points outside the fluid are marked invalid."),&dialog);note->setWordWrap(true);form->addRow(note);
        std::array<QDoubleSpinBox*,3> coordinates{};
        for(int i=0;i<3;++i){auto* value=new QDoubleSpinBox(&dialog);value->setObjectName(QString("probe-coordinate-%1").arg(i));value->setDecimals(9);value->setRange(-1e9,1e9);value->setValue((bounds[2*i]+bounds[2*i+1])/2);form->addRow(QString("%1 (m)").arg(QChar('X'+i)),value);coordinates[i]=value;}
        auto* field=fields(&dialog,viewport,selected);form->addRow(tr("Field"),field);
        auto* value=new QLabel(&dialog);value->setObjectName("probe-result");value->setTextInteractionFlags(Qt::TextSelectableByMouse);value->setWordWrap(true);value->setMinimumHeight(52);form->addRow(value);
        auto* query=new QPushButton(tr("Evaluate point"),&dialog);query->setObjectName("probe-evaluate");form->addRow(query);
        QObject::connect(query,&QPushButton::clicked,&dialog,[&](){try{const auto sample=viewport->probePoint({coordinates[0]->value(),coordinates[1]->value(),coordinates[2]->value()},field->currentData().toString().toStdString());
            value->setText(sample.valid?tr("%1\nValue: %2").arg(field->currentText(),QLocale().toString(sample.value,'g',10)):tr("Invalid point · outside the loaded fluid mesh"));
        }catch(const std::exception& error){value->setText(tr(error.what()));}});
        auto* buttons=new QDialogButtonBox(QDialogButtonBox::Close,&dialog);form->addRow(buttons);QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);query->click();dialog.exec();
    }catch(const std::exception& error){QMessageBox::information(parent,tr("Probe unavailable"),tr(error.what()));}
}
void exportViewportImage(QWidget* parent,VtkViewport* viewport) {
    const auto path=QFileDialog::getSaveFileName(parent,tr("Export CFD view"),{},tr("PNG image (*.png)"));if(path.isEmpty())return;
    QSaveFile output(path);const auto image=viewport->captureImage();
    if(image.isNull()||!output.open(QIODevice::WriteOnly)||!image.save(&output,"PNG")||!output.commit())QMessageBox::warning(parent,tr("Export failed"),tr("The view could not be saved. Choose a writable PNG file."));
}
}
