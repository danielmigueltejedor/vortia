#pragma once
#include "vortia/solver/openfoam/LogParser.hpp"
#include <QWidget>
#include <QString>
#include <QSet>
#include <map>
#include <memory>
#include <thread>
#include <vector>
class QLabel;class QComboBox;class QCheckBox;class QDoubleSpinBox;class QTabWidget;class QTableWidget;class QTimer;
namespace vortia {
struct MonitorPoint {double x=0,y=0;int session=0;};
struct MonitorHistory {
    std::map<std::string,std::vector<MonitorPoint>> series;
    std::uint64_t observations=0;
    bool reduced=false;
    void append(const std::vector<openfoam::MonitorSample>& samples,int sessionOffset=0);
    bool writeCsv(const QString& path) const;
    int latestSession() const;
};
class RunPlot;
class RunMonitor final:public QWidget {
    Q_OBJECT
public:
    explicit RunMonitor(QWidget* parent=nullptr);
    ~RunMonitor() override;
    void setCase(const QString& directory,bool force=false);
    void beginRun(const QString& directory);
    void finishRun();
    void appendBatch(const QString& json);
    const MonitorHistory& history()const{return *history_;}
    bool loading()const{return loading_;}
    bool exportCsv(const QString& path)const{return history_->writeCsv(path);}
private:
    void loadLog(const QString& file,bool force);
    void refresh();void refreshSessions();void setPaused(bool);
    std::vector<std::string> keysFor(int group)const;
    std::shared_ptr<MonitorHistory> history_,frozen_;
    std::jthread reader_;
    std::uint64_t generation_=0;
    QString logPath_,caseDirectory_,axisLabel_;
    struct Cached {qint64 size=0,modified=0;std::shared_ptr<MonitorHistory> history;};
    std::map<QString,Cached> cache_;
    bool running_=false,loading_=false,dirty_=true;
    int sessionOffset_=0;
    QSet<QString> hidden_;
    QLabel *source_,*summary_,*note_,*latest_;
    QComboBox *group_,*sessions_;
    QCheckBox *logarithmic_,*paused_;
    QDoubleSpinBox* window_;
    QTabWidget* views_;
    QTableWidget* table_;
    RunPlot* plot_;
    std::vector<RunPlot*> overview_;
    QTimer* redraw_;
};
}
