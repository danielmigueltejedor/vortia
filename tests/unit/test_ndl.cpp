#include <gtest/gtest.h>

#include "vortia/ui/ThemeManager.hpp"
#include "vortia/ui/WindowChrome.hpp"

#include <QApplication>
#include <QColor>
#include <QDockWidget>
#include <QLabel>
#include <QMainWindow>
#include <QHelpEvent>
#include <QMenu>
#include <QMouseEvent>
#include <QToolButton>
#include <QToolTip>
#include <QJsonDocument>
#include <QRegularExpression>

using namespace vortia;
using namespace vortia::ndl;

namespace {
const WindowButton Min = WindowButton::Minimize, Max = WindowButton::Maximize, Close = WindowButton::Close;
}

TEST(NdlWindowButtons, GnomeDefaultsAndCommonLayouts) {
    EXPECT_EQ(parseGnomeButtonLayout("appmenu:minimize,maximize,close"), (WindowButtonLayout{{}, {Min, Max, Close}}));
    // Stock GNOME: only a close button, on the right.
    EXPECT_EQ(parseGnomeButtonLayout("'appmenu:close'\n"), (WindowButtonLayout{{}, {Close}}));
    // macOS-like layout chosen by the user.
    EXPECT_EQ(parseGnomeButtonLayout("close,minimize,maximize:"), (WindowButtonLayout{{Close, Min, Max}, {}}));
    EXPECT_EQ(parseGnomeButtonLayout("close:minimize,maximize"), (WindowButtonLayout{{Close}, {Min, Max}}));
    EXPECT_EQ(parseGnomeButtonLayout("icon,menu:spacer,Minimize , CLOSE"), (WindowButtonLayout{{}, {Min, Close}}));
}

TEST(NdlWindowButtons, AWindowAlwaysKeepsAWayToCloseIt) {
    for (const char* useless : {"", ":", "appmenu:", "icon,menu:spacer", "''", "garbage"})
        EXPECT_EQ(parseGnomeButtonLayout(useless), defaultButtonLayout()) << useless;
    EXPECT_EQ(parseKdeButtonLayout("MS", "H"), defaultButtonLayout());
    EXPECT_EQ(defaultButtonLayout(), (WindowButtonLayout{{}, {Min, Max, Close}}));
}

TEST(NdlWindowButtons, AButtonListedTwiceAppearsOnce) {
    EXPECT_EQ(parseGnomeButtonLayout("close,close:close,minimize"), (WindowButtonLayout{{Close}, {Min}}));
    EXPECT_EQ(parseKdeButtonLayout("XX", "IAX"), (WindowButtonLayout{{Close}, {Min, Max}}));
}

TEST(NdlWindowButtons, KdeLetters) {
    EXPECT_EQ(parseKdeButtonLayout("MS", "HIAX"), (WindowButtonLayout{{}, {Min, Max, Close}}));
    EXPECT_EQ(parseKdeButtonLayout("XIA", ""), (WindowButtonLayout{{Close, Min, Max}, {}}));
}

TEST(NdlWindowButtons, EnvironmentOverrideWins) {
    qputenv("VORTIA_WINDOW_BUTTONS", "close:minimize");
    EXPECT_EQ(systemButtonLayout(), (WindowButtonLayout{{Close}, {Min}}));
    qunsetenv("VORTIA_WINDOW_BUTTONS");
}

TEST(NdlTokens, BundledTokensDefineEveryModeWithTheSameColours) {
    const QJsonObject tokens = bundledTokens();
    ASSERT_FALSE(tokens.isEmpty());
    const QJsonObject colours = tokens["color"].toObject();
    const QJsonObject light = colours["light"].toObject();
    ASSERT_FALSE(light.isEmpty());
    for (const QString& scheme : {QStringLiteral("light"), QStringLiteral("dark"), QStringLiteral("light-hc"),
                                  QStringLiteral("dark-hc")}) {
        EXPECT_EQ(colours[scheme].toObject().keys(), light.keys()) << scheme.toStdString();
        for (const QString& name : light.keys()) {
            const QString value = tokenValue(tokens, scheme, "color." + name);
            // Opaque colours are #RRGGBB; translucent ones are rgba(), which QColor does not parse.
            EXPECT_TRUE(QColor(value).isValid() || value.startsWith("rgba("))
                << scheme.toStdString() << " " << name.toStdString();
        }
    }
}

