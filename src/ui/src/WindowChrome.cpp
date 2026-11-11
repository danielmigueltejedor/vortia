#include "vortia/ui/WindowChrome.hpp"

#include "vortia/ui/ThemeManager.hpp"
#include "vortia/ui/UiIcon.hpp"

#include <cmath>
#include <utility>

#include <QAbstractButton>
#include <QApplication>
#include <QBoxLayout>
#include <QDialog>
#include <QDir>
#include <QDockWidget>
#include <QFileDialog>
#include <QFrame>
#include <QLabel>
#include <QLayout>
#include <QMenu>
#include <QMessageBox>
#include <QPushButton>
#include <QEvent>
#include <QJsonObject>
#include <QMainWindow>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QProcess>
#include <QSettings>
#include <QStyle>
#include <QTabBar>
#include <QToolButton>
#include <QWindow>

namespace vortia::ndl {

namespace {

constexpr int kResizeMargin = 6;

/// Round window button whose glyph is drawn in the current text colour, so
/// it follows the theme without separate icon files.
/// The glyph colour under the pointer: cyan for minimise, amber for maximise, from the tokens of
/// the mode ThemeManager applied (the style sheet tints the button to match).
QColor tokenColor(const QString& name) {
    static const QJsonObject tokens = bundledTokens();
    const QString scheme = qApp->property("ndl-scheme").toString();
    return QColor(tokenValue(tokens, scheme.isEmpty() ? QStringLiteral("light") : scheme, name));
}

QColor hoverInk(WindowButton kind) {
    return tokenColor(kind == WindowButton::Maximize ? QStringLiteral("color.windowMaximize")
                                                     : QStringLiteral("color.windowMinimize"));
}

/// A bold NDL icon from the bundle, inked in `ink`, at `size` logical pixels.
QPixmap inkedIcon(const QString& name, const QColor& ink, int size, qreal ratio) {
    QPixmap image = QIcon(QStringLiteral(":/vortia/icons/%1.svg").arg(name))
                        .pixmap(QSize(size, size) * ratio);
    if (image.isNull())
        return image;
    QPainter painter(&image);
    painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
    painter.fillRect(image.rect(), ink);
    painter.end();
    image.setDevicePixelRatio(ratio);
    return image;
}

class GlyphButton final : public QToolButton {
public:
    GlyphButton(WindowButton kind, QWidget* parent) : QToolButton(parent), kind_(kind) {
        setObjectName(QStringLiteral("ndl-window-button"));
        setFocusPolicy(Qt::NoFocus);
        setCursor(Qt::ArrowCursor);
        setProperty("role", kind == WindowButton::Close ? QStringLiteral("close")
                          : kind == WindowButton::Maximize ? QStringLiteral("maximize")
                                                           : QStringLiteral("minimize"));
    }
    void setRestore(bool restore) {
        if (restore_ != restore) {
            restore_ = restore;
            update();
        }
    }

protected:
    void paintEvent(QPaintEvent* event) override {
        QToolButton::paintEvent(event);
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        // NDL's bold window glyphs: 2 px strokes centred on whole pixels, so they stay crisp.
        // Close turns coral under the pointer (the style sheet), and its X turns white with it.
        const bool hovered = underMouse();
        const QColor ink = !hovered                     ? palette().color(QPalette::ButtonText)
                           : kind_ == WindowButton::Close ? QColor(Qt::white)
                                                          : hoverInk(kind_);
        QPen pen(ink, 2.0);
        pen.setCapStyle(Qt::RoundCap);
        pen.setJoinStyle(Qt::RoundJoin);
        painter.setPen(pen);
        const QPointF c(std::floor(width() / 2.0), std::floor(height() / 2.0));
        const qreal h = 4.0;
        switch (kind_) {
        case WindowButton::Minimize:
            painter.drawLine(QPointF(c.x() - h, c.y()), QPointF(c.x() + h, c.y()));
            break;
        case WindowButton::Maximize: {
            // NDL's two corners on a diagonal: open outwards to maximise, turned to the centre
            // to restore (window-maximize / window-restore in @nodalia/icons).
            const auto corner = [&](QPointF a, QPointF b, QPointF d) {
                painter.drawPolyline(QPolygonF{c + a, c + b, c + d});
            };
            if (restore_) {
                corner({2, -5}, {2, -2}, {5, -2});
                corner({-2, 5}, {-2, 2}, {-5, 2});
            } else {
                corner({0, -5}, {5, -5}, {5, 0});
                corner({0, 5}, {-5, 5}, {-5, 0});
            }
            break;
        }
        case WindowButton::Close:
            painter.drawLine(QPointF(c.x() - h, c.y() - h), QPointF(c.x() + h, c.y() + h));
            painter.drawLine(QPointF(c.x() + h, c.y() - h), QPointF(c.x() - h, c.y() + h));
            break;
        }
    }

