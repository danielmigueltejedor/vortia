#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <QWidget>
#include <QJsonArray>
#include <QJsonObject>
#include <memory>
class QTableWidget;class QComboBox;class QLabel;class QProcess;class QTemporaryDir;class QToolButton;
namespace vortia {
class MeshPlanView;
class MeshRegionsWidget final:public QWidget {
    Q_OBJECT
public:
    MeshRegionsWidget(const Sketch2D&,const QJsonObject& mesh,QWidget* parent=nullptr);
    ~MeshRegionsWidget()override;
    QJsonObject meshSettings()const;
    void setGlobalSettings(const QJsonObject&);
    void setBoundaries(const QJsonArray&);
    void requestPreview();
    bool busy()const;
signals:
    void settingsChanged();
    void previewFinished(bool success);
private:
    void changed();void refreshCuts();void refreshRegions();void archiveRegions();void refreshArchive();void editCut(int index,QPointF from={},QPointF to={});
    const Sketch2D& shape_;
    QJsonObject settings_;
    QJsonArray cuts_,regions_,boundaries_;
    MeshPlanView* view_;
    QTableWidget* table_;
    QComboBox* cutList_;
    QLabel* status_;
    QToolButton *generate_,*cancel_,*draw_,*restore_;
    QProcess* process_;
    std::unique_ptr<QTemporaryDir> stage_;
    bool updating_=false,cancelled_=false,stale_=true;
    quint64 revision_=0,requestedRevision_=0;
};
}
