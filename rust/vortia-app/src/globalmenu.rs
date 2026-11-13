//! The app's menus in the shell's top bar, over D-Bus.
//!
//! The menus are exported as a `com.canonical.dbusmenu` object and registered with
//! `com.canonical.AppMenu.Registrar`, with the process id as the window id: on Wayland a window
//! has no global id, and Nodalix's bar pairs a menu with its window by the process that owns it.
//! This is the Rust counterpart of `ndl::exportGlobalMenu` in the Qt app.
//!
//! The menu is static in shape (a bar of menus of actions and separators); what changes at run
//! time is which actions are enabled ([`GlobalMenu::set_enabled`]). The shell's clicks come back
//! as action names through [`GlobalMenu::poll`].

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{fdo, interface};

const REGISTRAR: &str = "com.canonical.AppMenu.Registrar";
const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
const INTERFACE: &str = "com.canonical.dbusmenu";
// Not /MenuBar/N: toolkits that export their own menu bars use those paths.
const PATH: &str = "/com/getnodalia/GlobalMenu";

/// How often the registrar is looked for again: it may start after the app, or restart with the
/// shell.
const WATCH: Duration = Duration::from_secs(2);

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A command: its label and the name [`GlobalMenu::poll`] reports when it is clicked.
    Action {
        label: String,
        action: String,
    },
    Separator,
}

impl Entry {
    pub fn action(label: &str, action: &str) -> Self {
        Entry::Action {
            label: label.to_owned(),
            action: action.to_owned(),
        }
    }
}

/// A menu of the bar: its title and its entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub entries: Vec<Entry>,
}

/// An item of the exported tree. The ids are the order the items are listed in; 0 is the root.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Submenu {
        id: i32,
        label: String,
        children: Vec<Node>,
    },
    Action {
        id: i32,
        label: String,
        action: String,
    },
    Separator {
        id: i32,
    },
}

impl Node {
    fn id(&self) -> i32 {
        match self {
            Node::Submenu { id, .. } | Node::Action { id, .. } | Node::Separator { id } => *id,
        }
    }
}

fn build(menus: &[Menu]) -> Vec<Node> {
    let mut next = 0;
    let mut id = || {
        next += 1;
        next
    };
    menus
        .iter()
        .map(|menu| Node::Submenu {
            id: id(),
            label: menu.title.clone(),
            children: menu
                .entries
                .iter()
                .map(|entry| match entry {
                    Entry::Action { label, action } => Node::Action {
                        id: id(),
                        label: label.clone(),
                        action: action.clone(),
                    },
                    Entry::Separator => Node::Separator { id: id() },
                })
                .collect(),
        })
        .collect()
}

/// A dbusmenu label: mnemonics are `_` there, so a literal one doubles.
fn label(text: &str) -> String {
    text.replace('_', "__")
}

/// `(ia{sv}av)`: an item, its properties and its children, each child a variant of the same.
type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

fn owned<'a>(value: impl Into<Value<'a>>) -> Option<OwnedValue> {
    OwnedValue::try_from(value.into()).ok()
}

struct State {
    nodes: Vec<Node>,
    disabled: HashSet<String>,
    revision: u32,
}