TEST(NdlTokens, DensityMetricsFollowTheChosenDensity) {
    const QJsonObject tokens = bundledTokens();
    EXPECT_EQ(tokenValue(tokens, "light", "density.controlHeight"), "32");
    EXPECT_EQ(tokenValue(tokens, "light", "density.controlHeight", "compact"), "24");
    EXPECT_EQ(tokenValue(tokens, "dark", "density.controlHeight", "comfortable"), "40");
    EXPECT_TRUE(tokenValue(tokens, "light", "density.controlHeight", "huge").isEmpty());
    qputenv("VORTIA_DENSITY", "compact");
    EXPECT_EQ(activeDensity(), "compact");
    qputenv("VORTIA_DENSITY", "huge");
    EXPECT_EQ(activeDensity(), "regular");
    EXPECT_EQ(activeDensity("comfortable"), "comfortable");
    qunsetenv("VORTIA_DENSITY");
    EXPECT_EQ(activeDensity("compact"), "compact");
    EXPECT_EQ(activeDensity("huge"), "regular");
}

TEST(NdlTokens, LookupHandlesNumbersAndUnknownReferences) {
    const QJsonObject tokens = QJsonDocument::fromJson(
        R"({"color":{"light":{"ink":"#111111"},"dark":{"ink":"#EEEEEE"}},"radius":{"panel":12}})").object();
    EXPECT_EQ(tokenValue(tokens, "light", "color.ink"), "#111111");
    EXPECT_EQ(tokenValue(tokens, "dark", "color.ink"), "#EEEEEE");
    EXPECT_EQ(tokenValue(tokens, "dark", "radius.panel"), "12");
    EXPECT_TRUE(tokenValue(tokens, "light", "color.missing").isEmpty());
    EXPECT_TRUE(tokenValue(tokens, "light", "nonsense").isEmpty());
    EXPECT_TRUE(tokenValue(tokens, "sepia", "color.ink").isEmpty());
}

TEST(NdlStyleSheet, RendersPlaceholdersAndReportsMissingOnes) {
    const QJsonObject tokens = QJsonDocument::fromJson(
        R"({"color":{"light":{"ink":"#111111"}},"radius":{"panel":12}})").object();
    QStringList missing;
    const QString css = renderStyleSheet(
        tokens, "light", "a{color:{{color.ink}};border-radius:{{radius.panel}}px;b:{{color.nope}};c:{{color.nope}}}",
        &missing);
    EXPECT_EQ(css, "a{color:#111111;border-radius:12px;b:{{color.nope}};c:{{color.nope}}}");
    EXPECT_EQ(missing, QStringList{"color.nope"});
}

// Every placeholder of the shipped template must resolve in every mode and
// density: an unresolved one would silently drop a whole style rule.
TEST(NdlStyleSheet, BundledTemplateResolvesCompletely) {
    const QString styleTemplate = bundledStyleSheetTemplate();
    ASSERT_GT(styleTemplate.size(), 1000);
    EXPECT_TRUE(styleTemplate.contains("vortia-workflow-navigation"));
    for (const QString& scheme : {QStringLiteral("light"), QStringLiteral("dark"), QStringLiteral("light-hc"),
                                  QStringLiteral("dark-hc")}) {
        for (const QString& density : {QStringLiteral("compact"), QStringLiteral("regular"),
                                       QStringLiteral("comfortable")}) {
            QStringList missing;
            const QString css = renderStyleSheet(bundledTokens(), scheme, styleTemplate, &missing, density);
            EXPECT_TRUE(missing.isEmpty()) << missing.join(", ").toStdString();
            EXPECT_FALSE(css.contains("{{"));
        }
    }
}

TEST(NdlTheme, EnvironmentOverridesTheSavedThemeForOneRun) {
    qunsetenv("VORTIA_THEME");
    EXPECT_EQ(ThemeManager::resolve("dark"), ThemeManager::Theme::Dark);
    EXPECT_EQ(ThemeManager::resolve("light"), ThemeManager::Theme::Light);
    qputenv("VORTIA_THEME", "light");
    EXPECT_EQ(ThemeManager::resolve("dark"), ThemeManager::Theme::Light);
    qputenv("VORTIA_THEME", "neon");
    EXPECT_EQ(ThemeManager::resolve("dark"), ThemeManager::Theme::Dark);
    qunsetenv("VORTIA_THEME");
}

namespace {
/// The app-wide filter for popups and tooltips, without dialog frames, leaving
/// VORTIA_NATIVE_TITLEBAR as the test process had it.
void installChromeWithoutFrames() {
    const bool had = qEnvironmentVariableIsSet("VORTIA_NATIVE_TITLEBAR");
    const QByteArray previous = qgetenv("VORTIA_NATIVE_TITLEBAR");
    qputenv("VORTIA_NATIVE_TITLEBAR", "1");
    vortia::ndl::installDialogChrome(qApp);
    if (had)
        qputenv("VORTIA_NATIVE_TITLEBAR", previous);
    else
        qunsetenv("VORTIA_NATIVE_TITLEBAR");
}

void sendMouse(QWidget* target, QEvent::Type type, QPointF at, Qt::MouseButtons buttons) {
    QMouseEvent event(type, at, target->mapToGlobal(at), Qt::LeftButton, buttons, Qt::NoModifier);
    QApplication::sendEvent(target, &event);
}
} // namespace

// Docked, the title bar does not move the panel (Qt's drag to undock hung under Wayland); only
// the window-mode button floats it.
TEST(NdlDock, OnlyTheWindowModeButtonFloatsAPanel) {
    QMainWindow window;
    auto* dock = new QDockWidget(QStringLiteral("Inspector"), &window);
    dock->setWidget(new QLabel(QStringLiteral("content")));
    vortia::ndl::installDockTitleBar(dock);
    window.addDockWidget(Qt::RightDockWidgetArea, dock);
    window.resize(800, 600);
    window.show();
    auto* bar = dock->titleBarWidget();
    ASSERT_NE(bar, nullptr);
    const QPointF grip(4, bar->height() / 2.0);
    sendMouse(bar, QEvent::MouseButtonPress, grip, Qt::LeftButton);
    sendMouse(bar, QEvent::MouseMove, grip + QPointF(-200, 120), Qt::LeftButton);
    sendMouse(bar, QEvent::MouseButtonRelease, grip + QPointF(-200, 120), Qt::NoButton);
    EXPECT_FALSE(dock->isFloating());
    const auto buttons = bar->findChildren<QToolButton*>();
    ASSERT_EQ(buttons.size(), 2);
    buttons.front()->click();
    EXPECT_TRUE(dock->isFloating());
}

// Menus are islands with NDL's radius: their window must be see-through for the corners to show,
// which has to be decided before the window exists.
TEST(NdlPopups, MenusGetASeeThroughWindow) {
    static bool installed = false;
    if (!installed) {
        // Popups only: other tests' dialogs keep their plain frame.
        installChromeWithoutFrames();
        installed = true;
    }
    QMenu menu;
    menu.addAction(QStringLiteral("One"));
    menu.ensurePolished();
    EXPECT_TRUE(menu.testAttribute(Qt::WA_TranslucentBackground));
}

// A button that shows its name does not repeat it in a tooltip; an icon-only one keeps it.
TEST(NdlTooltips, OnlyIconButtonsShowTheirName) {
    installChromeWithoutFrames();
    QWidget host;
    auto* labelled = new QToolButton(&host);
    labelled->setText(QStringLiteral("Malla"));
    labelled->setToolTip(QStringLiteral("Malla"));
    labelled->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    auto* iconOnly = new QToolButton(&host);
    iconOnly->setText(QStringLiteral("Cerrar"));
    iconOnly->setToolTip(QStringLiteral("Cerrar"));
    iconOnly->setToolButtonStyle(Qt::ToolButtonIconOnly);
    host.show();
    const auto shown = [](QWidget* widget) {
        QToolTip::hideText();
        QHelpEvent event(QEvent::ToolTip, QPoint(1, 1), widget->mapToGlobal(QPoint(1, 1)));
        QApplication::sendEvent(widget, &event);
        const bool visible = QToolTip::isVisible();
        QToolTip::hideText();
        return visible;
    };
    auto* explained = new QToolButton(&host);
    explained->setText(QStringLiteral("Preparar reinicio limpio"));
    explained->setToolTip(QStringLiteral("Guarda el diagnóstico y vuelve a empezar desde la última malla"));
    explained->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    auto* withShortcut = new QToolButton(&host);
    withShortcut->setText(QStringLiteral("Geometría"));
    withShortcut->setToolTip(QStringLiteral("Geometría · Alt+2"));
    withShortcut->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    EXPECT_FALSE(shown(labelled));
    EXPECT_FALSE(shown(withShortcut));
    EXPECT_TRUE(shown(explained));
    EXPECT_TRUE(shown(iconOnly));
}
