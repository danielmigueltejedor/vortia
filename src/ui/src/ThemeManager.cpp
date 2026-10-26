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

QTabWidget#vortia-ribbon::pane { border: none; background: #252c36; }
QTabWidget#vortia-ribbon QTabBar::tab { min-width: 78px; padding: 8px 14px; color: #a7b5c6; border: none; background: #252c36; }
QTabWidget#vortia-ribbon QTabBar::tab:selected { color: #f0f7ff; border-bottom: 2px solid #54aaf2; background: #303b49; }
QTabWidget#vortia-ribbon QTabBar::tab:hover { background: #364656; }
QToolButton#vortia-ribbon-button { border: 1px solid transparent; border-radius: 7px; color: #e7edf7; background: transparent; padding: 4px; }
QToolButton#vortia-ribbon-button:hover { background: #35485e; border-color: #52667b; }
QToolButton#vortia-ribbon-button:pressed { background: #255479; }
QToolButton#vortia-ribbon-button:disabled { color: #788798; }
QLabel#vortia-ribbon-group { color: #8c9cac; font-size: 10px; font-weight: 600; }
QFrame#vortia-ribbon-separator { color: #425063; background: #425063; max-width: 1px; }

QTabBar#vortia-project-tabs { background: #1d242e; border-bottom: 1px solid #364659; }
QTabBar#vortia-project-tabs::tab { background: #26313d; color: #b7c8d9; min-width: 118px; padding: 10px 16px; margin: 3px 1px 0; border: 1px solid #364657; border-bottom: 0; border-top-left-radius: 6px; border-top-right-radius: 6px; }
QTabBar#vortia-project-tabs::tab:selected { background: #354659; color: #ffffff; border-top: 2px solid #54aaf2; }
QTabBar#vortia-project-tabs::tab:hover:!selected { background: #334357; }
QTabWidget#vortia-editor-tabs::pane { border: none; background: #1c232b; }
QTabWidget#vortia-editor-tabs QTabBar::tab { min-width: 98px; padding: 8px 14px; color: #a7bacb; background: #252f3b; border: none; }
QTabWidget#vortia-editor-tabs QTabBar::tab:selected { color: #fff; background: #344456; border-bottom: 2px solid #54aaf2; }
QTabWidget#vortia-editor-tabs QTabBar::tab:hover { background: #405169; }
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

QTabWidget#vortia-ribbon::pane { border: none; background: #e9eef5; }
QTabWidget#vortia-ribbon QTabBar::tab { min-width: 78px; padding: 8px 14px; color: #5d7287; border: none; background: #e9eef5; }
QTabWidget#vortia-ribbon QTabBar::tab:selected { color: #163853; border-bottom: 2px solid #2a86c7; background: #f6f9fc; }
QTabWidget#vortia-ribbon QTabBar::tab:hover { background: #d6e5f3; }
QToolButton#vortia-ribbon-button { border: 1px solid transparent; border-radius: 7px; color: #304457; background: transparent; padding: 4px; }
QToolButton#vortia-ribbon-button:hover { background: #d6e7f5; border-color: #a5c1d8; }
QToolButton#vortia-ribbon-button:pressed { background: #b8d7f1; }
QToolButton#vortia-ribbon-button:disabled { color: #97a2af; }
QLabel#vortia-ribbon-group { color: #667b8f; font-size: 10px; font-weight: 600; }
QFrame#vortia-ribbon-separator { color: #b8c9db; background: #b8c9db; max-width: 1px; }

