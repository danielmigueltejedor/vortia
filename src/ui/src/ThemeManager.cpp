#include "vortia/ui/ThemeManager.hpp"

#include <QAbstractSpinBox>
#include <QApplication>
#include <QEvent>
#include <QLineEdit>
#include <QColor>
#include <QDir>
#include <QFile>
#include <QFont>
#include <QFontDatabase>
#include <QJsonDocument>
#include <QJsonValue>
#include <QPalette>
#include <QRegularExpression>
#include <QStandardPaths>
#include <QStyleFactory>

namespace vortia {

namespace ndl {

namespace {

QByteArray readResource(const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray{};
}

} // namespace

QJsonObject bundledTokens() {
    return QJsonDocument::fromJson(readResource(QStringLiteral(":/ndl/nodalia-qt.json"))).object();
}

QString bundledStyleSheetTemplate() {
    return QString::fromUtf8(readResource(QStringLiteral(":/ndl/nodalia.qss")) + '\n'
                             + readResource(QStringLiteral(":/ndl/vortia.qss")));
}

QString tokenValue(const QJsonObject& tokens, const QString& scheme, const QString& reference,
                   const QString& density) {
    const QStringList parts = reference.split(QLatin1Char('.'));
    if (parts.size() != 2)
        return {};
    QJsonObject group = tokens.value(parts[0]).toObject();
    if (parts[0] == QStringLiteral("color"))
        group = group.value(scheme).toObject();
    else if (parts[0] == QStringLiteral("density"))
        group = group.value(density).toObject();
    const QJsonValue value = group.value(parts[1]);
    if (value.isString())
        return value.toString();
    if (value.isDouble())
        return QString::number(value.toDouble());
    return {};
}

QString renderStyleSheet(const QJsonObject& tokens, const QString& scheme,
                         const QString& styleTemplate, QStringList* missing,
                         const QString& density) {
    static const QRegularExpression placeholder(QStringLiteral(R"(\{\{([A-Za-z]+\.[A-Za-z]+)\}\})"));
    QString rendered;
    rendered.reserve(styleTemplate.size());
    qsizetype position = 0;
    auto matches = placeholder.globalMatch(styleTemplate);
    while (matches.hasNext()) {
        const auto match = matches.next();
        rendered += styleTemplate.mid(position, match.capturedStart() - position);
        const QString value = tokenValue(tokens, scheme, match.captured(1), density);
        if (value.isEmpty()) {
            rendered += match.captured(0);
            if (missing && !missing->contains(match.captured(1)))
                missing->append(match.captured(1));
        } else {
            rendered += value;
        }
        position = match.capturedEnd();
    }
    rendered += styleTemplate.mid(position);
    return rendered;
}

QString activeDensity(const QString& fallback) {
    const auto known = [](const QString& name) {
        return name == QStringLiteral("compact") || name == QStringLiteral("regular")
               || name == QStringLiteral("comfortable");
    };
    const QString requested = qEnvironmentVariable("VORTIA_DENSITY");
    if (known(requested))
        return requested;
    return known(fallback) ? fallback : QStringLiteral("regular");
}

} // namespace ndl

namespace {

QByteArray readResource(const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray{};
}

// Qt style sheets do not pad the text of a spin box: its inner line edit
// ignores both the spin box's padding and its own. Text margins do work, so
// every spin box gets the field padding when it is polished.
class SpinBoxTextPadding final : public QObject {
public:
    using QObject::QObject;
    int padding = 0;
    bool eventFilter(QObject* watched, QEvent* event) override {
        if (event->type() == QEvent::Polish) {
            if (auto* spin = qobject_cast<QAbstractSpinBox*>(watched))
                if (auto* edit = spin->findChild<QLineEdit*>())
                    edit->setTextMargins(padding, 0, 0, 0);
        }
        return false;
    }
};

// A style sheet draws `image: url(...)` as it is: a black icon stays black on
// a dark surface. Every icon the sheet names is therefore copied to the cache
// directory in the ink of the active scheme, and the rule points at the copy.
QString tintStyleSheetIcons(QString sheet, const QString& scheme, const QColor& ink, const QColor& onAccent) {
    const QString directory = QStandardPaths::writableLocation(QStandardPaths::CacheLocation)
                              + QStringLiteral("/ndl-icons/") + scheme;
    if (!QDir().mkpath(directory))
        return sheet;
    static const QRegularExpression reference(QStringLiteral(R"(url\((:/vortia/icons/([a-z-]+)\.svg)(\?on-accent)?\))"));
    QString result;
    qsizetype position = 0;
    auto matches = reference.globalMatch(sheet);
    while (matches.hasNext()) {
        const auto match = matches.next();
        result += sheet.mid(position, match.capturedStart() - position);
        position = match.capturedEnd();
        const bool accent = !match.captured(3).isEmpty();
        QByteArray svg = readResource(match.captured(1));
        svg.replace("#000", (accent ? onAccent : ink).name().toUtf8());
        const QString target = directory + QLatin1Char('/') + match.captured(2)
                               + (accent ? QStringLiteral("-on-accent.svg") : QStringLiteral(".svg"));
        QFile file(target);
        if (svg.isEmpty() || !file.open(QIODevice::WriteOnly | QIODevice::Truncate) || file.write(svg) != svg.size()) {
            result += QStringLiteral("url(") + match.captured(1) + QLatin1Char(')');
            continue;
        }
        file.close();
        result += QStringLiteral("url(") + target + QLatin1Char(')');
    }
    return result + sheet.mid(position);
}

} // namespace

namespace {

/// Sora (titles) and Inter (everything else), the typefaces NDL names. They ship in the resources
/// (OFL-1.1, licences beside them) so Vortia looks the same where neither is installed.
void useBundledFonts(QApplication& app) {
    static const bool registered = [] {
        bool ok = true;
        for (const char* file : {":/fonts/sora/Sora[wght].ttf", ":/fonts/inter/Inter[opsz,wght].ttf"})
            ok = QFontDatabase::addApplicationFont(QString::fromLatin1(file)) >= 0 && ok;
        return ok;
    }();
    if (registered) {
        QFont font = app.font();
        font.setFamilies({QStringLiteral("Inter")});
        app.setFont(font);
    }
}

} // namespace

void ThemeManager::apply(QApplication& app, Theme theme, const QString& density) {
    app.setStyle(QStyleFactory::create(QStringLiteral("Fusion")));
    useBundledFonts(app);
    const QString scheme = toName(theme);
    const QJsonObject tokens = ndl::bundledTokens();
    const auto color = [&](const char* name) {
        return QColor(ndl::tokenValue(tokens, scheme, QStringLiteral("color.") + QLatin1String(name)));
    };

    QPalette p;
    p.setColor(QPalette::Window, color("surfaceWindow"));
    p.setColor(QPalette::Base, color("surfacePanel"));
    p.setColor(QPalette::AlternateBase, color("surfaceSunken"));
    p.setColor(QPalette::Button, color("controlBg"));
    for (auto role : {QPalette::WindowText, QPalette::Text, QPalette::ButtonText}) {
        p.setColor(role, color("textPrimary"));
        p.setColor(QPalette::Disabled, role, color("textDisabled"));
    }
    p.setColor(QPalette::ToolTipBase, color("surfaceInverse"));
    p.setColor(QPalette::ToolTipText, color("textInverse"));
    p.setColor(QPalette::PlaceholderText, color("textTertiary"));
    p.setColor(QPalette::Light, color("surfaceRaised"));
    p.setColor(QPalette::Midlight, color("borderSubtle"));
    p.setColor(QPalette::Mid, color("borderDefault"));
    p.setColor(QPalette::Dark, color("borderStrong"));
    p.setColor(QPalette::Highlight, color("accentDefault"));
    p.setColor(QPalette::HighlightedText, color("textOnAccent"));
    p.setColor(QPalette::Link, color("textLink"));
    p.setColor(QPalette::BrightText, color("statusDangerFg"));
    app.setPalette(p);

    static SpinBoxTextPadding* spinPadding = nullptr;
    if (!spinPadding) {
        spinPadding = new SpinBoxTextPadding(&app);
        app.installEventFilter(spinPadding);
    }
    spinPadding->padding = ndl::tokenValue(tokens, scheme, QStringLiteral("space.sm")).toInt();
    QString sheet = ndl::renderStyleSheet(tokens, scheme, ndl::bundledStyleSheetTemplate(), nullptr,
                                          ndl::activeDensity(density));
    sheet = tintStyleSheetIcons(sheet, scheme, color("textPrimary"), color("textOnAccent"));
    app.setStyleSheet(sheet);
}

ThemeManager::Theme ThemeManager::fromName(const QString& name) {
    return name == QStringLiteral("dark") ? Theme::Dark : Theme::Light;
}

ThemeManager::Theme ThemeManager::resolve(const QString& savedName) {
    const QString override = qEnvironmentVariable("VORTIA_THEME");
    if (override == QStringLiteral("light") || override == QStringLiteral("dark"))
        return fromName(override);
    return fromName(savedName);
}

QString ThemeManager::toName(Theme theme) {
    return theme == Theme::Light ? QStringLiteral("light") : QStringLiteral("dark");
}

} // namespace vortia
