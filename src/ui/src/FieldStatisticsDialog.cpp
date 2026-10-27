#include "vortia/ui/FieldTools.hpp"
#include "vortia/visualization/VtkViewport.hpp"
#include <QCoreApplication>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFormLayout>
#include <QHeaderView>
#include <QLabel>
#include <QLocale>
#include <QPainter>
#include <QPushButton>
#include <QSaveFile>
#include <QTableWidget>
#include <QTextStream>
#include <QVBoxLayout>
#include <algorithm>
namespace vortia {
namespace {
QString statsTr(const char* text){return QCoreApplication::translate("vortia::FieldTools",text);}
class HistogramPlot final:public QWidget {
public:
    FieldStatistics values;
    explicit HistogramPlot(QWidget* parent):QWidget(parent){setMinimumHeight(210);setObjectName("field-histogram");}
protected:
    void paintEvent(QPaintEvent*) override {
        QPainter painter(this);painter.setRenderHint(QPainter::Antialiasing);painter.fillRect(rect(),palette().brush(QPalette::Base));if(values.histogram.empty())return;
        const QRectF plot=QRectF(rect()).adjusted(65,22,-22,-40);double maximum=0;for(const auto& bin:values.histogram)maximum=std::max(maximum,100*bin.measure/values.measure);
        painter.setPen(palette().color(QPalette::Mid));painter.drawLine(plot.bottomLeft(),plot.topLeft());painter.drawLine(plot.bottomLeft(),plot.bottomRight());const double width=plot.width()/static_cast<double>(values.histogram.size());
        painter.setPen(Qt::NoPen);painter.setBrush(palette().color(QPalette::Highlight));for(std::size_t i=0;i<values.histogram.size();++i){const double height=maximum>0?plot.height()*(100*values.histogram[i].measure/values.measure)/maximum:0;painter.drawRoundedRect(QRectF(plot.left()+static_cast<double>(i)*width+1,plot.bottom()-height,std::max(1.0,width-2),height),2,2);}
        painter.setPen(palette().color(QPalette::Text));const QLocale locale;painter.drawText(QRectF(0,plot.top()-8,58,25),Qt::AlignRight,locale.toString(maximum,'g',4)+"%");painter.drawText(QRectF(0,plot.bottom()-12,58,25),Qt::AlignRight,"0%");painter.drawText(QRectF(plot.left(),plot.bottom()+10,plot.width()/2,25),Qt::AlignLeft,locale.toString(values.minimum,'g',6));painter.drawText(QRectF(plot.center().x(),plot.bottom()+10,plot.width()/2,25),Qt::AlignRight,locale.toString(values.maximum,'g',6));
    }
};
}
void showFieldStatistics(QWidget* parent,VtkViewport* viewport,const QString& selected){
    QDialog dialog(parent);dialog.setObjectName("vortia-field-statistics");dialog.setWindowTitle(statsTr("Statistics and histogram"));dialog.resize(740,790);auto* layout=new QVBoxLayout(&dialog);
    auto* note=new QLabel(statsTr("Reports use the original calculated fluid block, including the original wedge for axisymmetric cases. The 360° display does not multiply report totals. Histogram bins are weighted by cell volume or area."),&dialog);note->setWordWrap(true);layout->addWidget(note);
    auto* fields=new QComboBox(&dialog);fields->setObjectName("statistics-field");for(const auto& name:viewport->availableFields()){const auto key=QString::fromStdString(name);fields->addItem(fieldDisplayName(key),key);}const int index=fields->findData(selected);if(index>=0)fields->setCurrentIndex(index);layout->addWidget(fields);
    auto* table=new QTableWidget(8,2,&dialog);table->setObjectName("statistics-values");table->setHorizontalHeaderLabels({statsTr("Measure"),statsTr("Value")});table->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);table->verticalHeader()->hide();table->setEditTriggers(QAbstractItemView::NoEditTriggers);table->verticalHeader()->setDefaultSectionSize(30);table->verticalHeader()->setSectionResizeMode(QHeaderView::Fixed);table->setFixedHeight(8*30+table->horizontalHeader()->height()+2*table->frameWidth());layout->addWidget(table);
    auto* plot=new HistogramPlot(&dialog);layout->addWidget(plot,1);auto* status=new QLabel(&dialog);status->setObjectName("statistics-status");status->setWordWrap(true);layout->addWidget(status);
    auto* buttons=new QDialogButtonBox(QDialogButtonBox::Close,&dialog);auto* csv=buttons->addButton(statsTr("Export report CSV…"),QDialogButtonBox::ActionRole);csv->setObjectName("statistics-export");layout->addWidget(buttons);QObject::connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    const auto update=[&](){try{plot->values=viewport->statistics(fields->currentData().toString().toStdString());const auto& v=plot->values;
        const QString measure=v.dimension==3?statsTr("Calculated volume (m³)"):v.dimension==2?statsTr("Calculated area (m²)"):statsTr("Calculated length (m)");
        const std::array<QString,8> names{statsTr("Minimum cell value"),statsTr("Maximum cell value"),statsTr("Weighted mean"),statsTr("Weighted standard deviation"),measure,statsTr("Field integral over calculated measure"),statsTr("Valid cells"),statsTr("Excluded cells")};
        const std::array<double,8> values{v.minimum,v.maximum,v.mean,v.standardDeviation,v.measure,v.integral,static_cast<double>(v.cells),static_cast<double>(v.excluded)};
        for(int row=0;row<8;++row){table->setItem(row,0,new QTableWidgetItem(names[static_cast<std::size_t>(row)]));table->setItem(row,1,new QTableWidgetItem(QLocale().toString(values[static_cast<std::size_t>(row)],'g',11)));}
        status->setText(v.pointAverages?statsTr("Cell values are averages of the stored point values. Non-finite, ghost and zero-measure cells are excluded. This report does not establish convergence."):statsTr("Uses stored cell values. Non-finite, ghost and zero-measure cells are excluded. This report does not establish convergence."));csv->setEnabled(true);
    }catch(const std::exception& error){plot->values={};table->clearContents();status->setText(statsTr(error.what()));csv->setEnabled(false);}plot->update();};
    QObject::connect(fields,qOverload<int>(&QComboBox::currentIndexChanged),&dialog,[&](int){update();});
    QObject::connect(csv,&QPushButton::clicked,&dialog,[&](){const auto path=QFileDialog::getSaveFileName(&dialog,statsTr("Export statistics"),{},"CSV (*.csv)");if(path.isEmpty())return;QSaveFile file(path);if(!file.open(QIODevice::WriteOnly)){status->setText(statsTr("The report could not be saved."));return;}QTextStream stream(&file);stream.setLocale(QLocale::c());stream.setRealNumberPrecision(17);const auto& v=plot->values;QString field=fields->currentData().toString();field.replace('"',"\"\"");stream<<"scope,association,field,dimension,valid_cells,excluded_cells,min,max,weighted_mean,weighted_std,measure,field_integral\noriginal_calculated_block,"<<(v.pointAverages?"mean_of_point_values":"stored_cell_values")<<",\""<<field<<"\","<<v.dimension<<','<<v.cells<<','<<v.excluded<<','<<v.minimum<<','<<v.maximum<<','<<v.mean<<','<<v.standardDeviation<<','<<v.measure<<','<<v.integral<<"\n\nlower,upper,cells,measure,measure_fraction\n";for(const auto& bin:v.histogram)stream<<bin.low<<','<<bin.high<<','<<bin.cells<<','<<bin.measure<<','<<bin.measure/v.measure<<'\n';stream.flush();if(!file.commit())status->setText(statsTr("The report could not be saved."));});update();dialog.exec();
}
}