QTabBar#vortia-project-tabs { background: #e7edf5; border-bottom: 1px solid #c7d5e3; }
QTabBar#vortia-project-tabs::tab { background: #e4ebf3; color: #4e667f; min-width: 118px; padding: 10px 16px; margin: 3px 1px 0; border: 1px solid #c8d6e4; border-bottom: 0; border-top-left-radius: 6px; border-top-right-radius: 6px; }
QTabBar#vortia-project-tabs::tab:selected { background: #fff; color: #173c5b; border-top: 2px solid #2a86c7; }
QTabBar#vortia-project-tabs::tab:hover:!selected { background: #d5e4f2; }
QTabWidget#vortia-editor-tabs::pane { border: none; background: #f6f9fc; }
QTabWidget#vortia-editor-tabs QTabBar::tab { min-width: 98px; padding: 8px 14px; color: #60778d; background: #e5edf5; border: none; }
QTabWidget#vortia-editor-tabs QTabBar::tab:selected { color: #123e5c; background: #fff; border-bottom: 2px solid #2a86c7; }
QTabWidget#vortia-editor-tabs QTabBar::tab:hover { background: #d7e7f4; }
)QSS");
    app.setStyleSheet(theme == Theme::Dark ? darkSheet : lightSheet);
    const bool dark=theme==Theme::Dark;
    const QString controls=QStringLiteral(R"QSS(
QComboBox {padding-right:32px; combobox-popup:0;}
QComboBox::drop-down {subcontrol-origin:padding;subcontrol-position:top right;width:28px;border:0;border-top-right-radius:6px;border-bottom-right-radius:6px;}
QComboBox::down-arrow {image:url(:/vortia/icons/chevron-down.svg);width:12px;height:12px;}
QComboBox::drop-down:hover {background:%3;}
QComboBox QAbstractItemView {background:%2;color:%6;border:1px solid %1;border-radius:8px;padding:5px;outline:0;selection-background-color:%5;selection-color:white;}
QComboBox QAbstractItemView::item {min-height:28px;padding:5px 10px;border-radius:5px;}
QComboBox QAbstractItemView::item:selected {background:%5;color:white;}
QComboBox:disabled, QAbstractSpinBox:disabled, QLineEdit:disabled {color:%4;background:%3;border-color:%1;}
QSpinBox, QDoubleSpinBox {padding-right:29px;}
QSpinBox::up-button, QDoubleSpinBox::up-button {subcontrol-origin:border;subcontrol-position:top right;width:25px;height:16px;background:transparent;border:0;border-top-right-radius:6px;}
QSpinBox::down-button, QDoubleSpinBox::down-button {subcontrol-origin:border;subcontrol-position:bottom right;width:25px;height:16px;background:transparent;border:0;border-bottom-right-radius:6px;}
QSpinBox::up-button:hover, QDoubleSpinBox::up-button:hover, QSpinBox::down-button:hover, QDoubleSpinBox::down-button:hover {background:%3;}
QSpinBox::up-arrow, QDoubleSpinBox::up-arrow {image:url(:/vortia/icons/chevron-up.svg);width:10px;height:10px;}
QSpinBox::down-arrow, QDoubleSpinBox::down-arrow {image:url(:/vortia/icons/chevron-down.svg);width:10px;height:10px;}
QScrollBar:vertical {background:transparent;width:11px;margin:2px;}
QScrollBar:horizontal {background:transparent;height:11px;margin:2px;}
QScrollBar::handle:vertical {background:%4;border-radius:4px;min-height:28px;}
QScrollBar::handle:horizontal {background:%4;border-radius:4px;min-width:28px;}
QScrollBar::handle:hover {background:%5;}
QScrollBar::add-line, QScrollBar::sub-line {background:none;border:0;width:0;height:0;}
QScrollBar::add-page, QScrollBar::sub-page {background:none;}
QMenu {background:%2;color:%6;border:1px solid %1;border-radius:9px;padding:6px;}
QMenu::item {padding:8px 30px 8px 26px;border-radius:5px;}
QMenu::item:selected {background:%5;color:white;}
QMenu::item:disabled {color:%4;}
QMenu::separator {height:1px;background:%1;margin:5px 8px;}
QMenu::right-arrow {image:url(:/vortia/icons/chevron-right.svg);width:12px;height:12px;}
QToolButton::menu-indicator {image:url(:/vortia/icons/chevron-down.svg);width:10px;height:10px;}
QToolButton::menu-button {border:0;background:transparent;width:18px;}
QTabBar::close-button {image:url(:/vortia/icons/close.svg);width:13px;height:13px;padding:3px;border-radius:4px;}
QTabBar::close-button:hover {background:%3;}
QDockWidget::close-button {image:url(:/vortia/icons/close.svg);border:0;background:transparent;border-radius:4px;width:18px;height:18px;}
QDockWidget::float-button {image:url(:/vortia/icons/undock.svg);border:0;background:transparent;border-radius:4px;width:18px;height:18px;}
QDockWidget::close-button:hover, QDockWidget::float-button:hover {background:%3;}
QTableWidget {background:%2;alternate-background-color:%3;color:%6;border:1px solid %1;border-radius:7px;gridline-color:%1;selection-background-color:%5;selection-color:white;}
QHeaderView::section {background:%3;color:%6;padding:7px 10px;border:0;border-bottom:1px solid %1;}
QTableCornerButton::section {background:%3;border:0;}
QListWidget {background:%2;color:%6;border:1px solid %1;border-radius:7px;padding:4px;outline:0;}
QListWidget::item {padding:7px 9px;border-radius:5px;}
QListWidget::item:selected {background:%5;color:white;}
QSlider::groove:horizontal {height:4px;background:%1;border-radius:2px;}
QSlider::handle:horizontal {width:15px;margin:-6px 0;background:%5;border:1px solid %5;border-radius:8px;}
)QSS").arg(dark?"#435262":"#cbd7e2",dark?"#222b35":"#ffffff",dark?"#354351":"#edf3f8",
            dark?"#718091":"#99adbd","#147f9f",dark?"#e5edf5":"#24384a");
    app.setStyleSheet(app.styleSheet()+controls);
}

ThemeManager::Theme ThemeManager::fromName(const QString& name) {
    return name == QStringLiteral("light") ? Theme::Light : Theme::Dark;
}

QString ThemeManager::toName(Theme theme) {
    return theme == Theme::Light ? QStringLiteral("light") : QStringLiteral("dark");
}

} // namespace vortia