    void enterEvent(QEnterEvent* event) override {
        QToolButton::enterEvent(event);
        update();
    }

    void leaveEvent(QEvent* event) override {
        QToolButton::leaveEvent(event);
        update();
    }

private:
    WindowButton kind_;
    bool restore_{false};
};

bool hasAnyButton(const WindowButtonLayout& layout) {
    return !layout.left.isEmpty() || !layout.right.isEmpty();
}

QList<WindowButton> gnomeSide(const QString& side) {
    QList<WindowButton> buttons;
    for (const QString& raw : side.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        const QString name = raw.trimmed().toLower();
        WindowButton button;
        if (name == QStringLiteral("minimize"))
            button = WindowButton::Minimize;
        else if (name == QStringLiteral("maximize"))
            button = WindowButton::Maximize;
        else if (name == QStringLiteral("close"))
            button = WindowButton::Close;
        else
            continue;
        if (!buttons.contains(button))
            buttons.append(button);
    }
    return buttons;
}

QList<WindowButton> kdeSide(const QString& letters) {
    QList<WindowButton> buttons;
    for (const QChar letter : letters) {
        WindowButton button;
        if (letter == QLatin1Char('I'))
            button = WindowButton::Minimize;
        else if (letter == QLatin1Char('A'))
            button = WindowButton::Maximize;
        else if (letter == QLatin1Char('X'))
            button = WindowButton::Close;
        else
            continue;
        if (!buttons.contains(button))
            buttons.append(button);
    }
    return buttons;
}

} // namespace

WindowButtonLayout defaultButtonLayout() {
    return {{}, {WindowButton::Minimize, WindowButton::Maximize, WindowButton::Close}};
}

WindowButtonLayout parseGnomeButtonLayout(const QString& specification) {
    QString spec = specification.trimmed();
    // gsettings prints GVariant text: 'appmenu:close'
    while (spec.size() >= 2 && (spec.front() == QLatin1Char('\'') || spec.front() == QLatin1Char('"'))
           && spec.back() == spec.front())
        spec = spec.mid(1, spec.size() - 2).trimmed();
    const qsizetype colon = spec.indexOf(QLatin1Char(':'));
    WindowButtonLayout layout;
    if (colon < 0) {
        layout.left = gnomeSide(spec);
    } else {
        layout.left = gnomeSide(spec.left(colon));
        layout.right = gnomeSide(spec.mid(colon + 1));
    }
    for (const WindowButton button : layout.left)
        layout.right.removeAll(button);
    return hasAnyButton(layout) ? layout : defaultButtonLayout();
}

WindowButtonLayout parseKdeButtonLayout(const QString& left, const QString& right) {
    WindowButtonLayout layout{kdeSide(left), kdeSide(right)};
    for (const WindowButton button : layout.left)
        layout.right.removeAll(button);
    return hasAnyButton(layout) ? layout : defaultButtonLayout();
}

WindowButtonLayout systemButtonLayout() {
    if (qEnvironmentVariableIsSet("VORTIA_WINDOW_BUTTONS"))
        return parseGnomeButtonLayout(qEnvironmentVariable("VORTIA_WINDOW_BUTTONS"));
    const QString desktop = qEnvironmentVariable("XDG_CURRENT_DESKTOP").toLower();
    if (desktop.contains(QStringLiteral("kde"))) {
        const QSettings kwin(QDir::homePath() + QStringLiteral("/.config/kwinrc"), QSettings::IniFormat);
        return parseKdeButtonLayout(
            kwin.value(QStringLiteral("org.kde.kdecoration2/ButtonsOnLeft"), QStringLiteral("MS")).toString(),
            kwin.value(QStringLiteral("org.kde.kdecoration2/ButtonsOnRight"), QStringLiteral("HIAX")).toString());
    }
    for (const char* family : {"gnome", "unity", "cinnamon", "budgie", "pantheon", "mate", "xfce"}) {
        if (!desktop.contains(QLatin1String(family)))
            continue;
        QProcess gsettings;
        gsettings.start(QStringLiteral("gsettings"),
                        {QStringLiteral("get"), QStringLiteral("org.gnome.desktop.wm.preferences"),
                         QStringLiteral("button-layout")});
        if (gsettings.waitForStarted(500) && gsettings.waitForFinished(1500)
            && gsettings.exitStatus() == QProcess::NormalExit && gsettings.exitCode() == 0)
            return parseGnomeButtonLayout(QString::fromUtf8(gsettings.readAllStandardOutput()));
        gsettings.kill();
        gsettings.waitForFinished(200);
        break;
    }
    return defaultButtonLayout();
}

bool usesClientSideDecorations() {
    // VORTIA_FORCE_CSD=1 shows the integrated bar on any system (design review).
    if (qEnvironmentVariableIntValue("VORTIA_FORCE_CSD") != 0)
        return true;
#ifdef Q_OS_LINUX
    if (qEnvironmentVariableIntValue("VORTIA_NATIVE_TITLEBAR") != 0)
        return false;
    // Preferences › Window controls: "system" hands the frame back to the desktop.
    const QSettings settings(QStringLiteral("Vortia"), QStringLiteral("Vortia"));
    return settings.value(QStringLiteral("ui/window-controls")).toString() != QStringLiteral("system");
#else
    return false;
#endif
}

WindowChrome::WindowChrome(QWidget* window, QWidget* header, QBoxLayout* headerRow,
                           const WindowButtonLayout& layout)
    : QObject(window), window_(window), header_(header) {
    window_->setWindowFlag(Qt::FramelessWindowHint, true);

    const auto makeButton = [this](WindowButton kind) {
        auto* button = new GlyphButton(kind, header_);
        switch (kind) {
        case WindowButton::Minimize:
            button->setToolTip(tr("Minimize"));
            connect(button, &QToolButton::clicked, window_, &QWidget::showMinimized);
            break;
        case WindowButton::Maximize:
            button->setToolTip(tr("Maximize"));
            connect(button, &QToolButton::clicked, window_, [this] {
                window_->isMaximized() ? window_->showNormal() : window_->showMaximized();
            });
            maximizeButtons_.append(button);
            break;
        case WindowButton::Close:
            button->setToolTip(tr("Close"));
            connect(button, &QToolButton::clicked, window_, &QWidget::close);
            break;
        }
        return button;
    };
    int index = 0;
    if (!layout.left.isEmpty()) {
        headerRow->insertSpacing(index++, 8);
        for (const WindowButton kind : layout.left)
            headerRow->insertWidget(index++, makeButton(kind));
    }
    for (const WindowButton kind : layout.right)
        headerRow->addWidget(makeButton(kind));
    if (!layout.right.isEmpty())
        headerRow->addSpacing(8);

    addDragArea(header_);
    window_->installEventFilter(this);

    // Eight thin handles on the window edges; the top ones sit above the header.
    const Qt::Edges edges[] = {Qt::TopEdge, Qt::BottomEdge, Qt::LeftEdge, Qt::RightEdge,
                               Qt::TopEdge | Qt::LeftEdge, Qt::TopEdge | Qt::RightEdge,
                               Qt::BottomEdge | Qt::LeftEdge, Qt::BottomEdge | Qt::RightEdge};
    for (const Qt::Edges edge : edges) {
        auto* handle = new QWidget(window_);
        handle->setObjectName(QStringLiteral("ndl-resize-handle"));
        handle->setProperty("edges", static_cast<int>(edge.toInt()));
        const bool horizontal = edge == Qt::Edges(Qt::LeftEdge) || edge == Qt::Edges(Qt::RightEdge);
        const bool vertical = edge == Qt::Edges(Qt::TopEdge) || edge == Qt::Edges(Qt::BottomEdge);
        const bool diagonalDown = edge == (Qt::TopEdge | Qt::LeftEdge) || edge == (Qt::BottomEdge | Qt::RightEdge);
        handle->setCursor(horizontal ? Qt::SizeHorCursor : vertical ? Qt::SizeVerCursor
                          : diagonalDown ? Qt::SizeFDiagCursor : Qt::SizeBDiagCursor);
        handle->installEventFilter(this);
        resizeHandles_.append(handle);
    }
    placeResizeHandles();
    syncMaximizeButtons();
}

void WindowChrome::addDragArea(QWidget* widget) {
    if (widget && !dragAreas_.contains(widget)) {
        dragAreas_.append(widget);
        widget->installEventFilter(this);
    }
}

void WindowChrome::placeResizeHandles() {
    const bool resizable = !(window_->windowState() & (Qt::WindowMaximized | Qt::WindowFullScreen));
    const int w = window_->width(), h = window_->height(), m = kResizeMargin, corner = 2 * m;
    for (QWidget* handle : resizeHandles_) {
        const Qt::Edges edge = Qt::Edges::fromInt(handle->property("edges").toInt());
        QRect area;
        if (edge == Qt::Edges(Qt::TopEdge)) area = {corner, 0, w - 2 * corner, m};
        else if (edge == Qt::Edges(Qt::BottomEdge)) area = {corner, h - m, w - 2 * corner, m};
        else if (edge == Qt::Edges(Qt::LeftEdge)) area = {0, corner, m, h - 2 * corner};
        else if (edge == Qt::Edges(Qt::RightEdge)) area = {w - m, corner, m, h - 2 * corner};
        else if (edge == (Qt::TopEdge | Qt::LeftEdge)) area = {0, 0, corner, corner};
        else if (edge == (Qt::TopEdge | Qt::RightEdge)) area = {w - corner, 0, corner, corner};
        else if (edge == (Qt::BottomEdge | Qt::LeftEdge)) area = {0, h - corner, corner, corner};
        else area = {w - corner, h - corner, corner, corner};
        handle->setGeometry(area);
        handle->setVisible(resizable);
        if (resizable)
            handle->raise();
    }
}

void WindowChrome::syncMaximizeButtons() {
    const bool maximized = window_->isMaximized();
    for (QToolButton* button : maximizeButtons_) {
        static_cast<GlyphButton*>(button)->setRestore(maximized);
        button->setToolTip(maximized ? tr("Restore") : tr("Maximize"));
    }
}

bool WindowChrome::eventFilter(QObject* watched, QEvent* event) {
    if (watched == window_) {
        if (event->type() == QEvent::Resize || event->type() == QEvent::Show)
            placeResizeHandles();
        else if (event->type() == QEvent::WindowStateChange) {
            placeResizeHandles();
            syncMaximizeButtons();
        }
        return false;
    }
    auto* widget = qobject_cast<QWidget*>(watched);
    if (!widget)
        return false;

    if (resizeHandles_.contains(widget)) {
        if (event->type() == QEvent::MouseButtonPress
            && static_cast<QMouseEvent*>(event)->button() == Qt::LeftButton) {
            if (QWindow* handle = window_->windowHandle())
                handle->startSystemResize(Qt::Edges::fromInt(widget->property("edges").toInt()));
            return true;
        }
        return false;
    }

    if (!dragAreas_.contains(widget))
        return false;
    switch (event->type()) {
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonDblClick: {
        auto* mouse = static_cast<QMouseEvent*>(event);
        if (mouse->button() != Qt::LeftButton)
            return false;
        // Only the empty part of the bar belongs to the window: a tab or a
        // button under the pointer keeps its own behaviour.
        if (auto* tabs = qobject_cast<QTabBar*>(widget); tabs && tabs->tabAt(mouse->position().toPoint()) >= 0)
            return false;
        if (QWidget* child = widget->childAt(mouse->position().toPoint());
            child && !dragAreas_.contains(child))
            return false;
        if (event->type() == QEvent::MouseButtonDblClick) {
            dragOrigin_.reset();
            window_->isMaximized() ? window_->showNormal() : window_->showMaximized();
        } else {
            // The move starts once the pointer actually travels. Starting it
            // on the press hands the pointer to the window manager and the
            // second click of a double click never arrives.
            dragOrigin_ = mouse->globalPosition().toPoint();
        }
        return true;
    }
    case QEvent::MouseMove: {
        auto* mouse = static_cast<QMouseEvent*>(event);
        if (!dragOrigin_ || !(mouse->buttons() & Qt::LeftButton))
            return false;
        if ((mouse->globalPosition().toPoint() - *dragOrigin_).manhattanLength() < QApplication::startDragDistance())
            return true;
        dragOrigin_.reset();
        if (QWindow* handle = window_->windowHandle())
            handle->startSystemMove();
        return true;
    }
    case QEvent::MouseButtonRelease:
        if (dragOrigin_) {
            dragOrigin_.reset();
            return true;
        }
        return false;
    default:
        return false;
    }
}

namespace {

/// Message boxes speak with NDL's bold icons, in the colour of what they say.
void restyleMessageBox(QMessageBox* box) {
    QString icon, ink;
    switch (box->icon()) {
    case QMessageBox::Warning: icon = QStringLiteral("warning"); ink = QStringLiteral("color.statusWarningFg"); break;
    case QMessageBox::Critical: icon = QStringLiteral("error"); ink = QStringLiteral("color.statusDangerFg"); break;
    case QMessageBox::Question: icon = QStringLiteral("help"); ink = QStringLiteral("color.accentText"); break;
    case QMessageBox::Information: icon = QStringLiteral("info"); ink = QStringLiteral("color.accentText"); break;
    default: break;
    }
    if (!icon.isEmpty())
        box->setIconPixmap(inkedIcon(icon, tokenColor(ink), 40, box->devicePixelRatioF()));
    for (QAbstractButton* button : box->buttons()) {
        // The risky action in coral; backing out stays neutral even as the default button.
        const auto role = box->buttonRole(button);
        const char* variant = role == QMessageBox::DestructiveRole ? "danger"
                              : role == QMessageBox::RejectRole   ? "neutral"
                                                                  : nullptr;
        if (variant) {
            button->setProperty("ndlVariant", QString::fromLatin1(variant));
            button->style()->unpolish(button);
            button->style()->polish(button);
        }
    }
}

class DialogChrome final : public QObject {
public:
    explicit DialogChrome(QObject* parent, bool frames) : QObject(parent), frames_(frames) {}

protected:
    bool eventFilter(QObject* watched, QEvent* event) override {
        if (event->type() == QEvent::ToolTip)
            return repeatsVisibleText(qobject_cast<QAbstractButton*>(watched));
        if (event->type() != QEvent::Polish)
            return false;
        roundPopup(qobject_cast<QWidget*>(watched));
        if (!frames_)
            return false;
        auto* dialog = qobject_cast<QDialog*>(watched);
        // File dialogs are the desktop's own; a dialog without a layout has no room for a header.
        if (!dialog || !dialog->isWindow() || qobject_cast<QFileDialog*>(dialog) || !dialog->layout()
            || dialog->property("ndl-chrome").toBool()
            || dialog->windowFlags().testFlag(Qt::FramelessWindowHint))
            return false;
        dialog->setProperty("ndl-chrome", true);
        // The style polished the dialog before this event; again, so the frame rule applies.
        dialog->style()->unpolish(dialog);
        dialog->style()->polish(dialog);
        // Before the window exists the flag is enough; once it does, the platform window takes it.
        if (!dialog->testAttribute(Qt::WA_WState_Created))
            dialog->setWindowFlag(Qt::FramelessWindowHint, true);
        else if (QWindow* window = dialog->windowHandle())
            window->setFlags(window->flags() | Qt::FramelessWindowHint);
        // Restyled first: a message box rebuilds its layout when its icon changes.
        if (auto* box = qobject_cast<QMessageBox*>(dialog))
            restyleMessageBox(box);

        auto* header = new QWidget(dialog);
        header->setObjectName(QStringLiteral("ndl-dialog-header"));
        auto* row = new QHBoxLayout(header);
        row->setContentsMargins(16, 8, 0, 0);
        row->setSpacing(8);
        auto* title = new QLabel(dialog->windowTitle(), header);
        title->setObjectName(QStringLiteral("ndl-dialog-header-title"));
        connect(dialog, &QWidget::windowTitleChanged, title, &QLabel::setText);
        // A dialog that opens with its own big title needs no second one above it.
        title->setVisible(!dialog->findChild<QLabel*>(QStringLiteral("ndl-dialog-title"))
                          && !dialog->findChild<QLabel*>(QStringLiteral("ndl-about-title")));
        row->addWidget(title);
        row->addStretch(1);
        new WindowChrome(dialog, header, row, WindowButtonLayout{{}, {WindowButton::Close}});
        dialog->layout()->setMenuBar(header);
        return false;
    }

private:
    /// A button that shows its name needs no tooltip saying it again; one that shows only an
    /// icon keeps its tooltip, which is then the only place its name appears.
    static bool repeatsVisibleText(QAbstractButton* button) {
        if (!button || button->text().isEmpty())
            return false;
        if (auto* tool = qobject_cast<QToolButton*>(button); tool && tool->toolButtonStyle() == Qt::ToolButtonIconOnly)
            return false;
        // Only a repeat is dropped: the label again, perhaps with its shortcut ("Malla · Alt+3").
        // A tooltip that explains more than the label keeps showing.
        const auto plain = [](QString text) {
            text.remove(QLatin1Char('&'));
            text = text.trimmed();
            for (const QString& ellipsis : {QStringLiteral("…"), QStringLiteral("...")}) {
                if (text.endsWith(ellipsis))
                    text.chop(ellipsis.size());
            }
            return text.trimmed();
        };
        const QString label = plain(button->text());
        const QString tip = plain(button->toolTip());
        return tip.isEmpty() || tip == label || tip.startsWith(label + QStringLiteral(" · "));
    }

