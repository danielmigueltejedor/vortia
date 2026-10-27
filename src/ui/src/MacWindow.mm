#include <QWidget>
#import <AppKit/AppKit.h>

namespace vortia {
// QWidget's macOS window ID is its NSView. Keep the native window title for
// accessibility and window menus, while document tabs supply the visible label.
void hideNativeWindowTitle(QWidget* host) {
    auto* view=reinterpret_cast<NSView*>(host->winId());
    if(auto* window=[view window]) [window setTitleVisibility:NSWindowTitleHidden];
}
}
