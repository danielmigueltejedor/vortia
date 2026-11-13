#pragma once

#include <QIcon>
#include <QStringList>
#include <QWidget>
#include <functional>

class QLabel;
class QVBoxLayout;
class QPushButton;

namespace vortia {

/// What Vortia shows with no project open, as Vortia Native does: the app, a line on what it is
/// for, New and Open, and the projects opened last. It lies over the window below its header, so
/// the window controls and menus stay where they are.
class WelcomeScreen : public QWidget {
    Q_OBJECT

public:
    /// `top` answers where the welcome starts, in window coordinates: the bottom of the header.
    WelcomeScreen(QWidget* window, std::function<int()> top);

    void setActionIcons(const QIcon& newProject, const QIcon& openProject, const QIcon& folder);
    /// Folders, most recent first. Missing ones are left out; with none, the column goes.
    void setRecentProjects(const QStringList& folders);
    /// Follows the window's size and the header's height.
    void place();

signals:
    void newProjectRequested();
    void openProjectRequested();
    void recentProjectRequested(const QString& folder);

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;
    void changeEvent(QEvent* event) override;

private:
    void retranslate();

    std::function<int()> top_;
    QPushButton* newButton_{};
    QPushButton* openButton_{};
    QLabel* title_{};
    QLabel* tagline_{};
    QLabel* body_{};
    QLabel* recentTitle_{};
    QWidget* recentColumn_{};
    QVBoxLayout* recentRows_{};
    QIcon folderIcon_;
    QStringList recent_;
};

/// The recent projects list kept in settings: most recent first, at most `limit`, no duplicates.
[[nodiscard]] QStringList rememberRecentProject(QStringList recent, const QString& folder, int limit = 8);

} // namespace vortia
