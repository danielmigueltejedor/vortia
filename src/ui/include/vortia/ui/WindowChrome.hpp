#pragma once

#include <QList>
#include <QObject>
#include <QPoint>
#include <QString>

#include <optional>

class QApplication;
class QBoxLayout;
class QDockWidget;
class QLabel;
class QToolButton;
class QWidget;

namespace vortia::ndl {

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
    WindowChrome(QWidget* window, QWidget* header, QBoxLayout* headerRow,
                 const WindowButtonLayout& layout);

    /// Additional widgets whose empty area drags the window (e.g. a tab bar).
    void addDragArea(QWidget* widget);

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;

private:
    void placeResizeHandles();
    void syncMaximizeButtons();

    QWidget* window_;
    QWidget* header_;
    QList<QWidget*> dragAreas_;
    QList<QWidget*> resizeHandles_;
    QList<QToolButton*> maximizeButtons_;
    std::optional<QPoint> dragOrigin_; ///< Press position of a possible window drag.
};

/// Gives every dialog the NDL frame from then on: no system title bar, a header with the dialog's
/// title in Sora (left out when the dialog shows its own big title) and only the coral close
/// button, dragged like the main window's. Message boxes also get NDL's bold notice icons.
/// File dialogs stay the desktop's own. Does nothing without client-side decorations.
void installDialogChrome(QApplication* app);

/// Replaces a dock's title bar with an NDL one: its title, then bold float and close buttons with
/// the same shape and hover as the rest of the window.
void installDockTitleBar(QDockWidget* dock);

/// Puts `label` in an NDL notice: a rounded island in the soft colour of `kind` ("info",
/// "warning") with its bold icon. Returns the notice to place in a layout; it shows and hides
/// with the label, so code that toggles the label keeps working.
[[nodiscard]] QWidget* wrapNotice(QLabel* label, const QString& kind);

} // namespace vortia::ndl
