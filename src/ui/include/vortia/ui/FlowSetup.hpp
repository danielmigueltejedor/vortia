#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include "vortia/ui/FlowCase.hpp"
#include <QJsonObject>
#include <QString>
#include <string>
#include <vector>
#include <cstdint>
#include <algorithm>
#include <QWidget>
#include <functional>
class QStackedWidget;
class QTabBar;
class QWidget;
namespace vortia {
class SettingsNavigation;

// One set of controls supports both embedded workspaces and the compatibility
// dialog. Each panel owns its geometry snapshot and keeps callbacks alive.
class FlowSetupPanel final : public QWidget {
    Q_OBJECT
public:
    enum class View { All, Physics, Calculation };
    FlowSetupPanel(const Sketch2D&, const QJsonObject&, QWidget* parent=nullptr,
                   const QJsonObject& defaults={}, View view=View::All);
    QJsonObject configuration() const;
    void selectPage(int page);
signals:
    void configurationEdited(const QJsonObject&);
private:
    Sketch2D shape_;
    View view_;
    QStackedWidget* pages_{};
    QTabBar* sections_{};
    SettingsNavigation* navigation_{};
    std::function<void()> normalize_;
    std::function<QJsonObject()> snapshot_;
};
QJsonObject defaultFlowConfiguration(const Sketch2D&, const QJsonObject& previous={},
                                     const QJsonObject& defaults={});
// Keep small meshes out of MPI launch/communication overhead. Explicit user
// process counts always bypass this conservative desktop recommendation.
inline unsigned recommendedFlowProcesses(std::uint64_t cells,unsigned hardwareThreads) {
    const unsigned budget=std::clamp(hardwareThreads,1u,8u);
    const auto useful=std::clamp<std::uint64_t>(cells/3000,1,budget);
    if(useful==budget)return budget;
    unsigned chosen=1;while(chosen*2<=useful)chosen*=2;return chosen;
}
QJsonObject editFlowSetup(QWidget*, const Sketch2D&, const QJsonObject&, int page = 0, const QJsonObject& defaults = {});
QString generateFlowCase(QWidget*, const QString& sketchPath, const QJsonObject&,
                         const QString& projectRoot,
                         const std::vector<std::pair<std::string,std::string>>& environment, const QString& caseStorage = {});
}