impl State {
    fn find(&self, id: i32) -> Option<&Node> {
        fn walk(nodes: &[Node], id: i32) -> Option<&Node> {
            for node in nodes {
                if node.id() == id {
                    return Some(node);
                }
                if let Node::Submenu { children, .. } = node
                    && let Some(found) = walk(children, id)
                {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.nodes, id)
    }

    fn ids(&self) -> Vec<i32> {
        fn walk(nodes: &[Node], out: &mut Vec<i32>) {
            for node in nodes {
                out.push(node.id());
                if let Node::Submenu { children, .. } = node {
                    walk(children, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.nodes, &mut out);
        out
    }

    fn properties(&self, node: &Node) -> HashMap<String, OwnedValue> {
        let mut p = HashMap::new();
        let mut put = |key: &str, value: Option<OwnedValue>| {
            if let Some(value) = value {
                p.insert(key.to_owned(), value);
            }
        };
        match node {
            Node::Separator { .. } => put("type", owned("separator")),
            Node::Action {
                label: text,
                action,
                ..
            } => {
                put("label", owned(label(text)));
                put("enabled", owned(!self.disabled.contains(action)));
                put("visible", owned(true));
            }
            Node::Submenu { label: text, .. } => {
                put("label", owned(label(text)));
                put("enabled", owned(true));
                put("visible", owned(true));
                put("children-display", owned("submenu"));
            }
        }
        p
    }

    /// The tree under `node`, `depth` levels deep (negative: all of it).
    fn layout(&self, node: &Node, depth: i32) -> Layout {
        let children = match node {
            Node::Submenu { children, .. } if depth != 0 => children
                .iter()
                .filter_map(|child| owned(layout_value(self.layout(child, depth - 1))))
                .collect(),
            _ => Vec::new(),
        };
        (node.id(), self.properties(node), children)
    }

    fn root(&self, depth: i32) -> Layout {
        let mut properties = HashMap::new();
        if let Some(v) = owned("submenu") {
            properties.insert("children-display".to_owned(), v);
        }
        let children = if depth == 0 {
            Vec::new()
        } else {
            self.nodes
                .iter()
                .filter_map(|n| owned(layout_value(self.layout(n, depth - 1))))
                .collect()
        };
        (0, properties, children)
    }
}

/// A layout as the variant its parent holds it in.
fn layout_value(layout: Layout) -> Value<'static> {
    let (id, properties, children) = layout;
    let mut dict = zbus::zvariant::Dict::new(
        <String as zbus::zvariant::Type>::SIGNATURE,
        <OwnedValue as zbus::zvariant::Type>::SIGNATURE,
    );
    for (key, value) in properties {
        // A key and a value of the right types cannot be refused.
        let _ = dict.append(Value::from(key), Value::from(value));
    }
    let mut array = zbus::zvariant::Array::new(<OwnedValue as zbus::zvariant::Type>::SIGNATURE);
    for child in children {
        let _ = array.append(Value::from(child));
    }
    Value::Structure(
        zbus::zvariant::StructureBuilder::new()
            .add_field(id)
            .append_field(Value::Dict(dict))
            .append_field(Value::Array(array))
            .build()
            .unwrap_or_else(|_| zbus::zvariant::Structure::from((0_i32,))),
    )
}

struct Shared {
    state: Mutex<State>,
    clicks: Sender<String>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn click(&self, id: i32) {
        let action = {
            let state = self.state();
            match state.find(id) {
                Some(Node::Action { action, .. }) if !state.disabled.contains(action) => {
                    action.clone()
                }
                _ => return,
            }
        };
        if self.clicks.send(action).is_ok() {
            (self.wake)();
        }
    }
}

struct DbusMenu {
    shared: Arc<Shared>,
}

#[interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".to_owned()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "normal".to_owned()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> fdo::Result<(u32, Layout)> {
        let state = self.shared.state();
        let layout = if parent_id == 0 {
            state.root(recursion_depth)
        } else {
            let node = state
                .find(parent_id)
                .ok_or_else(|| fdo::Error::InvalidArgs(format!("no item {parent_id}")))?;
            state.layout(node, recursion_depth)
        };
        Ok((state.revision, layout))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let state = self.shared.state();
        // An empty list asks for every item.
        let ids = if ids.is_empty() { state.ids() } else { ids };
        ids.into_iter()
            .filter_map(|id| Some((id, state.properties(state.find(id)?))))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> fdo::Result<OwnedValue> {
        let state = self.shared.state();
        let node = state
            .find(id)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no item {id}")))?;
        state
            .properties(node)
            .remove(&name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("item {id} has no '{name}'")))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id == "clicked" {
            self.shared.click(id);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event_id, _, _) in events {
            if event_id == "clicked" {
                self.shared.click(id);
            }
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        let state = self.shared.state();
        let unknown = ids
            .into_iter()
            .filter(|id| state.find(*id).is_none())
            .collect();
        (Vec::new(), unknown)
    }
}

/// The exported menu. Dropping it takes the menu off the bus.
pub struct GlobalMenu {
    connection: Connection,
    shared: Arc<Shared>,
    clicks: Receiver<String>,
}

impl GlobalMenu {
    /// Exports `menus` on the session bus; `None` when there is no bus. `wake` is called after
    /// each click, so the window can poll again.
    pub fn start(menus: &[Menu], wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        Self::start_on(Builder::session().ok()?, menus, wake)
    }

    fn start_on(
        builder: Builder<'_>,
        menus: &[Menu],
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Option<Self> {
        let (tx, clicks) = channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                nodes: build(menus),
                disabled: HashSet::new(),
                revision: 1,
            }),
            clicks: tx,
            wake: Box::new(wake),
        });
        let connection = builder
            .serve_at(
                PATH,
                DbusMenu {
                    shared: shared.clone(),
                },
            )
            .ok()?
            .build()
            .ok()?;
        let watcher = connection.clone();
        let spawned = std::thread::Builder::new()
            .name("vortia-global-menu".into())
            .spawn(move || watch_registrar(&watcher));
        // Without the thread the menu is exported but never registered: the window keeps its own.
        drop(spawned);
        Some(Self {
            connection,
            shared,
            clicks,
        })
    }

