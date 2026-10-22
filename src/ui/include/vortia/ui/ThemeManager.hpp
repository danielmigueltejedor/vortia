#pragma once

#include <QString>

class QApplication;

namespace vortia {

/// Applies the Vortia visual identity (original design, no third-party
/// assets): a neutral scientific palette in dark and light variants.
class ThemeManager {
public:
    enum class Theme { Dark, Light };

    static void apply(QApplication& app, Theme theme);
    [[nodiscard]] static Theme fromName(const QString& name);
    [[nodiscard]] static QString toName(Theme theme);
};

} // namespace vortia
