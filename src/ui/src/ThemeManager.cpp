#include "vortia/ui/ThemeManager.hpp"

#include <QApplication>
#include <QPalette>
#include <QStyleFactory>

namespace vortia {

void ThemeManager::apply(QApplication& app, Theme theme) {
    app.setStyle(QStyleFactory::create(QStringLiteral("Fusion")));
    const bool dark=theme==Theme::Dark;
    const QString surface=dark?"#202630":"#f7f8fa",base=dark?"#1a2029":"#ffffff";
    const QString ink=dark?"#d7dfe8":"#25313e",muted=dark?"#91a0b0":"#607081";
    const QString border=dark?"#303946":"#d9dfe6",hover=dark?"#2a3442":"#e8edf2";
    const QString accent=dark?"#43abb8":"#177886",selected=dark?"#233b43":"#e3f0f2";
    const QString highlight=dark?"#276f7b":"#1d7380",disabled=dark?"#687585":"#8793a0";
    QPalette p;
    p.setColor(QPalette::Window,QColor(surface));p.setColor(QPalette::Base,QColor(base));
    p.setColor(QPalette::AlternateBase,QColor(surface));p.setColor(QPalette::Button,QColor(surface));
    for(auto role:{QPalette::WindowText,QPalette::Text,QPalette::ButtonText,QPalette::ToolTipText}) {
        p.setColor(role,QColor(ink));p.setColor(QPalette::Disabled,role,QColor(disabled));
    }
    p.setColor(QPalette::ToolTipBase,QColor(base));p.setColor(QPalette::PlaceholderText,QColor(muted));
    p.setColor(QPalette::Mid,QColor(border));p.setColor(QPalette::Midlight,QColor(hover));
    p.setColor(QPalette::Highlight,QColor(highlight));p.setColor(QPalette::HighlightedText,QColor("#ffffff"));
    p.setColor(QPalette::Link,QColor(accent));p.setColor(QPalette::BrightText,QColor(dark?"#ed8189":"#b43643"));
    app.setPalette(p);
    // Precision: neutral surfaces and ink, restrained accent for selection.
    app.setStyleSheet(QStringLiteral(R"QSS(
QWidget {font-size:13px;}
QMainWindow {background:%1;}
QToolBar {background:%1;border:0;border-bottom:1px solid %5;spacing:5px;padding:3px 5px;}
QToolBar#vortia-main-toolbar {padding:0;}
QToolBar::separator {background:%5;width:1px;margin:4px 8px;}
QToolBar#vortia-workflow-toolbar {border:0;border-right:1px solid %5;padding:0;background:%1;}
QWidget#vortia-workflow-navigation {background:%1;}
QToolButton {background:transparent;border:1px solid transparent;border-radius:5px;padding:5px 7px;color:%3;}
QToolButton:hover {background:%6;border-color:%5;}
QToolButton:pressed {background:%8;}
QToolButton:checked {background:%8;color:%7;}
QToolButton:disabled {color:%10;}
QWidget#vortia-workflow-navigation QToolButton {text-align:left;padding:9px 8px;}
QWidget#vortia-workflow-navigation QToolButton:checked {border:1px solid transparent;background:%8;color:%7;}
QToolButton#vortia-ribbon-button {padding:5px 8px;}
QLabel#vortia-ribbon-group {color:%4;font-size:11px;}
QFrame#vortia-ribbon-separator {background:%5;max-width:1px;}
QTabWidget#vortia-ribbon::pane {border:0;background:%2;}
QTabWidget::pane {border:0;background:%2;}
QTabBar::tab {background:%1;color:%4;border:0;border-bottom:2px solid transparent;padding:8px 13px;}
QTabBar::tab:selected {background:%2;color:%3;border-bottom:2px solid %7;}
QTabBar::tab:hover:!selected {background:%6;}
QTabBar#vortia-project-tabs {background:%1;border-bottom:1px solid %5;}
QTabBar#vortia-project-tabs::tab {min-width:110px;padding:9px 15px;border-right:1px solid %5;}
QTabBar#vortia-project-tabs::tab:selected {background:%2;border-bottom:2px solid %7;}
QDockWidget {color:%4;font-weight:500;}
QDockWidget::title {background:%1;text-align:left;padding:8px 10px;border-bottom:1px solid %5;}
QTreeWidget {background:%2;border:0;padding:6px 3px;outline:0;}
QTreeWidget::item {min-height:30px;padding:3px 8px;border-radius:4px;}
QTreeWidget::item:hover {background:%6;}
QTreeWidget::item:selected {background:%8;color:%3;}
QLineEdit,QSpinBox,QDoubleSpinBox,QComboBox {background:%2;color:%3;border:1px solid %5;border-radius:5px;min-height:24px;padding:4px 8px;selection-background-color:%9;selection-color:white;}
QLineEdit:focus,QSpinBox:focus,QDoubleSpinBox:focus,QComboBox:focus {border-color:%7;}
QComboBox {padding-right:30px;combobox-popup:0;}
QComboBox::drop-down {subcontrol-origin:padding;subcontrol-position:top right;width:26px;border:0;}
QComboBox::down-arrow {image:url(:/vortia/icons/chevron-down.svg);width:12px;height:12px;}
QComboBox::drop-down:hover {background:%6;}
QComboBox QAbstractItemView {background:%2;color:%3;border:1px solid %5;border-radius:5px;padding:4px;outline:0;selection-background-color:%9;selection-color:white;}
QComboBox QAbstractItemView::item {min-height:28px;padding:5px 10px;}
QComboBox QAbstractItemView::item:selected {background:%9;color:white;}
QComboBox:disabled,QAbstractSpinBox:disabled,QLineEdit:disabled {color:%10;background:%1;}
QSpinBox,QDoubleSpinBox {padding-right:28px;}
QSpinBox::up-button,QDoubleSpinBox::up-button {subcontrol-origin:border;subcontrol-position:top right;width:24px;height:16px;border:0;background:transparent;}
QSpinBox::down-button,QDoubleSpinBox::down-button {subcontrol-origin:border;subcontrol-position:bottom right;width:24px;height:16px;border:0;background:transparent;}
QSpinBox::up-button:hover,QDoubleSpinBox::up-button:hover,QSpinBox::down-button:hover,QDoubleSpinBox::down-button:hover {background:%6;}
QSpinBox::up-arrow,QDoubleSpinBox::up-arrow {image:url(:/vortia/icons/chevron-up.svg);width:10px;height:10px;}
QSpinBox::down-arrow,QDoubleSpinBox::down-arrow {image:url(:/vortia/icons/chevron-down.svg);width:10px;height:10px;}
QPushButton {background:%1;color:%3;border:1px solid %5;border-radius:5px;padding:6px 12px;min-height:20px;}
QPushButton:hover {background:%6;}
QPushButton:pressed,QPushButton:checked {background:%8;color:%7;border-color:%7;}
QPushButton:disabled {color:%10;background:%1;}
QWidget#flow-navigation-sidebar QPushButton,QWidget#preferences-navigation-sidebar QPushButton {text-align:left;background:transparent;border:1px solid transparent;padding:11px 9px;min-height:25px;}
QWidget#flow-navigation-sidebar QPushButton:checked,QWidget#preferences-navigation-sidebar QPushButton:checked {background:%8;color:%7;}
QGroupBox {border:1px solid %5;border-radius:6px;margin-top:12px;padding:13px;}
QGroupBox::title {subcontrol-origin:margin;left:10px;padding:0 5px;}
QTableWidget {background:%2;alternate-background-color:%1;color:%3;border:1px solid %5;border-radius:5px;gridline-color:%5;selection-background-color:%8;selection-color:%3;}
QHeaderView::section {background:%1;color:%4;padding:7px 9px;border:0;border-bottom:1px solid %5;}
QTableCornerButton::section {background:%1;border:0;}
QListWidget {background:%2;color:%3;border:1px solid %5;border-radius:5px;padding:4px;outline:0;}
QListWidget::item {padding:7px 9px;border-radius:4px;}
QListWidget::item:selected {background:%8;color:%3;}
QPlainTextEdit {background:%2;border:0;color:%3;font-family:Menlo,monospace;font-size:11px;padding:8px;}
QScrollBar:vertical {background:transparent;width:10px;margin:2px;}
QScrollBar:horizontal {background:transparent;height:10px;margin:2px;}
QScrollBar::handle:vertical {background:%10;border-radius:4px;min-height:28px;}
QScrollBar::handle:horizontal {background:%10;border-radius:4px;min-width:28px;}
QScrollBar::handle:hover {background:%4;}
QScrollBar::add-line,QScrollBar::sub-line {border:0;background:none;width:0;height:0;}
QScrollBar::add-page,QScrollBar::sub-page {background:none;}
QMenu {background:%2;color:%3;border:1px solid %5;border-radius:6px;padding:5px;}
QMenu::item {padding:7px 29px 7px 26px;border-radius:4px;}
QMenu::item:selected {background:%9;color:white;}
QMenu::item:disabled {color:%10;}
QMenu::separator {height:1px;background:%5;margin:5px 8px;}
QMenu::right-arrow {image:url(:/vortia/icons/chevron-right.svg);width:12px;height:12px;}
QToolButton::menu-indicator {image:url(:/vortia/icons/chevron-down.svg);width:9px;height:9px;}
QToolButton::menu-button {border:0;background:transparent;width:16px;}
QTabBar::close-button {image:url(:/vortia/icons/close.svg);width:12px;height:12px;padding:3px;border-radius:4px;}
QTabBar::close-button:hover {background:%6;}
QDockWidget::close-button {image:url(:/vortia/icons/close.svg);border:0;background:transparent;width:17px;height:17px;}
QDockWidget::float-button {image:url(:/vortia/icons/undock.svg);border:0;background:transparent;width:17px;height:17px;}
QDockWidget::close-button:hover,QDockWidget::float-button:hover {background:%6;}
QStatusBar {background:%1;border-top:1px solid %5;color:%4;}
QStatusBar QLabel {padding:3px 6px;}
QSplitter::handle {background:%5;}
QToolTip {background:%2;color:%3;border:1px solid %5;padding:6px;}
QSlider::groove:horizontal {height:4px;background:%5;border-radius:2px;}
QSlider::handle:horizontal {width:14px;margin:-5px 0;background:%7;border:0;border-radius:7px;}
)QSS").arg(surface,base,ink,muted,border,hover,accent,selected,highlight,disabled));
}

ThemeManager::Theme ThemeManager::fromName(const QString& name) {
    return name==QStringLiteral("light")?Theme::Light:Theme::Dark;
}
QString ThemeManager::toName(Theme theme) {
    return theme==Theme::Light?QStringLiteral("light"):QStringLiteral("dark");
}
} // namespace vortia
