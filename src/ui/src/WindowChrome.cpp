#include "vortia/ui/WindowChrome.hpp"

#include <QApplication>
#include <QBoxLayout>
#include <QDir>
#include <QEvent>
#include <QMainWindow>
#include <QMouseEvent>
#include <QPainter>
#include <QProcess>
#include <QSettings>
#include <QTabBar>
#include <QToolButton>
#include <QWindow>

namespace vortia::ndl {

namespace {

constexpr int kResizeMargin = 6;

/// Round window button whose glyph is drawn in the current text colour, so
/// it follows the theme without separate icon files.
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
        QPen pen(palette().color(QPalette::ButtonText), 1.4);
        pen.setCapStyle(Qt::RoundCap);
        painter.setPen(pen);
        const QPointF c = QRectF(rect()).center();
        const qreal h = 4.0;
        switch (kind_) {
        case WindowButton::Minimize:
            painter.drawLine(QPointF(c.x() - h, c.y() + 3.0), QPointF(c.x() + h, c.y() + 3.0));
            break;
        case WindowButton::Maximize:
            if (restore_) {
                painter.drawRect(QRectF(c.x() - h, c.y() - h + 2.0, 2 * h - 2.0, 2 * h - 2.0));
                painter.drawLine(QPointF(c.x() - h + 2.0, c.y() - h), QPointF(c.x() + h, c.y() - h));
                painter.drawLine(QPointF(c.x() + h, c.y() - h), QPointF(c.x() + h, c.y() + h - 2.0));
            } else {
                painter.drawRect(QRectF(c.x() - h, c.y() - h, 2 * h, 2 * h));
            }
            break;
        case WindowButton::Close:
            painter.drawLine(QPointF(c.x() - h, c.y() - h), QPointF(c.x() + h, c.y() + h));
            painter.drawLine(QPointF(c.x() + h, c.y() - h), QPointF(c.x() - h, c.y() + h));
            break;
        }
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
    return qEnvironmentVariableIntValue("VORTIA_NATIVE_TITLEBAR") == 0;
#else
    return false;
#endif
}

WindowChrome::WindowChrome(QMainWindow* window, QWidget* header, QBoxLayout* headerRow,
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

} // namespace vortia::ndl
