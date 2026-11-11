#pragma once

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
void exportGlobalMenu(QMenuBar* menuBar);

} // namespace vortia::ndl
