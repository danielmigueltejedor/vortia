#include "vortia/ui/HeaderMenus.hpp"

#include <gtest/gtest.h>

#include <QApplication>
#include <QCoreApplication>
#include <QDeadlineTimer>
#include <QMenu>
#include <QMenuBar>
#include <QToolButton>
#ifdef VORTIA_HAS_QTDBUS
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#endif

#include <functional>

using vortia::ndl::HeaderMenus;

namespace {

// A name no shell registers, so the test owns the answer.
const QString kService = QStringLiteral("dev.danielmiguelt.Vortia.TestMenuRegistrar");

struct Window {
    QWidget root;
    QMenuBar source{&root};
    Window() {
        source.addMenu(QStringLiteral("File"))->addAction(QStringLiteral("Open"));
        source.addMenu(QStringLiteral("Edit"));
    }
};

bool waitFor(const std::function<bool()>& done) {
    QDeadlineTimer deadline(5000);
    while (!done() && !deadline.hasExpired())
        QCoreApplication::processEvents(QEventLoop::AllEvents, 50);
    return done();
}

}  // namespace

TEST(HeaderMenus, WithoutAGlobalMenuTheMenusSitInTheHeader) {
    Window w;
    HeaderMenus menus(&w.source, &w.root, kService);
    EXPECT_FALSE(HeaderMenus::shellHasGlobalMenu(kService));
    EXPECT_FALSE(menus.inlineMenus()->isHidden());
    EXPECT_TRUE(menus.menuButton()->isHidden());
    EXPECT_FALSE(menus.inlineMenus()->isNativeMenuBar());
    EXPECT_EQ(menus.inlineMenus()->actions(), w.source.actions());
}

TEST(HeaderMenus, AGlobalMenuLeavesOneButton) {
    Window w;
    HeaderMenus menus(&w.source, &w.root, kService);
    menus.setShellHasGlobalMenu(true);
    EXPECT_TRUE(menus.inlineMenus()->isHidden());
    EXPECT_FALSE(menus.menuButton()->isHidden());
    menus.setShellHasGlobalMenu(false);
    EXPECT_FALSE(menus.inlineMenus()->isHidden());
    EXPECT_TRUE(menus.menuButton()->isHidden());
}

TEST(HeaderMenus, TheButtonOpensTheWindowsMenusAsTheyAreNow) {
    Window w;
    HeaderMenus menus(&w.source, &w.root, kService);
    QMenu* popup = menus.menuButton()->menu();
    ASSERT_NE(popup, nullptr);
    w.source.addMenu(QStringLiteral("Help"));
    emit popup->aboutToShow();
    EXPECT_EQ(popup->actions(), w.source.actions());
    emit popup->aboutToShow();  // opening twice must not duplicate them
    EXPECT_EQ(popup->actions().size(), 3);
}

TEST(HeaderMenus, NoSourceMenuBarIsHarmless) {
    QWidget root;
    HeaderMenus menus(nullptr, &root, kService);
    emit menus.menuButton()->menu()->aboutToShow();
    EXPECT_TRUE(menus.menuButton()->menu()->actions().isEmpty());
    EXPECT_TRUE(menus.inlineMenus()->actions().isEmpty());
}

TEST(HeaderMenus, FollowsTheShellsMenuServiceAsItComesAndGoes) {
#ifndef VORTIA_HAS_QTDBUS
    GTEST_SKIP() << "built without Qt D-Bus";
#else
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.isConnected())
        GTEST_SKIP() << "no session bus";
    Window w;
    HeaderMenus menus(&w.source, &w.root, kService);
    ASSERT_FALSE(menus.inlineMenus()->isHidden());

    ASSERT_TRUE(bus.registerService(kService));
    EXPECT_TRUE(waitFor([&] { return menus.inlineMenus()->isHidden(); }));
    EXPECT_FALSE(menus.menuButton()->isHidden());
    EXPECT_TRUE(HeaderMenus::shellHasGlobalMenu(kService));

    {
        // A window opened while the shell already has the service starts with the button.
        HeaderMenus late(&w.source, &w.root, kService);
        EXPECT_TRUE(late.inlineMenus()->isHidden());
        EXPECT_FALSE(late.menuButton()->isHidden());
    }

    ASSERT_TRUE(bus.unregisterService(kService));
    EXPECT_TRUE(waitFor([&] { return !menus.inlineMenus()->isHidden(); }));
    EXPECT_TRUE(menus.menuButton()->isHidden());
#endif
}

int main(int argc, char** argv) {
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QApplication app(argc, argv);
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