    /// Menus, combo lists and tooltips are islands with NDL's radius: their window has to be
    /// see-through for the corners to show, and that is only possible before it exists.
    static void roundPopup(QWidget* widget) {
        if (!widget || !widget->isWindow() || widget->testAttribute(Qt::WA_WState_Created))
            return;
        if (qobject_cast<QMenu*>(widget) || widget->inherits("QComboBoxPrivateContainer")
            || widget->inherits("QTipLabel"))
            widget->setAttribute(Qt::WA_TranslucentBackground, true);
    }

    bool frames_;
};

/// Moves a floating dock by its title bar. Qt moves it by setting its position, which Wayland
/// does not allow; the compositor has to move it. A docked panel does not move by its title.
class DockDrag final : public QObject {
public:
    DockDrag(QDockWidget* dock, QObject* parent) : QObject(parent), dock_(dock) {}

protected:
    bool eventFilter(QObject* watched, QEvent* event) override {
        auto* bar = qobject_cast<QWidget*>(watched);
        if (!bar)
            return false;
        // Docked, the title bar does not move the panel: Qt's own drag to undock hangs under
        // Wayland. Only the window-mode button floats it; the buttons keep their clicks.
        if (!dock_->isFloating()) {
            const bool mouse = event->type() == QEvent::MouseButtonPress
                               || event->type() == QEvent::MouseButtonDblClick
                               || event->type() == QEvent::MouseMove
                               || event->type() == QEvent::MouseButtonRelease;
            return mouse && !onButton(bar, static_cast<QMouseEvent*>(event));
        }
        switch (event->type()) {
        case QEvent::MouseButtonDblClick:
            return true;
        case QEvent::MouseButtonPress: {
            auto* mouse = static_cast<QMouseEvent*>(event);
            if (mouse->button() != Qt::LeftButton || onButton(bar, mouse))
                return false;
            origin_ = mouse->globalPosition().toPoint();
            return true;
        }
        case QEvent::MouseMove: {
            auto* mouse = static_cast<QMouseEvent*>(event);
            if (!origin_ || !(mouse->buttons() & Qt::LeftButton))
                return false;
            if ((mouse->globalPosition().toPoint() - *origin_).manhattanLength() < QApplication::startDragDistance())
                return true;
            origin_.reset();
            if (QWindow* window = dock_->windowHandle())
                window->startSystemMove();
            return true;
        }
        case QEvent::MouseButtonRelease:
            if (!origin_)
                return false;
            origin_.reset();
            return true;
        default:
            return false;
        }
    }

private:
    /// The title text is part of the bar; only its buttons take their own clicks.
    static bool onButton(QWidget* bar, QMouseEvent* mouse) {
        return qobject_cast<QAbstractButton*>(bar->childAt(mouse->position().toPoint())) != nullptr;
    }

