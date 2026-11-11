#include "vortia/ui/GlobalMenu.hpp"

#include <QtGlobal>

#if defined(Q_OS_LINUX)

#include <QAction>
#include <QActionEvent>
#include <QActionGroup>
#include <QCoreApplication>
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusMetaType>
#include <QDBusObjectPath>
#include <QDBusPendingCallWatcher>
#include <QDBusServiceWatcher>
#include <QDBusVariant>
#include <QDBusVirtualObject>
#include <QEvent>
#include <QGuiApplication>
#include <QHash>
#include <QMenu>
#include <QMenuBar>
#include <QPointer>
#include <QTimer>

namespace {

constexpr auto kRegistrar = "com.canonical.AppMenu.Registrar";
constexpr auto kRegistrarPath = "/com/canonical/AppMenu/Registrar";
constexpr auto kMenuInterface = "com.canonical.dbusmenu";
// Not /MenuBar/N: Qt exports its own menu bars there (unregistered on Wayland).
constexpr auto kMenuPath = "/com/getnodalia/GlobalMenu";

/// One node of `GetLayout`'s answer, `(ia{sv}av)`: the children nest as variants.
struct LayoutItem {
    int id = 0;
    QVariantMap properties;
    QList<LayoutItem> children;
};

/// One row of `GetGroupProperties`' answer, `(ia{sv})`.
struct ItemProperties {
    int id = 0;
    QVariantMap properties;
};

} // namespace

Q_DECLARE_METATYPE(LayoutItem)
Q_DECLARE_METATYPE(ItemProperties)
Q_DECLARE_METATYPE(QList<ItemProperties>)

namespace {

QDBusArgument& operator<<(QDBusArgument& argument, const LayoutItem& item) {
    argument.beginStructure();
    argument << item.id << item.properties;
    argument.beginArray(qMetaTypeId<QDBusVariant>());
    for (const LayoutItem& child : item.children)
        argument << QDBusVariant(QVariant::fromValue(child));
    argument.endArray();
    argument.endStructure();
    return argument;
}

// Only ever sent, never read; QtDBus still wants both directions to register the type.
const QDBusArgument& operator>>(const QDBusArgument& argument, LayoutItem&) {
    return argument;
}

QDBusArgument& operator<<(QDBusArgument& argument, const ItemProperties& item) {
    argument.beginStructure();
    argument << item.id << item.properties;
    argument.endStructure();
    return argument;
}

const QDBusArgument& operator>>(const QDBusArgument& argument, ItemProperties&) {
    return argument;
}

/// A dbusmenu label: mnemonics are `_` there, `&` in Qt, and a literal `_` doubles.
QString menuLabel(QString text) {
    text.replace(QLatin1Char('_'), QStringLiteral("__"));
    QString out;
    for (int i = 0; i < text.size(); ++i) {
        if (text[i] == QLatin1Char('&') && i + 1 < text.size()) {
            out += text[i + 1] == QLatin1Char('&') ? QStringLiteral("&") : QStringLiteral("_") + text[i + 1];
            ++i;
        } else {
            out += text[i];
        }
    }
    return out;
}

const char* const kIntrospection = R"xml(<interface name="com.canonical.dbusmenu">
  <property name="Version" type="u" access="read"/>
  <property name="TextDirection" type="s" access="read"/>
  <property name="Status" type="s" access="read"/>
  <property name="IconThemePath" type="as" access="read"/>
  <method name="GetLayout">
    <arg type="i" name="parentId" direction="in"/><arg type="i" name="recursionDepth" direction="in"/>
    <arg type="as" name="propertyNames" direction="in"/>
    <arg type="u" name="revision" direction="out"/><arg type="(ia{sv}av)" name="layout" direction="out"/>
  </method>
  <method name="GetGroupProperties">
    <arg type="ai" name="ids" direction="in"/><arg type="as" name="propertyNames" direction="in"/>
    <arg type="a(ia{sv})" name="properties" direction="out"/>
  </method>
  <method name="GetProperty">
    <arg type="i" name="id" direction="in"/><arg type="s" name="name" direction="in"/>
    <arg type="v" name="value" direction="out"/>
  </method>
  <method name="Event">
    <arg type="i" name="id" direction="in"/><arg type="s" name="eventId" direction="in"/>
    <arg type="v" name="data" direction="in"/><arg type="u" name="timestamp" direction="in"/>
  </method>
  <method name="EventGroup">
    <arg type="a(isvu)" name="events" direction="in"/><arg type="ai" name="idErrors" direction="out"/>
  </method>
  <method name="AboutToShow">
    <arg type="i" name="id" direction="in"/><arg type="b" name="needUpdate" direction="out"/>
  </method>
  <method name="AboutToShowGroup">
    <arg type="ai" name="ids" direction="in"/><arg type="ai" name="updatesNeeded" direction="out"/>
    <arg type="ai" name="idErrors" direction="out"/>
  </method>
  <signal name="ItemsPropertiesUpdated">
    <arg type="a(ia{sv})" name="updatedProps"/><arg type="a(ias)" name="removedProps"/>
  </signal>
  <signal name="LayoutUpdated"><arg type="u" name="revision"/><arg type="i" name="parent"/></signal>
  <signal name="ItemActivationRequested"><arg type="i" name="id"/><arg type="u" name="timestamp"/></signal>
</interface>
)xml";

