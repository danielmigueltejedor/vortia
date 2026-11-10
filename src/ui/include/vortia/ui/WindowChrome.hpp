#pragma once

#include <QList>
#include <QObject>
#include <QString>

class QBoxLayout;
class QMainWindow;
class QToolButton;
class QWidget;

namespace vortia::lienzo {

enum class WindowButton { Minimize, Maximize, Close };

/// Which window buttons go on which side of the top bar, in order.
struct WindowButtonLayout {
    QList<WindowButton> left;
    QList<WindowButton> right;
    bool operator==(const WindowButtonLayout&) const = default;
};

/// Minimize, maximize, close on the right: used when the desktop says nothing.
[[nodiscard]] WindowButtonLayout defaultButtonLayout();

/// GNOME-style specification, as stored in
/// `org.gnome.desktop.wm.preferences button-layout`:
/// "appmenu:minimize,maximize,close", "close,minimize,maximize:", "'appmenu:close'".
/// Unknown names (appmenu, icon, spacer) are skipped. A specification without
/// any known button yields the default layout: a window must stay closable.
[[nodiscard]] WindowButtonLayout parseGnomeButtonLayout(const QString& specification);

/// KWin specification (`ButtonsOnLeft` / `ButtonsOnRight` in kwinrc), one
/// letter per button: I minimize, A maximize, X close; others are skipped.
[[nodiscard]] WindowButtonLayout parseKdeButtonLayout(const QString& left, const QString& right);

/// Layout of the running desktop: `VORTIA_WINDOW_BUTTONS` (GNOME syntax) when
/// set, else GNOME-family settings or KWin's configuration, else the default.
[[nodiscard]] WindowButtonLayout systemButtonLayout();

/// True when Vortia should draw its own window controls: Linux desktops,
/// unless `VORTIA_NATIVE_TITLEBAR=1` asks for the system title bar.
[[nodiscard]] bool usesClientSideDecorations();

/// Turns `header` into the title bar of a frameless `window`: window buttons
/// on the side(s) given by `layout`, dragging and double-click-to-maximize on
/// the header's empty area, and resize handles on the window edges.
class WindowChrome final : public QObject {
    Q_OBJECT
public:
    /// Must be called before the window is first shown. `headerRow` is the
    /// header's horizontal layout; buttons are inserted at both ends.
    WindowChrome(QMainWindow* window, QWidget* header, QBoxLayout* headerRow,
                 const WindowButtonLayout& layout);

    /// Additional widgets whose empty area drags the window (e.g. a tab bar).
    void addDragArea(QWidget* widget);

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;

private:
    void placeResizeHandles();
    void syncMaximizeButtons();

    QMainWindow* window_;
    QWidget* header_;
    QList<QWidget*> dragAreas_;
    QList<QWidget*> resizeHandles_;
    QList<QToolButton*> maximizeButtons_;
};

} // namespace vortia::lienzo