    /// The actions the shell has clicked since the last call.
    pub fn poll(&self) -> Vec<String> {
        self.clicks.try_iter().collect()
    }

    /// Enables or disables an action, and tells the shell when that changed.
    pub fn set_enabled(&self, action: &str, enabled: bool) {
        let revision = {
            let mut state = self.shared.state();
            let changed = if enabled {
                state.disabled.remove(action)
            } else {
                state.disabled.insert(action.to_owned())
            };
            if !changed {
                return;
            }
            state.revision = state.revision.wrapping_add(1);
            state.revision
        };
        // A shell that is gone is not an error for the app.
        let _ = self.connection.emit_signal(
            None::<&str>,
            PATH,
            INTERFACE,
            "LayoutUpdated",
            &(revision, 0_i32),
        );
    }
}

/// Registers the menu with the registrar whenever one appears or is replaced.
fn watch_registrar(connection: &Connection) {
    let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(connection) else {
        return;
    };
    let Ok(name) = zbus::names::BusName::try_from(REGISTRAR) else {
        return;
    };
    let mut registered: Option<String> = None;
    loop {
        let owner = bus
            .get_name_owner(name.clone())
            .ok()
            .map(|owner| owner.to_string());
        if owner != registered {
            if owner.is_some() {
                register(connection);
            }
            registered = owner;
        }
        std::thread::sleep(WATCH);
    }
}