/// The menu bar as a `com.canonical.dbusmenu` object, read live from the widgets.
class MenuObject final : public QDBusVirtualObject {
public:
    explicit MenuObject(QMenuBar* bar) : bar_(bar) {
        // Any change to a menu (an entry enabled, added, renamed) is announced once the event
        // loop is idle, so a burst of changes is one LayoutUpdated.
        announce_.setSingleShot(true);
        announce_.setInterval(0);
        QObject::connect(&announce_, &QTimer::timeout, &announce_, [this] {
            ++revision_;
            QDBusMessage signal = QDBusMessage::createSignal(QString::fromLatin1(kMenuPath),
                                                             QString::fromLatin1(kMenuInterface),
                                                             QStringLiteral("LayoutUpdated"));
            signal << revision_ << 0;
            QDBusConnection::sessionBus().send(signal);
        });
    }

    QString introspect(const QString&) const override { return QString::fromLatin1(kIntrospection); }

    bool handleMessage(const QDBusMessage& message, const QDBusConnection& connection) override {
        const QList<QVariant> args = message.arguments();
        const QString member = message.member();
        QDBusMessage reply;
        if (message.interface() == QStringLiteral("org.freedesktop.DBus.Properties")) {
            const QVariantMap all{{QStringLiteral("Version"), 3U},
                                  {QStringLiteral("TextDirection"), QStringLiteral("ltr")},
                                  {QStringLiteral("Status"), QStringLiteral("normal")},
                                  {QStringLiteral("IconThemePath"), QStringList()}};
            if (member == QStringLiteral("GetAll"))
                reply = message.createReply(QVariant(all));
            else if (member == QStringLiteral("Get"))
                reply = message.createReply(QVariant::fromValue(QDBusVariant(all.value(args.value(1).toString()))));
            else
                return false;
        } else if (member == QStringLiteral("GetLayout")) {
            reply = message.createReply();
            reply << revision_ << QVariant::fromValue(layout(args.value(0).toInt(), args.value(1).toInt()));
        } else if (member == QStringLiteral("GetGroupProperties")) {
            QList<ItemProperties> rows;
            // An empty list asks for every item (the dbusmenu contract).
            QList<int> ids = qdbus_cast<QList<int>>(args.value(0));
            if (ids.isEmpty())
                ids = actions_.keys();
            for (int id : ids)
                if (QAction* action = actions_.value(id))
                    rows.append({id, properties(action)});
            reply = message.createReply(QVariant::fromValue(rows));
        } else if (member == QStringLiteral("GetProperty")) {
            QAction* action = actions_.value(args.value(0).toInt());
            const QVariant value = action ? properties(action).value(args.value(1).toString()) : QVariant();
            reply = message.createReply(QVariant::fromValue(QDBusVariant(value)));
        } else if (member == QStringLiteral("Event")) {
            event(args.value(0).toInt(), args.value(1).toString());
            reply = message.createReply();
        } else if (member == QStringLiteral("EventGroup")) {
            const QDBusArgument events = args.value(0).value<QDBusArgument>();
            events.beginArray();
            while (!events.atEnd()) {
                int id = 0;
                QString kind;
                QDBusVariant data;
                uint timestamp = 0;
                events.beginStructure();
                events >> id >> kind >> data >> timestamp;
                events.endStructure();
                event(id, kind);
            }
            events.endArray();
            reply = message.createReply(QVariant::fromValue(QList<int>()));
        } else if (member == QStringLiteral("AboutToShow")) {
            // Menus Vortia fills when they open are filled before the shell reads them.
            if (QAction* action = actions_.value(args.value(0).toInt()); action && action->menu())
                emit action->menu()->aboutToShow();
            reply = message.createReply(QVariant(true));
        } else if (member == QStringLiteral("AboutToShowGroup")) {
            // The grouped form of AboutToShow: fill each menu; unknown ids are reported back.
            QList<int> unknown;
            for (int id : qdbus_cast<QList<int>>(args.value(0))) {
                QAction* action = actions_.value(id);
                if (action && action->menu())
                    emit action->menu()->aboutToShow();
                else if (!action)
                    unknown.append(id);
            }
            reply = message.createReply();
            reply << QVariant::fromValue(QList<int>()) << QVariant::fromValue(unknown);
        } else {
            return false;
        }
        return connection.send(reply);
    }