    QDockWidget* dock_;
    std::optional<QPoint> origin_;
};

class NoticeFollower final : public QObject {
public:
    NoticeFollower(QWidget* notice, QLabel* icon, QString kind)
        : QObject(notice), notice_(notice), icon_(icon), kind_(std::move(kind)) {}
    void paintIcon() {
        const QString ink = kind_ == QStringLiteral("warning") ? QStringLiteral("color.statusWarningFg")
                                                              : QStringLiteral("color.accentText");
        icon_->setPixmap(inkedIcon(kind_ == QStringLiteral("warning") ? kind_ : QStringLiteral("info"),
                                   tokenColor(ink), 20, notice_->devicePixelRatioF()));
    }

protected:
    bool eventFilter(QObject* watched, QEvent* event) override {
        if (watched == notice_) {
            // A theme change restyles the window: re-ink the icon for the new mode.
            if (event->type() == QEvent::StyleChange || event->type() == QEvent::PaletteChange)
                paintIcon();
        } else if (event->type() == QEvent::ShowToParent) {
            notice_->show();
        } else if (event->type() == QEvent::HideToParent) {
            notice_->hide();
        }
        return false;
    }

private:
    QWidget* notice_;
    QLabel* icon_;
    QString kind_;
};

class CanvasCorners final : public QWidget {
public:
    CanvasCorners(QWidget* canvas, int radius) : QWidget(canvas), radius_(radius) {
        setAttribute(Qt::WA_TransparentForMouseEvents);
        setAttribute(Qt::WA_NoSystemBackground);
        canvas->installEventFilter(this);
        setGeometry(canvas->rect());
        raise();
    }

protected:
    bool eventFilter(QObject*, QEvent* event) override {
        if (event->type() == QEvent::Resize) {
            setGeometry(parentWidget()->rect());
            raise();
        }
        return false;
    }

