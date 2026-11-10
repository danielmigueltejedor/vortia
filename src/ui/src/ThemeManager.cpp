#include "vortia/ui/ThemeManager.hpp"

#include <QApplication>
#include <QColor>
#include <QFile>
#include <QJsonDocument>
#include <QJsonValue>
#include <QPalette>
#include <QRegularExpression>
#include <QStyleFactory>

namespace vortia {

namespace lienzo {

namespace {

QByteArray readResource(const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray{};
}

} // namespace

QJsonObject bundledTokens() {
    return QJsonDocument::fromJson(readResource(QStringLiteral(":/lienzo/tokens.json"))).object();
}

QString bundledStyleSheetTemplate() {
    return QString::fromUtf8(readResource(QStringLiteral(":/lienzo/lienzo.qss")));
}

QString tokenValue(const QJsonObject& tokens, const QString& scheme, const QString& reference) {
    const QStringList parts = reference.split(QLatin1Char('.'));
    if (parts.size() != 2)
        return {};
    QJsonObject group = tokens.value(parts[0]).toObject();
    if (parts[0] == QStringLiteral("color"))
        group = group.value(scheme).toObject();
    const QJsonValue value = group.value(parts[1]);
    if (value.isString())
        return value.toString();
    if (value.isDouble())
        return QString::number(value.toDouble());
    return {};
}

QString renderStyleSheet(const QJsonObject& tokens, const QString& scheme,
                         const QString& styleTemplate, QStringList* missing) {
    static const QRegularExpression placeholder(QStringLiteral(R"(\{\{([A-Za-z]+\.[A-Za-z]+)\}\})"));
    QString rendered;
    rendered.reserve(styleTemplate.size());
    qsizetype position = 0;
    auto matches = placeholder.globalMatch(styleTemplate);
    while (matches.hasNext()) {
        const auto match = matches.next();
        rendered += styleTemplate.mid(position, match.capturedStart() - position);
        const QString value = tokenValue(tokens, scheme, match.captured(1));
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

} // namespace lienzo

void ThemeManager::apply(QApplication& app, Theme theme) {
    app.setStyle(QStyleFactory::create(QStringLiteral("Fusion")));
    const QString scheme = toName(theme);
    const QJsonObject tokens = lienzo::bundledTokens();
    const auto color = [&](const char* name) {
        return QColor(lienzo::tokenValue(tokens, scheme, QStringLiteral("color.") + QLatin1String(name)));
    };

    QPalette p;
    p.setColor(QPalette::Window, color("canvas"));
    p.setColor(QPalette::Base, color("surface"));
    p.setColor(QPalette::AlternateBase, color("surfaceSunken"));
    p.setColor(QPalette::Button, color("surface"));
    for (auto role : {QPalette::WindowText, QPalette::Text, QPalette::ButtonText}) {
        p.setColor(role, color("ink"));
        p.setColor(QPalette::Disabled, role, color("inkDisabled"));
    }
    p.setColor(QPalette::ToolTipBase, color("ink"));
    p.setColor(QPalette::ToolTipText, color("surface"));
    p.setColor(QPalette::PlaceholderText, color("inkMuted"));
    p.setColor(QPalette::Mid, color("border"));
    p.setColor(QPalette::Midlight, color("hover"));
    p.setColor(QPalette::Highlight, color("accent"));
    p.setColor(QPalette::HighlightedText, color("onAccent"));
    p.setColor(QPalette::Link, color("accent"));
    p.setColor(QPalette::BrightText, color("danger"));
    app.setPalette(p);
    app.setStyleSheet(lienzo::renderStyleSheet(tokens, scheme, lienzo::bundledStyleSheetTemplate()));
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