    /// Watches a menu, and every menu inside it, for changes worth re-reading.
    void watch(QWidget* menu) {
        menu->installEventFilter(&changes_);
        changes_.owner = this;
        // Submenus already in place (Camera, Display…) are not announced by ActionAdded: walk
        // them now, or their checks go stale in the desktop's bar.
        for (QAction* action : menu->actions()) {
            if (QMenu* sub = action->menu())
                watch(sub);
        }
    }

private:
    struct ChangeFilter final : public QObject {
        MenuObject* owner = nullptr;
        bool eventFilter(QObject* watched, QEvent* event) override {
            switch (event->type()) {
            case QEvent::ActionAdded:
                if (auto* added = static_cast<QActionEvent*>(event)->action(); added && added->menu())
                    owner->watch(added->menu());
                [[fallthrough]];
            case QEvent::ActionChanged:
            case QEvent::ActionRemoved:
                owner->announce_.start();
                break;
            default:
                break;
            }
            return QObject::eventFilter(watched, event);
        }
    };

    int idFor(QAction* action) {
        if (auto it = ids_.constFind(action); it != ids_.constEnd())
            return *it;
        const int id = next_++;
        ids_.insert(action, id);
        actions_.insert(id, action);
        return id;
    }

    QVariantMap properties(QAction* action) const {
        QVariantMap p;
        if (action->isSeparator()) {
            p.insert(QStringLiteral("type"), QStringLiteral("separator"));
            return p;
        }
        p.insert(QStringLiteral("label"), menuLabel(action->text()));
        p.insert(QStringLiteral("enabled"), action->isEnabled());
        p.insert(QStringLiteral("visible"), action->isVisible());
        if (action->menu())
            p.insert(QStringLiteral("children-display"), QStringLiteral("submenu"));
        if (action->isCheckable()) {
            const bool radio = action->actionGroup() && action->actionGroup()->isExclusive();
            p.insert(QStringLiteral("toggle-type"), radio ? QStringLiteral("radio") : QStringLiteral("checkmark"));
            p.insert(QStringLiteral("toggle-state"), action->isChecked() ? 1 : 0);
        }
        return p;
    }

    LayoutItem item(QAction* action, int depth) {
        LayoutItem node{idFor(action), properties(action), {}};
        if (QMenu* menu = action->menu(); menu && depth != 0)
            for (QAction* child : menu->actions())
                node.children.append(item(child, depth - 1));
        return node;
    }

    LayoutItem layout(int parent, int depth) {
        if (QAction* action = actions_.value(parent))
            return item(action, depth);
        LayoutItem root{0, {{QStringLiteral("children-display"), QStringLiteral("submenu")}}, {}};
        if (bar_ && depth != 0)
            for (QAction* action : bar_->actions())
                root.children.append(item(action, depth - 1));
        return root;
    }

