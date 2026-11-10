// Runs the real macOS update helper against a real disk image and a stand-in
// application bundle. Reported for 0.4.0: the in-app update downloaded the new
// version and then never installed it.
#include <gtest/gtest.h>

#include "AppUpdaterInternal.hpp"

#include <QDir>
#include <QFile>
#include <QProcess>
#include <QTemporaryDir>

#ifdef Q_OS_MACOS

namespace {

bool run(const QString& program, const QStringList& arguments, const QProcessEnvironment& environment,
         int* exitCode = nullptr) {
    QProcess process;
    process.setProcessEnvironment(environment);
    process.start(program, arguments);
    if (!process.waitForStarted(10'000) || !process.waitForFinished(120'000))
        return false;
    if (exitCode)
        *exitCode = process.exitCode();
    return process.exitStatus() == QProcess::NormalExit;
}

void writeFile(const QString& path, const QByteArray& contents) {
    QDir().mkpath(QFileInfo(path).absolutePath());
    QFile file(path);
    ASSERT_TRUE(file.open(QIODevice::WriteOnly | QIODevice::Truncate)) << path.toStdString();
    file.write(contents);
}

QByteArray readFile(const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray("<missing>");
}

class UpdateHelper : public ::testing::Test {
protected:
    void SetUp() override {
        ASSERT_TRUE(dir_.isValid());
        root_ = dir_.path();
        bundle_ = root_ + "/Applications with space/Vortia.app";
        writeFile(bundle_ + "/Contents/version.txt", "old");
        writeFile(bundle_ + "/Contents/only-in-old.txt", "x");
        script_ = root_ + "/apply.sh";
        writeFile(script_, vortia::detail::macOSUpdateHelperScript().toUtf8());
        environment_ = QProcessEnvironment::systemEnvironment();
        // Never launch anything: record the "open" request instead.
        const QString opener = root_ + "/open-recorder.sh";
        writeFile(opener, ("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '" + root_ + "/opened.txt'\n").toUtf8());
        QFile::setPermissions(opener, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
        environment_.insert("VORTIA_UPDATE_OPEN", opener);
        environment_.insert("VORTIA_UPDATE_WAIT_STEPS", "4");
    }

    /// Disk image whose volume holds `folder` (created empty when absent).
    QString makeImage(const QString& name, bool withApplication) {
        const QString stage = root_ + "/stage-" + name;
        QDir().mkpath(stage);
        if (withApplication)
            writeFile(stage + "/Vortia.app/Contents/version.txt", "new");
        else
            writeFile(stage + "/README.txt", "no application here");
        const QString image = root_ + "/" + name + ".dmg";
        int code = -1;
        EXPECT_TRUE(run("/usr/bin/hdiutil", {"create", "-srcfolder", stage, "-volname", "Vortia test", "-fs", "HFS+",
                                             "-format", "UDZO", image}, environment_, &code));
        EXPECT_EQ(code, 0);
        return image;
    }

    int apply(const QString& image, qint64 parentPid) {
        int code = -1;
        EXPECT_TRUE(run("/bin/bash", {script_, image, root_ + "/mount", bundle_, QString::number(parentPid),
                                      root_ + "/update.log", root_ + "/result.txt"}, environment_, &code))
            << readFile(root_ + "/update.log").toStdString();
        return code;
    }

    /// Pid of a process that has already exited.
    static qint64 finishedPid() {
        QProcess process;
        process.start("/usr/bin/true", QStringList{});
        process.waitForStarted();
        const qint64 pid = process.processId();
        process.waitForFinished();
        return pid;
    }

    QTemporaryDir dir_;
    QString root_, bundle_, script_;
    QProcessEnvironment environment_;
};

TEST_F(UpdateHelper, ReplacesTheApplicationAndReopensIt) {
    const QString image = makeImage("good", true);
    EXPECT_EQ(apply(image, finishedPid()), 0) << readFile(root_ + "/update.log").toStdString();
    EXPECT_EQ(readFile(bundle_ + "/Contents/version.txt"), "new");
    EXPECT_FALSE(QFile::exists(bundle_ + "/Contents/only-in-old.txt"));
    EXPECT_EQ(readFile(root_ + "/result.txt").trimmed(), "ok");
    EXPECT_EQ(readFile(root_ + "/opened.txt").trimmed(), bundle_.toUtf8());
    // Nothing is left behind: no backup, no staging copy, no package, no mount.
    EXPECT_FALSE(QFile::exists(bundle_ + ".vortia-backup"));
    EXPECT_FALSE(QFile::exists(bundle_ + ".vortia-new"));
    EXPECT_FALSE(QFile::exists(image));
    EXPECT_TRUE(QDir(root_ + "/mount").isEmpty());
}

// The integrated engine is a disk image inside the application. The helper
// must release the one of the application it replaces, and leave alone an
// image mounted from anywhere else (another running copy of Vortia).
TEST_F(UpdateHelper, ReleasesOnlyTheEngineVolumeOfTheReplacedApplication) {
    const QString update = makeImage("with-engine", true);
    const auto mountEngine = [&](const QString& imageDir, const QString& name) {
        const QString stage = root_ + "/engine-stage-" + name;
        writeFile(stage + "/etc/bashrc", "engine");
        QDir().mkpath(imageDir);
        const QString image = imageDir + "/" + name + ".dmg";
        const QString mount = root_ + "/engine-mount-" + name;
        QDir().mkpath(mount);
        int code = -1;
        EXPECT_TRUE(run("/usr/bin/hdiutil", {"create", "-srcfolder", stage, "-volname", name, "-fs", "HFS+",
                                             "-format", "UDZO", image}, environment_, &code));
        EXPECT_EQ(code, 0);
        EXPECT_TRUE(run("/usr/bin/hdiutil", {"attach", image, "-nobrowse", "-readonly", "-mountpoint", mount},
                        environment_, &code));
        EXPECT_EQ(code, 0);
        return mount;
    };
    const QString own = mountEngine(bundle_ + "/Contents/Resources/Engine", "own");
    const QString foreign = mountEngine(root_ + "/Other copy/Vortia.app/Contents/Resources/Engine", "foreign");
    ASSERT_TRUE(QFile::exists(own + "/etc/bashrc"));
    ASSERT_TRUE(QFile::exists(foreign + "/etc/bashrc"));

    EXPECT_EQ(apply(update, finishedPid()), 0) << readFile(root_ + "/update.log").toStdString();
    EXPECT_EQ(readFile(bundle_ + "/Contents/version.txt"), "new");
    EXPECT_FALSE(QFile::exists(own + "/etc/bashrc")) << "engine of the replaced application still mounted";
    EXPECT_TRUE(QFile::exists(foreign + "/etc/bashrc")) << "engine of another copy was unmounted";
    int code = -1;
    run("/usr/bin/hdiutil", {"detach", foreign, "-force"}, environment_, &code);
}

TEST_F(UpdateHelper, KeepsTheOldApplicationWhenTheImageHasNoVortia) {
    const QString image = makeImage("empty", false);
    EXPECT_EQ(apply(image, finishedPid()), 1);
    EXPECT_EQ(readFile(bundle_ + "/Contents/version.txt"), "old");
    EXPECT_EQ(readFile(root_ + "/result.txt").trimmed(), "contents");
    // The previous version is started again and the package is kept.
    EXPECT_EQ(readFile(root_ + "/opened.txt").trimmed(), bundle_.toUtf8());
    EXPECT_TRUE(QFile::exists(image));
    EXPECT_TRUE(QDir(root_ + "/mount").isEmpty());
}

TEST_F(UpdateHelper, ReportsAnUnreadableImageInsteadOfHanging) {
    const QString image = root_ + "/broken.dmg";
    writeFile(image, "this is not a disk image");
    EXPECT_EQ(apply(image, finishedPid()), 1);
    EXPECT_EQ(readFile(bundle_ + "/Contents/version.txt"), "old");
    EXPECT_EQ(readFile(root_ + "/result.txt").trimmed(), "mount");
}

TEST_F(UpdateHelper, GivesUpWithoutTouchingAnythingWhileVortiaIsStillRunning) {
    const QString image = makeImage("waiting", true);
    // This test process plays the application that never exits.
    EXPECT_EQ(apply(image, QCoreApplication::applicationPid()), 1);
    EXPECT_EQ(readFile(bundle_ + "/Contents/version.txt"), "old");
    EXPECT_EQ(readFile(root_ + "/result.txt").trimmed(), "still-running");
    EXPECT_FALSE(QFile::exists(root_ + "/opened.txt"));
    EXPECT_TRUE(QFile::exists(image));
}

TEST_F(UpdateHelper, RefusesABundlePathThatIsNotVortia) {
    const QString image = makeImage("refused", true);
    const QString victim = root_ + "/Documents";
    writeFile(victim + "/keep.txt", "precious");
    bundle_ = victim;
    EXPECT_EQ(apply(image, finishedPid()), 2);
    EXPECT_EQ(readFile(victim + "/keep.txt"), "precious");
    EXPECT_EQ(readFile(root_ + "/result.txt").trimmed(), "internal");
}

} // namespace

#endif // Q_OS_MACOS