    void paintEvent(QPaintEvent*) override {
        // The window's frame shows outside an island's corners; a theme change repaints it.
        QPainterPath outside;
        outside.addRect(QRectF(rect()));
        QPainterPath island;
        island.addRoundedRect(QRectF(rect()), radius_, radius_);
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.fillPath(outside.subtracted(island), tokenColor(QStringLiteral("color.surfaceWindow")));
    }

private:
    int radius_;
};

} // namespace

void roundCanvasCorners(QWidget* canvas, int radius) {
    if (canvas)
        new CanvasCorners(canvas, radius);
}

void installDialogChrome(QApplication* app) {
    app->installEventFilter(new DialogChrome(app, usesClientSideDecorations()));
}

void installDockTitleBar(QDockWidget* dock) {
    auto* bar = new QWidget(dock);
    bar->setObjectName(QStringLiteral("ndl-dock-title"));
    // Title and body are one island: the bar paints the island's top, the body its bottom.
    bar->setAttribute(Qt::WA_StyledBackground, true);
    if (QWidget* body = dock->widget())
        body->setProperty("ndl-dock-body", true);
    bar->installEventFilter(new DockDrag(dock, bar));
    auto* row = new QHBoxLayout(bar);
    row->setContentsMargins(16, 10, 10, 2);
    row->setSpacing(4);
    auto* title = new QLabel(dock->windowTitle(), bar);
    title->setObjectName(QStringLiteral("ndl-dock-title-label"));
    QObject::connect(dock, &QWidget::windowTitleChanged, title, &QLabel::setText);
    row->addWidget(title, 1);
    const auto button = [&](const char* icon, const QString& tip) {
        auto* b = new QToolButton(bar);
        b->setObjectName(QStringLiteral("ndl-dock-button"));
        b->setIcon(uiIcon(QStringLiteral(":/vortia/icons/%1.svg").arg(QLatin1String(icon))));
        b->setIconSize(QSize(16, 16));
        b->setToolTip(tip);
        b->setFocusPolicy(Qt::NoFocus);
        row->addWidget(b);
        return b;
    };
    auto* floatButton = button("undock", QObject::tr("Float"));
    auto* closeButton = button("close", QObject::tr("Close"));
    QObject::connect(floatButton, &QToolButton::clicked, dock, [dock] { dock->setFloating(!dock->isFloating()); });
    // The same button docks a floating panel again: its name says which it does now.
    QObject::connect(dock, &QDockWidget::topLevelChanged, floatButton, [floatButton](bool floating) {
        floatButton->setToolTip(floating ? QObject::tr("Dock") : QObject::tr("Float"));
        floatButton->setAccessibleName(floatButton->toolTip());
    });
    floatButton->setAccessibleName(floatButton->toolTip());
    QObject::connect(closeButton, &QToolButton::clicked, dock, &QWidget::close);
    const auto sync = [dock, floatButton, closeButton] {
        floatButton->setVisible(dock->features().testFlag(QDockWidget::DockWidgetFloatable));
        closeButton->setVisible(dock->features().testFlag(QDockWidget::DockWidgetClosable));
    };
    sync();
    QObject::connect(dock, &QDockWidget::featuresChanged, bar, sync);
    dock->setTitleBarWidget(bar);
}

QWidget* wrapNotice(QLabel* label, const QString& kind) {
    auto* notice = new QFrame(label->parentWidget());
    notice->setObjectName(QStringLiteral("ndl-notice"));
    notice->setProperty("kind", kind);
    auto* row = new QHBoxLayout(notice);
    row->setContentsMargins(14, 10, 16, 10);
    row->setSpacing(10);
    auto* icon = new QLabel(notice);
    icon->setObjectName(QStringLiteral("ndl-notice-icon"));
    icon->setFixedSize(20, 20);
    row->addWidget(icon, 0, Qt::AlignTop);
    const bool shown = !label->isHidden();
    label->setParent(notice);
    row->addWidget(label, 1);
    label->setVisible(shown);
    auto* follower = new NoticeFollower(notice, icon, kind);
    follower->paintIcon();
    notice->installEventFilter(follower);
    label->installEventFilter(follower);
    notice->setVisible(shown);
    return notice;
}

} // namespace vortia::ndl