    void event(int id, const QString& kind) {
        QPointer<QAction> action = actions_.value(id);
        if (kind != QStringLiteral("clicked") || !action || action->menu())
            return;
        // Queued: the shell is still waiting on this call, and the action may open a dialog.
        QMetaObject::invokeMethod(action, [action] {
            if (action && action->isEnabled())
                action->trigger();
        }, Qt::QueuedConnection);
    }

    QPointer<QMenuBar> bar_;
    QHash<QAction*, int> ids_;
    QHash<int, QPointer<QAction>> actions_;
    int next_ = 1;
    uint revision_ = 1;
    QTimer announce_;
    ChangeFilter changes_;
};

/// Set when the registrar refused the registration: the menus then stay in the window.
bool registrationFailed = false;

/// Tells the registrar where the menu is. The window id is this process's id: on Wayland a
/// window has no global id, and Nodalix's bar finds the menu through the process that owns it.
void registerWithRegistrar() {
    QDBusMessage call = QDBusMessage::createMethodCall(QString::fromLatin1(kRegistrar),
                                                       QString::fromLatin1(kRegistrarPath),
                                                       QString::fromLatin1(kRegistrar),
                                                       QStringLiteral("RegisterWindow"));
    call << static_cast<uint>(QCoreApplication::applicationPid())
         << QDBusObjectPath(QString::fromLatin1(kMenuPath));
    auto* pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(call));
    QObject::connect(pending, &QDBusPendingCallWatcher::finished, [](QDBusPendingCallWatcher* done) {
        registrationFailed = done->isError();
        done->deleteLater();
    });
}

} // namespace

namespace vortia::ndl {

bool globalMenuAvailable() {
    if (!QGuiApplication::platformName().startsWith(QStringLiteral("wayland")) || registrationFailed)
        return false;
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.isConnected() || !bus.interface())
        return false;
    // Only a registrar that pairs a menu with its window by process can show it: Nodalix's,
    // whose process also owns the global-menu service. Another registrar expects an X11 window
    // id, and the menus would vanish from the window without reaching a bar.
    const QString registrar = bus.interface()->serviceOwner(QString::fromLatin1(kRegistrar));
    const QString nodalix = bus.interface()->serviceOwner(QStringLiteral("space.unmade.GlobalMenu"));
    return !registrar.isEmpty() && registrar == nodalix;
}

void exportGlobalMenu(QMenuBar* menuBar) {
    // On X11 Qt registers the menu bar by itself; elsewhere there is no top bar to hand it to.
    if (!menuBar || !QGuiApplication::platformName().startsWith(QStringLiteral("wayland")))
        return;
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.isConnected())
        return;
    qDBusRegisterMetaType<LayoutItem>();
    qDBusRegisterMetaType<ItemProperties>();
    qDBusRegisterMetaType<QList<ItemProperties>>();

    static MenuObject* exported = nullptr;
    if (exported)
        return;
    exported = new MenuObject(menuBar);
    if (!bus.registerVirtualObject(QString::fromLatin1(kMenuPath), exported)) {
        delete exported;
        exported = nullptr;
        return;
    }
    exported->watch(menuBar);
    for (QAction* action : menuBar->actions())
        if (action->menu())
            exported->watch(action->menu());

    // The registrar may start after Vortia (or restart with the shell): register whenever it
    // appears.
    auto* watcher = new QDBusServiceWatcher(QString::fromLatin1(kRegistrar), bus,
                                            QDBusServiceWatcher::WatchForRegistration, menuBar);
    QObject::connect(watcher, &QDBusServiceWatcher::serviceRegistered, menuBar,
                     [](const QString&) { registerWithRegistrar(); });
    if (bus.interface()->isServiceRegistered(QString::fromLatin1(kRegistrar)))
        registerWithRegistrar();
}

} // namespace vortia::ndl

#else

namespace vortia::ndl {

void exportGlobalMenu(QMenuBar*) {}

bool globalMenuAvailable() {
    return false;
}

} // namespace vortia::ndl

#endif
