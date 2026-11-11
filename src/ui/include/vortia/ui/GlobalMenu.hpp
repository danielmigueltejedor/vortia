#pragma once

#include <functional>

class QMenuBar;

namespace vortia::ndl {

/// Hands the window's menu bar to the desktop's top bar on Linux under Wayland.
///
/// Nodalix (and any shell with an AppMenu registrar) shows an app's menus in its top bar. Qt
/// registers its menu bar there by itself on X11, but not on Wayland, where it only speaks a KDE
/// protocol. This exports the menu bar over `com.canonical.dbusmenu` and registers it with
/// `com.canonical.AppMenu.Registrar`; the shell pairs it with the window through the process id.
/// Does nothing elsewhere, or when no registrar is running. The menu bar stays the source of
/// truth: the exported layout is read from it on every request.
/// `registered` hears whether the registrar accepted the menu (and again if it restarts), so
/// the window hides its own menus only once the bar really has them.
void exportGlobalMenu(QMenuBar* menuBar, std::function<void(bool)> registered = {});

/// Whether the menus are in the desktop's top bar: Linux under Wayland with Nodalix's AppMenu
/// registrar (it pairs a menu with its window by process), and a registration it accepted.
/// Otherwise the window keeps its menus.
[[nodiscard]] bool globalMenuAvailable();

} // namespace vortia::ndl
