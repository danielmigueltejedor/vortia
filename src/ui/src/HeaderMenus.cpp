#include "vortia/ui/HeaderMenus.hpp"

#include <QHBoxLayout>
#include <QMenu>
#include <QMenuBar>
#include <QToolButton>
#ifdef VORTIA_HAS_QTDBUS
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusServiceWatcher>
#endif

namespace vortia::ndl {

QString HeaderMenus::registrarService() {
    return QStringLiteral("com.canonical.AppMenu.Registrar");
}

bool HeaderMenus::shellHasGlobalMenu([[maybe_unused]] const QString& service) {
#if defined(Q_OS_MACOS)
    return true;
#elif defined(VORTIA_HAS_QTDBUS)
    const auto* bus = QDBusConnection::sessionBus().interface();
    return bus && bus->isServiceRegistered(service).value();
#else
    return false;
#endif
}

HeaderMenus::HeaderMenus(QMenuBar* source, QWidget* parent,
                         [[maybe_unused]] const QString& service)
    : QWidget(parent), button_(new QToolButton(this)), inline_(new QMenuBar(this)) {
    auto* row = new QHBoxLayout(this);
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(0);

    button_->setObjectName(QStringLiteral("ndl-app-menu"));
    button_->setText(QStringLiteral("≡"));
    button_->setToolTip(tr("Main menu"));
    button_->setPopupMode(QToolButton::InstantPopup);
    button_->setFocusPolicy(Qt::NoFocus);
    auto* menu = new QMenu(button_);
    // Read at open time: the window may add menus after the header is built.
    connect(menu, &QMenu::aboutToShow, this, [menu, source] {
        menu->clear();
        if (source)
            menu->addActions(source->actions());
    });
    button_->setMenu(menu);

    inline_->setObjectName(QStringLiteral("vortia-header-menus"));
    inline_->setNativeMenuBar(false);
    if (source)
        inline_->addActions(source->actions());

    row->addWidget(button_);
    row->addWidget(inline_);
    setShellHasGlobalMenu(shellHasGlobalMenu(service));

#ifdef VORTIA_HAS_QTDBUS
    // The shell's menu service can start or stop while Vortia runs.
    auto* watcher = new QDBusServiceWatcher(service, QDBusConnection::sessionBus(),
                                            QDBusServiceWatcher::WatchForOwnerChange, this);
    connect(watcher, &QDBusServiceWatcher::serviceRegistered, this,
            [this] { setShellHasGlobalMenu(true); });
    connect(watcher, &QDBusServiceWatcher::serviceUnregistered, this,
            [this] { setShellHasGlobalMenu(false); });
#endif
}

void HeaderMenus::setShellHasGlobalMenu(bool global) {
    button_->setVisible(global);
    inline_->setVisible(!global);
}

}  // namespace vortia::ndl
