#pragma once

#include <QJsonObject>
#include <QString>
#include <QStringList>

class QApplication;

namespace vortia {

/// Lienzo: the design language shared by Nodalia desktop apps. Tokens live in
/// `resources/lienzo/tokens.json` and the Qt style sheet in
/// `resources/lienzo/lienzo.qss`; both are plain files another app can reuse.
namespace lienzo {

/// The tokens bundled with the application (`:/lienzo/tokens.json`).
[[nodiscard]] QJsonObject bundledTokens();
/// The style-sheet template bundled with the application (`:/lienzo/lienzo.qss`).
[[nodiscard]] QString bundledStyleSheetTemplate();

/// Value of `group.name` (for example "color.accent", "radius.panel") for
/// `scheme` ("light" or "dark"); colours are looked up under the scheme.
/// Empty when the token does not exist.
[[nodiscard]] QString tokenValue(const QJsonObject& tokens, const QString& scheme,
                                 const QString& reference);

/// Replaces every `{{group.name}}` placeholder of `styleTemplate`. References
/// that do not resolve are left in place and reported in `missing`.
[[nodiscard]] QString renderStyleSheet(const QJsonObject& tokens, const QString& scheme,
                                       const QString& styleTemplate,
                                       QStringList* missing = nullptr);

} // namespace lienzo

/// Applies the Lienzo design language to the application.
class ThemeManager {
public:
    enum class Theme { Dark, Light };

    static void apply(QApplication& app, Theme theme);
    [[nodiscard]] static Theme fromName(const QString& name);
    /// The theme to show for a saved preference. `VORTIA_THEME=light|dark`
    /// overrides it for the current run (design review, CI screenshots)
    /// without touching the preference.
    [[nodiscard]] static Theme resolve(const QString& savedName);
    [[nodiscard]] static QString toName(Theme theme);
};

} // namespace vortia
