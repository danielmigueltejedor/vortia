#include "vortia/ui/LineProfile.hpp"
#include "vortia/visualization/VtkViewport.hpp"
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QSpinBox>
#include <QFormLayout>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QComboBox>
#include <QCheckBox>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QFileDialog>
#include <QSaveFile>
#include <QMessageBox>
#include <QTextStream>
#include <QLocale>
#include <limits>
#include <algorithm>
#include <cmath>
namespace vortia {
namespace {
QString profileTr(const char* s){return QCoreApplication::translate("vortia::LineProfile",s);}
class ProfilePlot final:public QWidget {
public:
    std::vector<LineSample> samples;
    int coordinate{0};
    bool verticalCoordinate{false};
    QString field;
    explicit ProfilePlot(QWidget* parent):QWidget(parent){setMinimumSize(660,360);}
    double abscissa(const LineSample& sample) const{return coordinate==0?sample.distance:sample.position[coordinate-1];}
    double xValue(const LineSample& sample) const {return verticalCoordinate?sample.value:abscissa(sample);}
    double yValue(const LineSample& sample) const {return verticalCoordinate?abscissa(sample):sample.value;}
protected:
    void paintEvent(QPaintEvent*) override {
        QPainter p(this);p.setRenderHint(QPainter::Antialiasing);p.fillRect(rect(),Qt::white);
        QRectF frame=QRectF(rect()).adjusted(85,45,-25,-65);
        double xmin=std::numeric_limits<double>::infinity(),xmax=-xmin,ymin=xmin,ymax=-xmin;
        for(const auto& sample:samples)if(sample.valid){xmin=std::min(xmin,xValue(sample));xmax=std::max(xmax,xValue(sample));ymin=std::min(ymin,yValue(sample));ymax=std::max(ymax,yValue(sample));}
        if(!std::isfinite(xmin)){p.setPen(Qt::black);p.drawText(frame,profileTr("No sampling points lie inside the loaded mesh."));return;}
        if(xmax==xmin)xmax=xmin+1e-9;
        if(ymax==ymin){const double pad=std::max(1e-9,std::abs(ymin)*.01);ymin-=pad;ymax+=pad;}
        p.setPen(QColor("#dddddd"));
        for(int i=0;i<=5;++i){const double t=i/5.0;p.drawLine(QPointF(frame.left()+t*frame.width(),frame.top()),QPointF(frame.left()+t*frame.width(),frame.bottom()));p.drawLine(QPointF(frame.left(),frame.top()+t*frame.height()),QPointF(frame.right(),frame.top()+t*frame.height()));}
        p.setPen(Qt::black);p.drawRect(frame);
        for(int i=0;i<=5;++i){const double t=i/5.0;
            p.drawText(QRectF(frame.left()+t*frame.width()-45,frame.bottom()+8,90,25),Qt::AlignHCenter,QString::number(xmin+t*(xmax-xmin),'g',5));
            p.drawText(QRectF(0,frame.bottom()-t*frame.height()-12,frame.left()-10,25),Qt::AlignRight,QString::number(ymin+t*(ymax-ymin),'g',5));
        }
        p.drawText(QRectF(frame.left(),5,frame.width(),30),Qt::AlignCenter,field);
        const QString xLabel=coordinate==0?profileTr("Distance (m)"):QString("%1 (m)").arg(coordinate==1?"X":coordinate==2?"Y":"Z");
        p.drawText(QRectF(frame.left(),height()-30,frame.width(),25),Qt::AlignCenter,verticalCoordinate?field:xLabel);
        p.save();p.translate(15,frame.center().y());p.rotate(-90);p.drawText(QRectF(-frame.height()/2,-12,frame.height(),25),Qt::AlignCenter,verticalCoordinate?xLabel:field);p.restore();
        QPainterPath path;bool penDown=false;
        for(const auto& sample:samples){
            if(!sample.valid){penDown=false;continue;}
            const QPointF point(frame.left()+(xValue(sample)-xmin)/(xmax-xmin)*frame.width(),frame.bottom()-(yValue(sample)-ymin)/(ymax-ymin)*frame.height());
            if(penDown)path.lineTo(point);else path.moveTo(point);penDown=true;
        }
        p.setPen(QPen(QColor("#147faf"),2));p.drawPath(path);
    }
};
}
void showLineProfile(QWidget* parent,VtkViewport* viewport,const QString& selectedField){
    try{
        const auto bounds=viewport->fieldBounds();
        QDialog input(parent);input.setObjectName("vortia-line-profile-input");input.setWindowTitle(profileTr("Sample a line"));
        auto* form=new QFormLayout(&input);
        auto* intro=new QLabel(profileTr("Enter two points in metres. Values outside the fluid mesh remain invalid and are not drawn as zero."),&input);intro->setWordWrap(true);form->addRow(intro);
        std::array<QDoubleSpinBox*,6> coordinates{};
        const char* labels[]={"Start X (m)","Start Y (m)","Start Z (m)","End X (m)","End Y (m)","End Z (m)"};
        for(int i=0;i<6;++i){auto* spin=new QDoubleSpinBox(&input);spin->setDecimals(9);spin->setRange(-1e9,1e9);spin->setObjectName(QString("profile-coordinate-%1").arg(i));
            const int axis=i%3;spin->setValue(axis==0?(i<3?bounds[0]:bounds[1]):(bounds[2*axis]+bounds[2*axis+1])/2);form->addRow(profileTr(labels[i]),spin);coordinates[static_cast<std::size_t>(i)]=spin;}
        auto* field=new QComboBox(&input);field->addItems({"p","T","Mach","U","Ux","Uy","Uz"});field->setCurrentText(selectedField=="|U|"?"U":selectedField);form->addRow(profileTr("Field"),field);
        auto* points=new QSpinBox(&input);points->setRange(2,10000);points->setValue(161);form->addRow(profileTr("Sampling points"),points);
        auto* axis=new QComboBox(&input);axis->addItems({profileTr("Distance"),"X","Y","Z"});form->addRow(profileTr("Coordinate"),axis);
        auto* vertical=new QCheckBox(profileTr("Place the coordinate on the vertical axis"),&input);form->addRow(vertical);
        auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&input);form->addRow(buttons);
        QObject::connect(buttons,&QDialogButtonBox::accepted,&input,&QDialog::accept);QObject::connect(buttons,&QDialogButtonBox::rejected,&input,&QDialog::reject);
        if(input.exec()!=QDialog::Accepted)return;
        const Vec3 start{coordinates[0]->value(),coordinates[1]->value(),coordinates[2]->value()};
        const Vec3 end{coordinates[3]->value(),coordinates[4]->value(),coordinates[5]->value()};
        auto samples=viewport->sampleLine(start,end,points->value(),field->currentText().toStdString());
        QDialog output(parent);output.setWindowTitle(profileTr("Line profile"));output.resize(900,570);
        auto* layout=new QVBoxLayout(&output);
        auto* plot=new ProfilePlot(&output);plot->samples=std::move(samples);plot->coordinate=axis->currentIndex();plot->verticalCoordinate=vertical->isChecked();plot->field=field->currentText()+(field->currentText()=="p"?" (Pa)":field->currentText()=="T"?" (K)":field->currentText().startsWith("U")?" (m/s)":"");layout->addWidget(plot,1);
        const auto valid=std::count_if(plot->samples.cbegin(),plot->samples.cend(),[](const auto& s){return s.valid;});
        auto* note=new QLabel(profileTr("%1 of %2 points are valid. This interpolated profile does not establish CFD convergence.").arg(static_cast<qlonglong>(valid)).arg(points->value()),&output);note->setWordWrap(true);layout->addWidget(note);
        auto* actions=new QHBoxLayout;layout->addLayout(actions);
        auto* csv=new QPushButton(profileTr("Export CSV…"),&output);auto* png=new QPushButton(profileTr("Save graph…"),&output);auto* close=new QPushButton(profileTr("Close"),&output);actions->addWidget(csv);actions->addWidget(png);actions->addStretch();actions->addWidget(close);
        QObject::connect(close,&QPushButton::clicked,&output,&QDialog::accept);
        QObject::connect(csv,&QPushButton::clicked,&output,[&](){
            const auto path=QFileDialog::getSaveFileName(&output,profileTr("Export profile"),{},"CSV (*.csv)");if(path.isEmpty())return;
            QSaveFile file(path);if(!file.open(QIODevice::WriteOnly))return;QTextStream stream(&file);stream.setLocale(QLocale::c());stream.setRealNumberPrecision(12);
            stream<<"distance_m,x_m,y_m,z_m,"<<field->currentText()<<",valid\n";
            for(const auto& s:plot->samples){stream<<s.distance<<','<<s.position.x<<','<<s.position.y<<','<<s.position.z<<',';if(s.valid)stream<<s.value;stream<<','<<(s.valid?1:0)<<'\n';}
            stream.flush();if(!file.commit())QMessageBox::warning(&output,profileTr("Export failed"),file.errorString());
        });
        QObject::connect(png,&QPushButton::clicked,&output,[&](){const auto path=QFileDialog::getSaveFileName(&output,profileTr("Save graph"),{},"PNG (*.png)");if(!path.isEmpty()&&!plot->grab().save(path))QMessageBox::warning(&output,profileTr("Export failed"),path);});
        output.exec();
    }catch(const std::exception& error){QMessageBox::information(parent,profileTr("Profile unavailable"),profileTr(error.what()));}
}
}
