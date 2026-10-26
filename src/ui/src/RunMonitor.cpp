#include "vortia/ui/RunMonitor.hpp"
#include <QCoreApplication>
#include <QPainter>
#include <QPainterPath>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QNativeGestureEvent>
#include <QKeyEvent>
#include <QToolTip>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QLabel>
#include <QComboBox>
#include <QCheckBox>
#include <QDoubleSpinBox>
#include <QPushButton>
#include <QToolButton>
#include <QTabWidget>
#include <QTableWidget>
#include <QScrollArea>
#include <QHeaderView>
#include <QTimer>
#include <QMenu>
#include <QAction>
#include <QJsonDocument>
#include <QJsonArray>
#include <QSignalBlocker>
#include <QDateTime>
#include <QFileInfo>
#include <QDir>
#include <QFileDialog>
#include <QSaveFile>
#include <QTextStream>
#include <QMessageBox>
#include <QLocale>
#include <fstream>
#include <algorithm>
#include <cmath>
#include <set>
#include <array>
#include <limits>
namespace vortia {
namespace {
QString monitorText(const char* s){return QCoreApplication::translate("vortia::RunMonitor",s);}
QString seriesName(const std::string& key){
    if(key.starts_with("residual.initial."))return monitorText("Initial · %1").arg(QString::fromStdString(key.substr(17)));
    if(key.starts_with("residual.final."))return monitorText("Final · %1").arg(QString::fromStdString(key.substr(15)));
    const std::map<std::string,const char*> names{{"mass.global","Global imbalance"},{"mass.local","Local imbalance"},{"mass.tolerance","Mass tolerance"},{"field.change","Field change"},{"field.tolerance","Field tolerance"},{"equation.error","Equation update indicator"},{"equation.tolerance","Equation tolerance"},{"flow.incoming","All open boundaries · incoming"},{"flow.outgoing","All open boundaries · outgoing"},{"courant.mean","Mean Courant"},{"courant.max","Maximum Courant"},{"runtime.cpu","Reported CPU time"},{"runtime.clock","Reported wall time"},{"runtime.rate","Solver coordinate / wall second"},{"stability.samples","Consecutive accepted samples"},{"stability.required","Required samples"}};
    const auto found=names.find(key);return found==names.end()?QString::fromStdString(key):monitorText(found->second);
}
QString rawUnit(const std::string& key){if(key.starts_with("flow."))return "kg/s";if(key=="runtime.cpu"||key=="runtime.clock")return "s";if(key=="runtime.rate")return "solver_coordinate/s";if(key.starts_with("stability."))return "samples";return "1";}
double scale(const std::string& key){return key.starts_with("mass.")||key.starts_with("field.")?100:1;}
bool tolerance(const std::string& key){return key.ends_with("tolerance")||key=="stability.required";}
const std::array<QColor,8> colors{QColor("#219dcb"),QColor("#ec9254"),QColor("#8c79d9"),QColor("#43b98f"),QColor("#d66897"),QColor("#8eac3c"),QColor("#608fda"),QColor("#b08860")};
void reduce(std::vector<MonitorPoint>& points){
    if(points.size()<=8192)return;
    std::vector<MonitorPoint> next;next.reserve(points.size()/2+16);
    for(std::size_t start=0;start<points.size();){
        std::size_t end=start+1;while(end<points.size()&&points[end].session==points[start].session&&end<start+8)++end;
        if(end-start<8)for(auto i=start;i<end;++i)next.push_back(points[i]);
        else {std::set<std::size_t> keep{start,end-1};auto lo=start,hi=start;for(auto i=start+1;i<end;++i){if(points[i].y<points[lo].y)lo=i;if(points[i].y>points[hi].y)hi=i;}keep.insert(lo);keep.insert(hi);for(auto i:keep)next.push_back(points[i]);}
        start=end;
    }
    // A run at every point, or alternating extrema, cannot be reduced further.
    if(next.size()>8192)next.erase(next.begin(),next.end()-8192);
    points=std::move(next);
}
}
void MonitorHistory::append(const std::vector<openfoam::MonitorSample>& samples,int offset){
    for(const auto& s:samples){if(s.series.empty()||s.series.size()>128||!std::isfinite(s.coordinate)||s.coordinate<0||!std::isfinite(s.value)||s.value<0||s.session<0)continue;
        if(!series.contains(s.series)&&series.size()>=64){reduced=true;continue;}
        auto& points=series[s.series];const int session=s.session+offset;
        if(!points.empty()&&points.back().x==s.coordinate&&points.back().session==session)points.back().y=s.value;
        else points.push_back({s.coordinate,s.value,session});
        ++observations;if(points.size()>8192){reduce(points);reduced=true;}
    }
}
int MonitorHistory::latestSession()const{int latest=0;for(const auto& [key,points]:series)if(!points.empty())latest=std::max(latest,points.back().session);return latest;}
bool MonitorHistory::writeCsv(const QString& path)const{
    QSaveFile file(path);if(!file.open(QIODevice::WriteOnly))return false;QTextStream text(&file);text.setLocale(QLocale::c());text.setRealNumberPrecision(17);
    text<<"session,solver_coordinate,series,value,unit\n";
    for(const auto& [key,points]:series)for(const auto& p:points)text<<p.session<<','<<p.x<<','<<QString::fromStdString(key)<<','<<p.y<<','<<rawUnit(key)<<'\n';
    text.flush();return text.status()==QTextStream::Ok&&file.commit();
}
class RunPlot final:public QWidget {
public:
    explicit RunPlot(QWidget* parent=nullptr):QWidget(parent){setMinimumSize(270,210);setMouseTracking(true);setFocusPolicy(Qt::StrongFocus);setToolTip(monitorText("Pinch or wheel to zoom horizontally · drag or swipe to pan · double click or F to fit"));}
    const MonitorHistory* data=nullptr;std::vector<std::string> keys;QString title,axis;
    bool logarithmic=true,compact=false;int session=-1;double lastWindow=0;
    void fit(){manual_=false;update();}
protected:
    void paintEvent(QPaintEvent*)override{
        QPainter p(this);p.setRenderHint(QPainter::Antialiasing);p.fillRect(rect(),palette().color(QPalette::Base));
        p.setPen(palette().color(QPalette::Mid));p.drawRoundedRect(rect().adjusted(1,1,-1,-1),9,9);
        auto font=p.font();font.setBold(true);p.setFont(font);p.setPen(palette().color(QPalette::Text));p.drawText(QRect(15,10,width()-30,23),Qt::AlignLeft|Qt::AlignVCenter,title);font.setBold(false);font.setPointSizeF(std::max(8.0,font.pointSizeF()-1));p.setFont(font);
        const int columns=std::max(1,(width()-30)/200),legendRows=static_cast<int>((keys.size()+static_cast<std::size_t>(columns)-1)/static_cast<std::size_t>(columns));
        for(std::size_t i=0;i<keys.size();++i){const double x=15+static_cast<int>(i%static_cast<std::size_t>(columns))*(width()-30)/columns,y=45+static_cast<int>(i/static_cast<std::size_t>(columns))*18;p.setPen(QPen(colors[i%colors.size()],2,tolerance(keys[i])?Qt::DashLine:Qt::SolidLine));p.drawLine(QPointF(x,y),QPointF(x+16,y));p.setPen(palette().color(QPalette::Text));p.drawText(QRectF(x+22,y-8,(width()-30)/columns-25,18),p.fontMetrics().elidedText(seriesName(keys[i]),Qt::ElideRight,(width()-30)/columns-25));}
        area_=QRectF(78,52+legendRows*18,std::max(1,width()-102),std::max(1,height()-109-legendRows*18));
        bool found=false;double lo=std::numeric_limits<double>::infinity(),hi=-lo;
        if(data)for(const auto& key:keys)if(const auto it=data->series.find(key);it!=data->series.end())for(const auto& v:it->second)if(session<0||v.session==session){lo=std::min(lo,v.x);hi=std::max(hi,v.x);found=true;}
        if(!found){p.setPen(palette().color(QPalette::PlaceholderText));p.drawText(area_,Qt::AlignCenter|Qt::TextWordWrap,monitorText("No reported samples for this view. Missing metrics are not replaced by zero."));return;}
        if(hi<=lo)hi=lo+1;
        if(manual_){lo=manualLo_;hi=manualHi_;}else if(lastWindow>0)lo=std::max(lo,hi-lastWindow);
        xLo_=lo;xHi_=hi;double bottom=std::numeric_limits<double>::infinity(),top=-bottom;
        const auto transformed=[&](const std::string& key,double value){const double y=value*scale(key);return logarithmic?std::log10(std::max(1e-16,y)):y;};
        for(const auto& key:keys)if(const auto it=data->series.find(key);it!=data->series.end())for(const auto& v:it->second)if((session<0||v.session==session)&&v.x>=lo&&v.x<=hi){const double y=transformed(key,v.y);bottom=std::min(bottom,y);top=std::max(top,y);}
        if(!std::isfinite(bottom)){p.drawText(area_,Qt::AlignCenter,monitorText("No samples in the selected range. Double click to fit."));return;}
        if(top-bottom<1e-12){bottom-=logarithmic?.5:std::max(1e-9,std::abs(bottom)*.05);top+=logarithmic?.5:std::max(1e-9,std::abs(top)*.05);}
        else{const double padding=(top-bottom)*.08;top+=padding;bottom-=padding;}
        if(logarithmic){bottom=std::max(-17.0,bottom);top=std::min(302.0,top);}
        for(int i=0;i<=4;++i){const double fraction=i/4.0,y=area_.bottom()-fraction*area_.height();p.setPen(QPen(palette().color(QPalette::Mid),.6,Qt::DotLine));p.drawLine(QPointF(area_.left(),y),QPointF(area_.right(),y));p.setPen(palette().color(QPalette::Text));const double value=bottom+fraction*(top-bottom);p.drawText(QRectF(3,y-9,69,18),Qt::AlignRight|Qt::AlignVCenter,QString::number(logarithmic?std::pow(10.0,value):value,'g',3));const double x=area_.left()+fraction*area_.width();p.drawText(QRectF(x-40,area_.bottom()+7,80,18),Qt::AlignCenter,QString::number(lo+fraction*(hi-lo),'g',5));}
        p.setPen(palette().color(QPalette::Text));p.drawText(QRectF(area_.left(),height()-25,area_.width(),19),Qt::AlignCenter,axis);
        p.save();p.setClipRect(area_);
        for(std::size_t i=0;i<keys.size();++i)if(const auto it=data->series.find(keys[i]);it!=data->series.end()){
            QPainterPath path;bool first=true;int lastSession=-1;QPointF single;int count=0;
            for(const auto& v:it->second){if((session>=0&&v.session!=session)||v.x<lo||v.x>hi){first=true;continue;}
                const QPointF at(area_.left()+(v.x-lo)/(hi-lo)*area_.width(),area_.bottom()-(transformed(keys[i],v.y)-bottom)/(top-bottom)*area_.height());
                if(first||v.session!=lastSession)path.moveTo(at);else path.lineTo(at);first=false;lastSession=v.session;single=at;++count;
            }
            p.setPen(QPen(colors[i%colors.size()],tolerance(keys[i])?1.3:1.8,tolerance(keys[i])?Qt::DashLine:Qt::SolidLine));p.drawPath(path);if(count==1){p.setBrush(colors[i%colors.size()]);p.drawEllipse(single,3,3);}
        }
        p.restore();
    }
    void mousePressEvent(QMouseEvent* e)override{if(e->button()==Qt::LeftButton){drag_=true;previous_=e->position();setFocus();e->accept();}}
    void mouseReleaseEvent(QMouseEvent* e)override{drag_=false;e->accept();}
    void leaveEvent(QEvent*)override{drag_=false;}
    void mouseDoubleClickEvent(QMouseEvent* e)override{drag_=false;fit();e->accept();}
    void mouseMoveEvent(QMouseEvent* e)override{
        if(drag_&&(e->buttons()&Qt::LeftButton)){pan(e->position().x()-previous_.x());previous_=e->position();e->accept();return;}
        if(!data||!area_.contains(e->position()))return;const double x=xLo_+(e->position().x()-area_.left())/area_.width()*(xHi_-xLo_);QStringList lines;
        for(const auto& key:keys)if(const auto it=data->series.find(key);it!=data->series.end()){const MonitorPoint* nearest=nullptr;double closest=std::numeric_limits<double>::infinity();for(const auto& point:it->second)if((session<0||point.session==session)&&std::abs(point.x-x)<closest){closest=std::abs(point.x-x);nearest=&point;}if(nearest)lines<<seriesName(key)+" · "+QString::number(nearest->x,'g',8)+": "+QString::number(nearest->y*scale(key),'g',10);if(lines.size()>=8)break;}
        if(!lines.isEmpty())QToolTip::showText(e->globalPosition().toPoint(),monitorText("Nearest recorded samples (not interpolated)")+"\n"+lines.join("\n"),this);
    }
    void wheelEvent(QWheelEvent* e)override{if(!e->pixelDelta().isNull()&&!(e->modifiers()&(Qt::ControlModifier|Qt::MetaModifier)))pan(e->pixelDelta().x()!=0?e->pixelDelta().x():e->pixelDelta().y());else zoom(std::pow(1.2,std::clamp(e->angleDelta().y()/120.0,-5.0,5.0)),e->position().x());e->accept();}
    bool event(QEvent* e)override{if(e->type()==QEvent::NativeGesture){auto* g=static_cast<QNativeGestureEvent*>(e);if(g->gestureType()==Qt::ZoomNativeGesture)zoom(1+g->value(),g->position().x());else if(g->gestureType()==Qt::PanNativeGesture)pan(g->delta().x());e->accept();return true;}if(e->type()==QEvent::FocusOut)drag_=false;return QWidget::event(e);}
    void keyPressEvent(QKeyEvent* e)override{if(e->key()==Qt::Key_F){fit();e->accept();}else QWidget::keyPressEvent(e);}
private:
    void zoom(double factor,double anchor){if(!std::isfinite(factor)||factor<=0||xHi_<=xLo_)return;factor=std::clamp(factor,.2,5.0);const double fraction=std::clamp((anchor-area_.left())/std::max(1.0,area_.width()),0.0,1.0),span=std::clamp((xHi_-xLo_)/factor,1e-9,1e15),x=xLo_+fraction*(xHi_-xLo_);manualLo_=x-fraction*span;manualHi_=manualLo_+span;manual_=true;update();}
    void pan(double pixels){if(!std::isfinite(pixels)||xHi_<=xLo_)return;const double delta=-pixels/std::max(1.0,area_.width())*(xHi_-xLo_);manualLo_=xLo_+delta;manualHi_=xHi_+delta;manual_=true;update();}
    QRectF area_;double xLo_=0,xHi_=1,manualLo_=0,manualHi_=1;bool manual_=false,drag_=false;QPointF previous_;
};
RunMonitor::RunMonitor(QWidget* parent):QWidget(parent),history_(std::make_shared<MonitorHistory>()){
    setObjectName("vortia-run-monitor");auto* root=new QVBoxLayout(this);root->setContentsMargins(14,12,14,12);
    source_=new QLabel(monitorText("No calculation case selected."),this);source_->setObjectName("monitor-source");source_->setTextInteractionFlags(Qt::TextSelectableByMouse);source_->setWordWrap(true);root->addWidget(source_);
    auto* tools=new QHBoxLayout;
    group_=new QComboBox(this);group_->setObjectName("monitor-group");group_->addItems({monitorText("Initial residuals"),monitorText("Final residuals"),monitorText("Mass imbalance (%)"),monitorText("Field change (%)"),monitorText("Equation update indicator"),monitorText("Open-boundary flow (kg/s)"),monitorText("Courant number"),monitorText("CPU / wall time (s)"),monitorText("Solver advance per second"),monitorText("Stable sample window")});tools->addWidget(group_);
    sessions_=new QComboBox(this);sessions_->setObjectName("monitor-session");sessions_->addItem(monitorText("All runs"),-1);tools->addWidget(sessions_);
    logarithmic_=new QCheckBox(monitorText("Logarithmic Y"),this);logarithmic_->setObjectName("monitor-logarithmic");logarithmic_->setChecked(true);tools->addWidget(logarithmic_);
    paused_=new QCheckBox(monitorText("Freeze view"),this);paused_->setObjectName("monitor-pause");tools->addWidget(paused_);tools->addStretch();root->addLayout(tools);
    auto* options=new QHBoxLayout;auto* intervalLabel=new QLabel(monitorText("Last coordinate interval (0 = all)"),this);intervalLabel->setWordWrap(true);intervalLabel->setMinimumWidth(140);options->addWidget(intervalLabel);window_=new QDoubleSpinBox(this);window_->setObjectName("monitor-window");window_->setRange(0,1e9);window_->setDecimals(5);window_->setSingleStep(100);options->addWidget(window_);
    auto* fit=new QPushButton(monitorText("Fit plots"),this);fit->setIcon(QIcon(":/vortia/icons/fit.svg"));options->addWidget(fit);
    auto* curves=new QToolButton(this);curves->setText(monitorText("Curves"));curves->setIcon(QIcon(":/vortia/icons/chart.svg"));curves->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);curves->setPopupMode(QToolButton::InstantPopup);auto* menu=new QMenu(curves);curves->setMenu(menu);options->addWidget(curves);options->addStretch();
    auto* csv=new QPushButton(monitorText("Export CSV…"),this);csv->setObjectName("monitor-export-csv");csv->setIcon(QIcon(":/vortia/icons/save.svg"));options->addWidget(csv);
    auto* image=new QPushButton(monitorText("Save plot PNG…"),this);image->setObjectName("monitor-export-png");image->setIcon(QIcon(":/vortia/icons/export-image.svg"));options->addWidget(image);root->addLayout(options);
    views_=new QTabWidget(this);views_->setObjectName("monitor-views");auto* dashboard=new QWidget(views_);auto* grid=new QGridLayout(dashboard);grid->setContentsMargins(0,8,0,0);
    for(int i=0;i<4;++i){auto* graph=new RunPlot(dashboard);graph->setObjectName(QString("monitor-overview-%1").arg(i));graph->compact=true;overview_.push_back(graph);grid->addWidget(graph,i/2,i%2);}auto* dashboardScroll=new QScrollArea(views_);dashboardScroll->setWidgetResizable(true);dashboardScroll->setFrameShape(QFrame::NoFrame);dashboardScroll->setWidget(dashboard);views_->addTab(dashboardScroll,monitorText("Overview"));
    plot_=new RunPlot(views_);plot_->setObjectName("monitor-detail-plot");auto* detailScroll=new QScrollArea(views_);detailScroll->setFrameShape(QFrame::NoFrame);detailScroll->setWidgetResizable(true);detailScroll->setWidget(plot_);views_->addTab(detailScroll,monitorText("Detailed graph"));
    table_=new QTableWidget(views_);table_->setObjectName("monitor-table");table_->setColumnCount(5);table_->setHorizontalHeaderLabels({monitorText("Run"),monitorText("Solver coordinate"),monitorText("Series"),monitorText("Raw value"),monitorText("Unit")});table_->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);table_->setEditTriggers(QAbstractItemView::NoEditTriggers);table_->setAlternatingRowColors(true);views_->addTab(table_,monitorText("Recent data"));root->addWidget(views_,1);
    latest_=new QLabel(this);latest_->setObjectName("monitor-latest");latest_->setWordWrap(true);latest_->setTextInteractionFlags(Qt::TextSelectableByMouse);root->addWidget(latest_);
    summary_=new QLabel(this);summary_->setObjectName("monitor-summary");root->addWidget(summary_);
    note_=new QLabel(monitorText("Recorded observations only; no synthetic intermediate iterations. Initial/final linear-solver residuals and the equation update indicator are different measures. Low residuals alone do not validate the flow. Log plots clip zero at 1e−16; tables/CSV retain zero. Flow totals cover all open boundaries of the computed domain, including any axisymmetric sector; they are not nozzle thrust or full-revolution inlet flow."),this);note_->setWordWrap(true);note_->hide();auto* interpretation=new QToolButton(this);interpretation->setText(monitorText("About these measures"));interpretation->setCheckable(true);interpretation->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);interpretation->setArrowType(Qt::RightArrow);root->addWidget(interpretation);root->addWidget(note_);connect(interpretation,&QToolButton::toggled,note_,[note=note_,interpretation](bool checked){note->setVisible(checked);interpretation->setArrowType(checked?Qt::DownArrow:Qt::RightArrow);});logarithmic_->setToolTip(monitorText("Zero is displayed at 1e−16 on a logarithmic axis. The data table and CSV retain the actual value."));
    connect(group_,qOverload<int>(&QComboBox::currentIndexChanged),this,[this](){plot_->fit();dirty_=true;});
    connect(sessions_,qOverload<int>(&QComboBox::currentIndexChanged),this,[this](){plot_->fit();for(auto* graph:overview_)graph->fit();dirty_=true;});
    connect(logarithmic_,&QCheckBox::toggled,this,[this](){dirty_=true;});connect(window_,qOverload<double>(&QDoubleSpinBox::valueChanged),this,[this](){plot_->fit();for(auto* graph:overview_)graph->fit();dirty_=true;});
    connect(paused_,&QCheckBox::toggled,this,&RunMonitor::setPaused);connect(fit,&QPushButton::clicked,this,[this](){plot_->fit();for(auto* graph:overview_)graph->fit();dirty_=true;});
    connect(views_,&QTabWidget::currentChanged,this,[this](){dirty_=true;});
    connect(menu,&QMenu::aboutToShow,this,[this,menu](){menu->clear();const auto keys=keysFor(group_->currentIndex());for(const auto& key:keys){auto* action=menu->addAction(seriesName(key));action->setCheckable(true);action->setChecked(!hidden_.contains(QString::fromStdString(key)));connect(action,&QAction::toggled,this,[this,key](bool checked){if(checked)hidden_.remove(QString::fromStdString(key));else hidden_.insert(QString::fromStdString(key));dirty_=true;});}if(keys.empty())menu->addAction(monitorText("No reported curves"))->setEnabled(false);});
    connect(csv,&QPushButton::clicked,this,[this](){const auto path=QFileDialog::getSaveFileName(this,monitorText("Export monitor history"),{},"CSV (*.csv)");if(!path.isEmpty()&&!exportCsv(path))QMessageBox::warning(this,monitorText("Export failed"),monitorText("The monitor history could not be saved."));});
    connect(image,&QPushButton::clicked,this,[this](){const auto path=QFileDialog::getSaveFileName(this,monitorText("Save monitor plot"),{},"PNG (*.png)");if(path.isEmpty())return;QWidget* graph=views_->currentIndex()==0?qobject_cast<QScrollArea*>(views_->widget(0))->widget():plot_;if(views_->currentIndex()==2){views_->setCurrentIndex(1);refresh();}QSaveFile output(path);if(!output.open(QIODevice::WriteOnly)||!graph->grab().save(&output,"PNG")||!output.commit())QMessageBox::warning(this,monitorText("Export failed"),monitorText("The plot image could not be saved."));});
    redraw_=new QTimer(this);redraw_->setInterval(200);connect(redraw_,&QTimer::timeout,this,[this](){if(dirty_&&isVisible())refresh();});redraw_->start();refresh();
}
RunMonitor::~RunMonitor(){readerStop_.store(true);if(reader_.joinable())reader_.join();}
void RunMonitor::setPaused(bool value){frozen_=value?std::make_shared<MonitorHistory>(*history_):nullptr;dirty_=true;refresh();}
std::vector<std::string> RunMonitor::keysFor(int group)const{
    const auto* h=frozen_?frozen_.get():history_.get();std::vector<std::string> keys;
    for(const auto& [key,points]:h->series){bool match=false;switch(group){case 0:match=key.starts_with("residual.initial.");break;case 1:match=key.starts_with("residual.final.");break;case 2:match=key.starts_with("mass.");break;case 3:match=key.starts_with("field.");break;case 4:match=key.starts_with("equation.");break;case 5:match=key.starts_with("flow.");break;case 6:match=key.starts_with("courant.");break;case 7:match=key=="runtime.cpu"||key=="runtime.clock";break;case 8:match=key=="runtime.rate";break;case 9:match=key.starts_with("stability.");break;default:break;}if(match&&!points.empty())keys.push_back(key);}
    return keys;
}
void RunMonitor::refreshSessions(){
    const auto* h=frozen_?frozen_.get():history_.get();std::set<int> all;for(const auto& [key,points]:h->series)for(const auto& p:points)all.insert(p.session);
    const auto selected=sessions_->currentData().toInt();const bool followLatest=sessions_->count()==1||selected==sessions_->itemData(sessions_->count()-1).toInt();QSignalBlocker block(sessions_);sessions_->clear();sessions_->addItem(monitorText("All runs"),-1);for(const int run:all)sessions_->addItem(monitorText("Run %1").arg(run),run);
    const int found=sessions_->findData(selected);sessions_->setCurrentIndex(followLatest&&sessions_->count()>1?sessions_->count()-1:std::max(0,found));
}
void RunMonitor::refresh(){
    dirty_=false;refreshSessions();const auto* h=frozen_?frozen_.get():history_.get();const int session=sessions_->currentData().toInt();
    const auto configure=[&](RunPlot* graph,int group,const QString& title){graph->data=h;graph->keys=keysFor(group);std::erase_if(graph->keys,[this](const auto& key){return hidden_.contains(QString::fromStdString(key));});if(graph->compact&&graph->keys.size()>4)graph->keys.resize(4);const int columns=std::max(1,(graph->width()-30)/200);const int rows=static_cast<int>((graph->keys.size()+static_cast<std::size_t>(columns)-1)/static_cast<std::size_t>(columns));graph->setMinimumHeight((graph->compact?190:260)+rows*18);graph->title=title;graph->axis=axisLabel_.isEmpty()?monitorText("Solver coordinate (Time)"):axisLabel_;graph->session=session;graph->lastWindow=window_->value();graph->logarithmic=logarithmic_->isChecked()&&group!=9;graph->update();};
    configure(plot_,group_->currentIndex(),group_->currentText());const std::array<int,4> groups{keysFor(0).empty()?6:0,2,3,4};for(int i=0;i<4;++i)configure(overview_[static_cast<std::size_t>(i)],groups[static_cast<std::size_t>(i)],group_->itemText(groups[static_cast<std::size_t>(i)]));
    if(views_->currentIndex()==2){struct Row{std::string key;MonitorPoint point;};std::vector<Row> rows;for(const auto& key:keysFor(group_->currentIndex())){const auto& points=h->series.at(key);int count=0;for(auto i=points.rbegin();i!=points.rend()&&count<300;++i)if(session<0||i->session==session){rows.push_back({key,*i});++count;}}std::stable_sort(rows.begin(),rows.end(),[](const Row& a,const Row& b){return a.point.session!=b.point.session?a.point.session>b.point.session:a.point.x>b.point.x;});if(rows.size()>300)rows.resize(300);table_->setRowCount(static_cast<int>(rows.size()));for(int row=0;row<static_cast<int>(rows.size());++row){const auto& entry=rows[static_cast<std::size_t>(row)];const QStringList values{QString::number(entry.point.session),QString::number(entry.point.x,'g',12),seriesName(entry.key),QString::number(entry.point.y,'g',12),rawUnit(entry.key)};for(int column=0;column<5;++column)table_->setItem(row,column,new QTableWidgetItem(values[column]));}}
    QStringList latestValues;
    for(const auto& key:keysFor(group_->currentIndex())){const auto& points=h->series.at(key);for(auto i=points.rbegin();i!=points.rend();++i)if(session<0||i->session==session){latestValues<<seriesName(key)+": "+QString::number(i->y*scale(key),'g',6)+(scale(key)==100?" %":rawUnit(key)=="1"?QString{}:" "+rawUnit(key));break;}if(latestValues.size()>=6)break;}
    latest_->setText(latestValues.join("   ·   "));
    summary_->setText((running_?monitorText("Live · "):loading_?monitorText("Loading recorded history · "):monitorText("Recorded history · "))+monitorText("%1 observations · %2 series").arg(h->observations).arg(h->series.size())+(frozen_?monitorText(" · view frozen; recording continues"):QString{})+(h->reduced?monitorText(" · display history reduced; complete log retained"):QString{}));
}
void RunMonitor::setCase(const QString& directory,bool force){
    caseDirectory_=directory;axisLabel_=QFileInfo::exists(QDir(directory).filePath(".vortia-flow-case"))||QFileInfo::exists(QDir(directory).filePath(".vortia-nozzle-case"))?monitorText("Iteration (pseudo-time; not physical seconds)"):monitorText("Solver coordinate (Time)");
    loadLog(directory.isEmpty()?QString{}:QDir(directory).filePath("log.vortia"),force);
}
void RunMonitor::loadLog(const QString& file,bool force){
    if(!force&&file==logPath_)return;readerStop_.store(true);if(reader_.joinable())reader_.join();const auto generation=++generation_;logPath_=file;loading_=false;running_=false;paused_->setChecked(false);frozen_.reset();history_=std::make_shared<MonitorHistory>();hidden_.clear();plot_->fit();for(auto* p:overview_)p->fit();
    source_->setText(file.isEmpty()?monitorText("No calculation case selected."):monitorText("Source: %1").arg(QDir::toNativeSeparators(file)));source_->setToolTip(monitorText("Histories belong to this calculation folder. Changing a sketch does not recalculate an existing history."));
    const QFileInfo info(file);if(file.isEmpty()||!info.isFile()){dirty_=true;refresh();return;}
    const auto cached=cache_.find(file);if(!force&&cached!=cache_.end()&&cached->second.size==info.size()&&cached->second.modified==info.lastModified().toMSecsSinceEpoch()){history_=cached->second.history;dirty_=true;refresh();return;}
    loading_=true;dirty_=true;refresh();
    readerStop_.store(false);
    reader_=std::thread([this,file,generation](){
        auto loaded=std::make_shared<MonitorHistory>();std::ifstream input(file.toStdString(),std::ios::binary);openfoam::LogParser parser(true);std::array<char,16384> buffer{};
        while(input&&!readerStop_.load()){input.read(buffer.data(),buffer.size());const auto size=input.gcount();if(size<=0)break;parser.feed(std::string_view(buffer.data(),static_cast<std::size_t>(size)));loaded->append(parser.takeMonitorSamples());}
        if(readerStop_.load())return;parser.flush();loaded->append(parser.takeMonitorSamples());
        QMetaObject::invokeMethod(this,[this,generation,loaded,file](){if(generation!=generation_||running_)return;history_=loaded;loading_=false;const QFileInfo current(file);if(cache_.size()>=4)cache_.erase(cache_.begin());cache_[file]={current.size(),current.lastModified().toMSecsSinceEpoch(),loaded};dirty_=true;refresh();},Qt::QueuedConnection);
    });
}
void RunMonitor::beginRun(const QString& directory){
    if(QDir(directory).filePath("log.vortia")!=logPath_)setCase(directory);
    readerStop_.store(true);if(reader_.joinable())reader_.join();++generation_;loading_=false;running_=true;sessionOffset_=history_->latestSession()+1;paused_->setChecked(false);dirty_=true;refresh();
}
void RunMonitor::finishRun(){if(!caseDirectory_.isEmpty())setCase(caseDirectory_,true);else{running_=false;dirty_=true;}}
void RunMonitor::appendBatch(const QString& json){
    if(!running_||json.size()>4*1024*1024)return;const auto document=QJsonDocument::fromJson(json.toUtf8());if(!document.isArray())return;std::vector<openfoam::MonitorSample> samples;samples.reserve(static_cast<std::size_t>(document.array().size()));
    for(const auto& value:document.array()){const auto row=value.toArray();if(row.size()!=4||!row[0].isDouble()||!row[1].isDouble()||!row[2].isString()||!row[3].isDouble())continue;samples.push_back({row[0].toDouble(),row[1].toDouble(),row[2].toString().toStdString(),row[3].toInt()});}
    history_->append(samples,sessionOffset_);dirty_=true;
}
}