fn register(connection: &Connection) {
    let Ok(path) = ObjectPath::try_from(PATH) else {
        return;
    };
    let Ok(registrar) =
        zbus::blocking::Proxy::new(connection, REGISTRAR, REGISTRAR_PATH, REGISTRAR)
    else {
        return;
    };
    // A registrar that went away between the lookup and the call is found again next round.
    let _: Result<(), _> = registrar.call("RegisterWindow", &(std::process::id(), path));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menus() -> Vec<Menu> {
        vec![
            Menu {
                title: "Vortia".into(),
                entries: vec![
                    Entry::action("About Vortia", "about"),
                    Entry::Separator,
                    Entry::action("Quit", "quit"),
                ],
            },
            Menu {
                title: "Case_File".into(),
                entries: vec![Entry::action("Run", "run")],
            },
        ]
    }

    #[test]
    fn ids_follow_the_order_of_the_menus() {
        let nodes = build(&menus());
        let state = State {
            nodes,
            disabled: HashSet::new(),
            revision: 1,
        };
        assert_eq!(state.ids(), [1, 2, 3, 4, 5, 6]);
        assert!(matches!(state.find(3), Some(Node::Separator { .. })));
        assert!(state.find(99).is_none());
    }

    #[test]
    fn literal_underscores_double() {
        assert_eq!(label("Case_File"), "Case__File");
    }

    /// A private bus, or `None` where there is no `dbus-daemon` to start one.
    struct Bus {
        child: std::process::Child,
        address: String,
    }

    impl Bus {
        fn start() -> Option<Self> {
            use std::io::{BufRead, BufReader};
            let mut child = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .ok()?;
            let mut address = String::new();
            BufReader::new(child.stdout.take()?)
                .read_line(&mut address)
                .ok()?;
            Some(Self {
                child,
                address: address.trim().to_owned(),
            })
        }

        fn connect(&self) -> Builder<'static> {
            Builder::address(self.address.as_str()).expect("the daemon printed an address")
        }
    }

    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    struct FakeRegistrar {
        windows: Arc<Mutex<Vec<(u32, String)>>>,
    }

    #[interface(name = "com.canonical.AppMenu.Registrar")]
    impl FakeRegistrar {
        fn register_window(&self, window: u32, path: ObjectPath<'_>) {
            self.windows
                .lock()
                .unwrap()
                .push((window, path.to_string()));
        }
    }

    fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
        for _ in 0..100 {
            if let Some(found) = probe() {
                return Some(found);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    #[test]
    fn the_menu_is_served_registered_and_clickable_over_a_bus() {
        let Some(bus) = Bus::start() else {
            eprintln!("skipped: no dbus-daemon");
            return;
        };
        let woken = Arc::new(Mutex::new(0_u32));
        let counter = woken.clone();
        let menu = GlobalMenu::start_on(bus.connect(), &menus(), move || {
            *counter.lock().unwrap() += 1;
        })
        .expect("the menu starts");

        // The registrar appears after the app, as it does when the shell restarts.
        let windows = Arc::new(Mutex::new(Vec::new()));
        let registrar = bus
            .connect()
            .name(REGISTRAR)
            .unwrap()
            .serve_at(
                REGISTRAR_PATH,
                FakeRegistrar {
                    windows: windows.clone(),
                },
            )
            .unwrap()
            .build()
            .expect("the registrar starts");
        let seen = wait_for(|| windows.lock().unwrap().first().cloned());
        assert_eq!(seen, Some((std::process::id(), PATH.to_owned())));

        // A client reads the layout the way the shell does.
        let client = bus.connect().build().unwrap();
        let name = menu.connection.unique_name().unwrap().to_string();
        let proxy = zbus::blocking::Proxy::new(&client, name.as_str(), PATH, INTERFACE).unwrap();
        let (revision, (root, _, children)): (u32, Layout) = proxy
            .call("GetLayout", &(0_i32, -1_i32, Vec::<String>::new()))
            .unwrap();
        assert_eq!((revision, root, children.len()), (1, 0, 2));
        let all: Vec<(i32, HashMap<String, OwnedValue>)> = proxy
            .call(
                "GetGroupProperties",
                &(Vec::<i32>::new(), Vec::<String>::new()),
            )
            .unwrap();
        assert_eq!(all.len(), 6);
        let label: String = proxy
            .call::<_, _, OwnedValue>("GetProperty", &(4_i32, "label"))
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(label, "Quit");

        // A click arrives as the action's name, and wakes the window.
        let _: () = proxy
            .call("Event", &(4_i32, "clicked", Value::from(0_i32), 0_u32))
            .unwrap();
        assert!(wait_for(|| (*woken.lock().unwrap() > 0).then_some(())).is_some());
        assert_eq!(menu.poll(), ["quit"]);

        // A disabled action does not click, and the change is announced.
        menu.set_enabled("quit", false);
        let _: () = proxy
            .call("Event", &(4_i32, "clicked", Value::from(0_i32), 0_u32))
            .unwrap();
        assert!(menu.poll().is_empty());
        let (revision, _): (u32, Layout) = proxy
            .call("GetLayout", &(0_i32, 1_i32, Vec::<String>::new()))
            .unwrap();
        assert_eq!(revision, 2);
        drop(registrar);
    }
}
