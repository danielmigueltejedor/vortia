#include "vortia/ui/WelcomeScreen.hpp"

#include <QDir>
#include <QEvent>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QLabel>
#include <QMouseEvent>
#include <QPixmap>
#include <QPushButton>
#include <algorithm>
#include <QVBoxLayout>

namespace vortia {

namespace {

/// One recent project: its folder's name, and where it is, smaller. The whole row is the target.
class RecentRow final : public QWidget {
public:
    RecentRow(const QString& folder, const QIcon& icon, std::function<void()> open, QWidget* parent)
        : QWidget(parent), open_(std::move(open)) {
        setObjectName(QStringLiteral("vortia-welcome-recent-row"));
        setAttribute(Qt::WA_StyledBackground, true);
        setAttribute(Qt::WA_Hover, true);
        setCursor(Qt::PointingHandCursor);
        setToolTip(QDir::toNativeSeparators(folder));
        auto* row = new QHBoxLayout(this);
        row->setContentsMargins(12, 7, 12, 7);
        row->setSpacing(10);
        auto* glyph = new QLabel(this);
        glyph->setPixmap(icon.pixmap(18, 18));
        glyph->setFixedSize(18, 18);
        row->addWidget(glyph, 0, Qt::AlignVCenter);
        auto* text = new QVBoxLayout;
        text->setSpacing(1);
        const QFileInfo info(folder);
        auto* name = new QLabel(info.fileName().isEmpty() ? folder : info.fileName(), this);
        name->setObjectName(QStringLiteral("vortia-welcome-recent-name"));
        auto* where = new QLabel(this);
        where->setObjectName(QStringLiteral("vortia-welcome-recent-path"));
        const QString parentPath = QDir::toNativeSeparators(info.absolutePath());
        where->setText(fontMetrics().elidedText(parentPath, Qt::ElideMiddle, 250));
        text->addWidget(name);
        text->addWidget(where);
        row->addLayout(text, 1);
    }

protected:
    void mouseReleaseEvent(QMouseEvent* event) override {
        if (event->button() == Qt::LeftButton && rect().contains(event->position().toPoint()) && open_)
            open_();
        QWidget::mouseReleaseEvent(event);
    }

private:
    std::function<void()> open_;
};

} // namespace

QStringList rememberRecentProject(QStringList recent, const QString& folder, int limit) {
    const QString clean = QDir::cleanPath(folder);
    if (clean.isEmpty())
        return recent;
    recent.removeAll(clean);
    recent.prepend(clean);
    while (recent.size() > limit)
        recent.removeLast();
    return recent;
}

WelcomeScreen::WelcomeScreen(QWidget* window, std::function<int()> top)
    : QWidget(window), top_(std::move(top)) {
    setObjectName(QStringLiteral("vortia-welcome"));
    setAttribute(Qt::WA_StyledBackground, true);

    auto* columns = new QHBoxLayout(this);
    columns->setContentsMargins(0, 0, 0, 0);
    columns->setSpacing(0);

    auto* main = new QWidget(this);
    auto* stack = new QVBoxLayout(main);
    stack->setContentsMargins(40, 40, 40, 40);
    stack->setSpacing(16);
    stack->addStretch(1);
    auto* icon = new QLabel(main);
    icon->setAlignment(Qt::AlignCenter);
    const QPixmap appIcon(QStringLiteral(":/vortia/app-icon.png"));
    if (!appIcon.isNull()) {
        const qreal ratio = devicePixelRatioF();
        QPixmap scaled = appIcon.scaled(QSize(112, 112) * ratio, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        scaled.setDevicePixelRatio(ratio);
        icon->setPixmap(scaled);
    }
    stack->addWidget(icon, 0, Qt::AlignHCenter);
    title_ = new QLabel(QStringLiteral("Vortia"), main);
    title_->setObjectName(QStringLiteral("vortia-welcome-title"));
    title_->setAlignment(Qt::AlignCenter);
    stack->addWidget(title_);
    tagline_ = new QLabel(main);
    tagline_->setObjectName(QStringLiteral("vortia-welcome-tagline"));
    tagline_->setAlignment(Qt::AlignCenter);
    stack->addWidget(tagline_);
    body_ = new QLabel(main);
    body_->setObjectName(QStringLiteral("vortia-welcome-body"));
    body_->setAlignment(Qt::AlignCenter);
    body_->setWordWrap(true);
    body_->setFixedWidth(400);
    // Centred by stretches, not by alignment: an aligned label loses its height for width and clips.
    auto* bodyRow = new QHBoxLayout;
    bodyRow->addStretch(1);
    bodyRow->addWidget(body_);
    bodyRow->addStretch(1);
    stack->addLayout(bodyRow);
    auto* buttons = new QHBoxLayout;
    buttons->setSpacing(12);
    buttons->addStretch(1);
    newButton_ = new QPushButton(main);
    newButton_->setObjectName(QStringLiteral("vortia-welcome-new"));
    newButton_->setProperty("ndlVariant", QStringLiteral("primary"));
    newButton_->setDefault(true);
    newButton_->setIconSize(QSize(16, 16));
    openButton_ = new QPushButton(main);
    openButton_->setObjectName(QStringLiteral("vortia-welcome-open"));
    openButton_->setProperty("ndlVariant", QStringLiteral("neutral"));
    openButton_->setIconSize(QSize(16, 16));
    for (auto* button : {newButton_, openButton_}) {
        button->setMinimumHeight(36);
        button->setCursor(Qt::PointingHandCursor);
        buttons->addWidget(button);
    }
    buttons->addStretch(1);
    stack->addSpacing(8);
    stack->addLayout(buttons);
    stack->addStretch(1);
    columns->addWidget(main, 1);

    recentColumn_ = new QWidget(this);
    recentColumn_->setObjectName(QStringLiteral("vortia-welcome-recent"));
    recentColumn_->setAttribute(Qt::WA_StyledBackground, true);
    recentColumn_->setFixedWidth(320);
    auto* recentLayout = new QVBoxLayout(recentColumn_);
    recentLayout->setContentsMargins(16, 40, 16, 40);
    recentLayout->setSpacing(4);
    recentTitle_ = new QLabel(recentColumn_);
    recentTitle_->setObjectName(QStringLiteral("vortia-welcome-recent-title"));
    recentTitle_->setContentsMargins(12, 0, 12, 6);
    recentLayout->addWidget(recentTitle_);
    recentRows_ = new QVBoxLayout;
    recentRows_->setSpacing(2);
    recentLayout->addLayout(recentRows_);
    recentLayout->addStretch(1);
    columns->addWidget(recentColumn_);
    recentColumn_->hide();

    connect(newButton_, &QPushButton::clicked, this, &WelcomeScreen::newProjectRequested);
    connect(openButton_, &QPushButton::clicked, this, &WelcomeScreen::openProjectRequested);
    window->installEventFilter(this);
    retranslate();
    place();
}

void WelcomeScreen::setActionIcons(const QIcon& newProject, const QIcon& openProject, const QIcon& folder) {
    newButton_->setIcon(newProject);
    openButton_->setIcon(openProject);
    folderIcon_ = folder;
    setRecentProjects(recent_);
}

void WelcomeScreen::setRecentProjects(const QStringList& folders) {
    recent_.clear();
    for (const auto& folder : folders)
        if (QFileInfo(folder).isDir())
            recent_ << folder;
    while (auto* item = recentRows_->takeAt(0)) {
        delete item->widget();
        delete item;
    }
    for (const auto& folder : recent_)
        recentRows_->addWidget(new RecentRow(folder, folderIcon_, [this, folder] {
            emit recentProjectRequested(folder);
        }, recentColumn_));
    recentColumn_->setVisible(!recent_.isEmpty());
}

void WelcomeScreen::place() {
    auto* window = parentWidget();
    if (!window || isHidden())
        return;
    const int y = top_ ? std::clamp(top_(), 0, window->height()) : 0;
    setGeometry(0, y, window->width(), window->height() - y);
    raise();
    // Over the work area, under the window's own resize handles.
    for (auto* handle : window->findChildren<QWidget*>(QStringLiteral("ndl-resize-handle"),
                                                        Qt::FindDirectChildrenOnly))
        handle->raise();
}

void WelcomeScreen::follow(QWidget* bar) {
    if (bar)
        bar->installEventFilter(this);
}

bool WelcomeScreen::eventFilter(QObject* watched, QEvent* event) {
    const auto type = event->type();
    const bool window = watched == parentWidget()
        && (type == QEvent::Resize || type == QEvent::LayoutRequest || type == QEvent::Show);
    const bool bar = watched != parentWidget()
        && (type == QEvent::Resize || type == QEvent::Move || type == QEvent::Show
            || type == QEvent::Hide);
    if (window || bar)
        place();
    return QWidget::eventFilter(watched, event);
}

void WelcomeScreen::changeEvent(QEvent* event) {
    if (event->type() == QEvent::LanguageChange)
        retranslate();
    QWidget::changeEvent(event);
}

void WelcomeScreen::retranslate() {
    tagline_->setText(tr("Engineering the flow"));
    body_->setText(tr("Create a project and draw the fluid domain, or open one you already have."));
    newButton_->setText(tr("New Project"));
    openButton_->setText(tr("Open Project…"));
    recentTitle_->setText(tr("Recent projects"));
}

} // namespace vortia
