#include "vortia/ui/ThemeManager.hpp"

#include <QApplication>
#include <QPalette>
#include <QStyleFactory>

namespace vortia {

void ThemeManager::apply(QApplication& app, Theme theme) {
    app.setStyle(QStyleFactory::create(QStringLiteral("Fusion")));
    QPalette p;

    if (theme == Theme::Dark) {
        p.setColor(QPalette::Window, QColor(36, 39, 46));
        p.setColor(QPalette::WindowText, QColor(222, 226, 232));
        p.setColor(QPalette::Base, QColor(27, 29, 35));
        p.setColor(QPalette::AlternateBase, QColor(36, 39, 46));
        p.setColor(QPalette::ToolTipBase, QColor(48, 52, 60));
        p.setColor(QPalette::ToolTipText, QColor(222, 226, 232));
        p.setColor(QPalette::Text, QColor(222, 226, 232));
        p.setColor(QPalette::Button, QColor(44, 48, 56));
        p.setColor(QPalette::ButtonText, QColor(222, 226, 232));
        p.setColor(QPalette::BrightText, QColor(255, 110, 110));
        p.setColor(QPalette::Link, QColor(96, 165, 220));
        p.setColor(QPalette::Highlight, QColor(38, 110, 168));
        p.setColor(QPalette::HighlightedText, QColor(245, 247, 250));
        p.setColor(QPalette::PlaceholderText, QColor(128, 134, 144));
        p.setColor(QPalette::Disabled, QPalette::Text, QColor(110, 116, 126));
        p.setColor(QPalette::Disabled, QPalette::ButtonText, QColor(110, 116, 126));
        p.setColor(QPalette::Disabled, QPalette::WindowText, QColor(110, 116, 126));
    } else {
        p.setColor(QPalette::Window, QColor(242, 244, 247));
        p.setColor(QPalette::WindowText, QColor(31, 35, 42));
        p.setColor(QPalette::Base, QColor(255, 255, 255));
        p.setColor(QPalette::AlternateBase, QColor(242, 244, 247));
        p.setColor(QPalette::ToolTipBase, QColor(255, 255, 240));
        p.setColor(QPalette::ToolTipText, QColor(31, 35, 42));
        p.setColor(QPalette::Text, QColor(31, 35, 42));
        p.setColor(QPalette::Button, QColor(232, 235, 240));
        p.setColor(QPalette::ButtonText, QColor(31, 35, 42));
        p.setColor(QPalette::BrightText, QColor(200, 40, 40));
        p.setColor(QPalette::Link, QColor(20, 100, 170));
        p.setColor(QPalette::Highlight, QColor(48, 125, 190));
        p.setColor(QPalette::HighlightedText, QColor(255, 255, 255));
        p.setColor(QPalette::PlaceholderText, QColor(140, 146, 154));
        p.setColor(QPalette::Disabled, QPalette::Text, QColor(160, 165, 172));
        p.setColor(QPalette::Disabled, QPalette::ButtonText, QColor(160, 165, 172));
        p.setColor(QPalette::Disabled, QPalette::WindowText, QColor(160, 165, 172));
    }
    app.setPalette(p);
}

ThemeManager::Theme ThemeManager::fromName(const QString& name) {
    return name == QStringLiteral("light") ? Theme::Light : Theme::Dark;
}

QString ThemeManager::toName(Theme theme) {
    return theme == Theme::Light ? QStringLiteral("light") : QStringLiteral("dark");
}

} // namespace vortia
