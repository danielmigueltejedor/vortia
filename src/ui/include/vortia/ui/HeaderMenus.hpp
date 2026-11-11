#pragma once

#include <QString>
#include <QWidget>

class QMenuBar;
class QToolButton;

namespace vortia::ndl {

/// The menus in a client-side-decorated header. A shell with a global menu
/// (macOS, Nodalix, KDE's app-menu applet) shows them in its top bar, so the
/// window keeps one button for them; without one they sit in the header.
class HeaderMenus : public QWidget {
    Q_OBJECT
public:
    /// The D-Bus service a Linux shell registers when it shows global menus.
    static QString registrarService();

    /// `source` is the window's own menu bar; its actions are shared, not copied.
    explicit HeaderMenus(QMenuBar* source, QWidget* parent = nullptr,
                         const QString& service = registrarService());

    /// Whether the shell shows menus in its own top bar right now.
    static bool shellHasGlobalMenu(const QString& service = registrarService());

    void setShellHasGlobalMenu(bool global);
    QToolButton* menuButton() const { return button_; }
    QMenuBar* inlineMenus() const { return inline_; }

private:
    QToolButton* button_;
    QMenuBar* inline_;
};

}  // namespace vortia::ndl
