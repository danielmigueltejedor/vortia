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
    // Match the CFD workspace across Qt themes without modifying the native menu bar.
    const QString darkSheet = QStringLiteral(R"QSS(
QWidget { font-size: 12px; }
QMainWindow { background: #20242b; }
QDockWidget { color: #dfe7f0; font-weight: 600; }
QDockWidget::title { background: #252c36; text-align: left; padding: 8px 12px; border-bottom: 1px solid #343e4c; }
QToolBar { background: #252c36; border: none; border-bottom: 1px solid #343e4c; spacing: 7px; padding: 7px 10px; }
QToolBar::separator { background: #414b59; width: 1px; margin: 4px 7px; }
QToolButton { border: 1px solid transparent; border-radius: 6px; padding: 5px 8px; background: transparent; }
QToolButton:hover { background: #364456; border-color: #46566a; }
QToolButton:pressed { background: #274f75; }
QTreeWidget { background: #1b2028; border: none; outline: none; padding: 10px 4px; }
QTreeWidget::item { height: 32px; padding: 0 8px; border-radius: 5px; }
QTreeWidget::item:hover { background: #303b48; }
QTreeWidget::item:selected { background: #244c71; color: #f4f8fc; }
QPlainTextEdit { background: #151a21; border: none; color: #c5d3df; font-family: "SF Mono", Menlo, monospace; font-size: 11px; padding: 9px; }
QLineEdit, QSpinBox, QDoubleSpinBox, QComboBox { background: #2b333e; border: 1px solid #445161; border-radius: 6px; min-height: 25px; padding: 3px 8px; selection-background-color: #28669b; }
QLineEdit:focus, QSpinBox:focus, QDoubleSpinBox:focus, QComboBox:focus { border: 1px solid #56a5e8; }
QComboBox::drop-down { border: none; width: 24px; }
QPushButton { border: 1px solid #495666; border-radius: 6px; padding: 7px 14px; background: #354151; }
QPushButton:hover { background: #43536a; }
QPushButton:pressed { background: #26466a; }
QPushButton:disabled { color: #84909d; background: #29313b; }
QStatusBar { background: #212833; border-top: 1px solid #35404e; color: #b8c8d9; }
QStatusBar QLabel { padding: 3px 8px; }
QSplitter::handle { background: #343d4b; }
QToolTip { background: #303947; color: #f0f4fa; border: 1px solid #536477; padding: 5px; }
)QSS");
    const QString lightSheet = QStringLiteral(R"QSS(
QWidget { font-size: 12px; }
QMainWindow { background: #f2f5f9; }
QDockWidget { color: #293648; font-weight: 600; }
QDockWidget::title { background: #e9eef5; text-align: left; padding: 8px 12px; border-bottom: 1px solid #d5dee9; }
QToolBar { background: #e9eef5; border: none; border-bottom: 1px solid #d5dee9; spacing: 7px; padding: 7px 10px; }
QToolBar::separator { background: #ccd6e2; width: 1px; margin: 4px 7px; }
QToolButton { border: 1px solid transparent; border-radius: 6px; padding: 5px 8px; background: transparent; }
QToolButton:hover { background: #d9e5f3; border-color: #becbdb; }
QToolButton:pressed { background: #bed8f0; }
QTreeWidget { background: #ffffff; border: none; outline: none; padding: 10px 4px; }
QTreeWidget::item { height: 32px; padding: 0 8px; border-radius: 5px; }
QTreeWidget::item:hover { background: #e7edf6; }
QTreeWidget::item:selected { background: #b8daf6; color: #122e48; }
QPlainTextEdit { background: #f7f9fc; border: none; color: #2b3b4f; font-family: "SF Mono", Menlo, monospace; font-size: 11px; padding: 9px; }
QLineEdit, QSpinBox, QDoubleSpinBox, QComboBox { background: #ffffff; border: 1px solid #c7d3e2; border-radius: 6px; min-height: 25px; padding: 3px 8px; selection-background-color: #8dbfeb; }
QLineEdit:focus, QSpinBox:focus, QDoubleSpinBox:focus, QComboBox:focus { border: 1px solid #318bd0; }
QComboBox::drop-down { border: none; width: 24px; }
QPushButton { border: 1px solid #c0cedd; border-radius: 6px; padding: 7px 14px; background: #e7edf5; }
QPushButton:hover { background: #d8e7f7; }
QPushButton:pressed { background: #c7def3; }
QPushButton:disabled { color: #929fad; background: #e9edf2; }
QStatusBar { background: #e8eef5; border-top: 1px solid #d2dbe6; color: #354e68; }
QStatusBar QLabel { padding: 3px 8px; }
QSplitter::handle { background: #d5dee9; }
QToolTip { background: #ffffff; color: #25374a; border: 1px solid #cbd7e3; padding: 5px; }
)QSS");
    app.setStyleSheet(theme == Theme::Dark ? darkSheet : lightSheet);
}

ThemeManager::Theme ThemeManager::fromName(const QString& name) {
    return name == QStringLiteral("light") ? Theme::Light : Theme::Dark;
}

QString ThemeManager::toName(Theme theme) {
    return theme == Theme::Light ? QStringLiteral("light") : QStringLiteral("dark");
}

} // namespace vortia
