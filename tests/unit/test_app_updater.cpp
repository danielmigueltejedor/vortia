#include "AppUpdaterInternal.hpp"

#include <gtest/gtest.h>

#include <QCryptographicHash>
#include <QJsonObject>

TEST(AppUpdater, ParsesGitHubAssetSha256) {
    const QByteArray data("release package");
    const QString digest = QString::fromLatin1(
        QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex());
    const QJsonObject asset{{QStringLiteral("digest"), QStringLiteral("sha256:") + digest}};

    EXPECT_EQ(vortia::detail::githubAssetSha256(asset), digest);
    EXPECT_TRUE(vortia::detail::matchesSha256(data, digest));
    EXPECT_FALSE(vortia::detail::matchesSha256(data + 'x', digest));
    EXPECT_FALSE(vortia::detail::matchesSha256(data, QStringLiteral("sha256:") + digest));
    EXPECT_TRUE(vortia::detail::githubAssetSha256(QJsonObject{}).isEmpty());
    EXPECT_TRUE(vortia::detail::githubAssetSha256(
                    {{QStringLiteral("digest"), QStringLiteral("sha256:invalid")}})
                    .isEmpty());
}

TEST(AppUpdater, ParsesAdhocCodesignDisplay) {
    const QString output = QStringLiteral(
        "Executable=/Applications/Vortia.app/Contents/MacOS/vortia\n"
        "Identifier=dev.vortia.app\n"
        "Format=app bundle with Mach-O thin (arm64)\n"
        "Signature=adhoc\n"
        "TeamIdentifier=not set\n");
    const auto identity = vortia::detail::parseCodesignDisplay(output);
    EXPECT_EQ(identity.identifier, QStringLiteral("dev.vortia.app"));
    EXPECT_EQ(identity.teamIdentifier, QStringLiteral("not set"));
    EXPECT_TRUE(identity.adhoc);
}

TEST(AppUpdater, ParsesDeveloperIdCodesignDisplay) {
    const QString output = QStringLiteral(
        "Identifier=dev.vortia.app\n"
        "Authority=Developer ID Application: Example (AB12CD34EF)\n"
        "TeamIdentifier=AB12CD34EF\n");
    const auto identity = vortia::detail::parseCodesignDisplay(output);
    EXPECT_EQ(identity.identifier, QStringLiteral("dev.vortia.app"));
    EXPECT_EQ(identity.teamIdentifier, QStringLiteral("AB12CD34EF"));
    EXPECT_FALSE(identity.adhoc);
}

TEST(AppUpdater, AllowsAdhocToAdhocSameIdentifier) {
    vortia::detail::MacOSCodeIdentity installed;
    installed.identifier = QStringLiteral("dev.vortia.app");
    installed.teamIdentifier = QStringLiteral("not set");
    installed.adhoc = true;
    vortia::detail::MacOSCodeIdentity downloaded = installed;
    QString error;
    EXPECT_TRUE(vortia::detail::sameMacOSUpdatePublisher(installed, downloaded, &error));
    EXPECT_TRUE(error.isEmpty());
}

TEST(AppUpdater, RejectsAdhocWhenIdentifierDiffers) {
    vortia::detail::MacOSCodeIdentity installed;
    installed.identifier = QStringLiteral("dev.vortia.app");
    installed.adhoc = true;
    vortia::detail::MacOSCodeIdentity downloaded;
    downloaded.identifier = QStringLiteral("com.example.other");
    downloaded.adhoc = true;
    QString error;
    EXPECT_FALSE(vortia::detail::sameMacOSUpdatePublisher(installed, downloaded, &error));
    EXPECT_FALSE(error.isEmpty());
}

TEST(AppUpdater, RejectsAdhocVersusDeveloperId) {
    vortia::detail::MacOSCodeIdentity installed;
    installed.identifier = QStringLiteral("dev.vortia.app");
    installed.adhoc = true;
    vortia::detail::MacOSCodeIdentity downloaded;
    downloaded.identifier = QStringLiteral("dev.vortia.app");
    downloaded.teamIdentifier = QStringLiteral("AB12CD34EF");
    downloaded.adhoc = false;
    QString error;
    EXPECT_FALSE(vortia::detail::sameMacOSUpdatePublisher(installed, downloaded, &error));
}

TEST(AppUpdater, AllowsMatchingDeveloperIdTeams) {
    vortia::detail::MacOSCodeIdentity installed;
    installed.identifier = QStringLiteral("dev.vortia.app");
    installed.teamIdentifier = QStringLiteral("AB12CD34EF");
    installed.adhoc = false;
    vortia::detail::MacOSCodeIdentity downloaded = installed;
    EXPECT_TRUE(vortia::detail::sameMacOSUpdatePublisher(installed, downloaded, nullptr));
}

namespace {

QJsonObject asset(const char* name) {
    return {{QStringLiteral("name"), QString::fromLatin1(name)},
            {QStringLiteral("browser_download_url"), QStringLiteral("https://e/") + QString::fromLatin1(name)}};
}

QJsonObject release(const char* tag, QJsonArray assets, bool draft = false, bool prerelease = false) {
    return {{QStringLiteral("tag_name"), QString::fromLatin1(tag)},
            {QStringLiteral("draft"), draft},
            {QStringLiteral("prerelease"), prerelease},
            {QStringLiteral("assets"), assets}};
}

} // namespace

// 0.7.0 ships Vortia Native beside the Qt app; the Qt app must take its own disk image
// whatever order GitHub lists the assets in.
TEST(AppUpdater, TakesItsOwnDiskImageNotVortiaNative) {
    using vortia::detail::UpdatePlatform;
    const QJsonArray assets{asset("Vortia-0.7.0-native-macos-arm64.dmg"), asset("Vortia-0.7.0-macos-arm64.dmg"),
                            asset("Vortia-0.7.0-linux-x86_64.AppImage.zsync"),
                            asset("Vortia-0.7.0-linux-x86_64.AppImage")};
    EXPECT_EQ(vortia::detail::selectUpdateAsset(assets, UpdatePlatform::MacOSArm64).value("name").toString(),
              QStringLiteral("Vortia-0.7.0-macos-arm64.dmg"));
    EXPECT_EQ(vortia::detail::selectUpdateAsset(assets, UpdatePlatform::LinuxX86_64).value("name").toString(),
              QStringLiteral("Vortia-0.7.0-linux-x86_64.AppImage"));
    EXPECT_TRUE(vortia::detail::selectUpdateAsset(assets, UpdatePlatform::MacOSIntel).isEmpty())
        << "an Intel Mac is never offered the arm64 image";
    const QJsonArray onlyNative{asset("Vortia-0.7.0-native-macos-arm64.dmg")};
    EXPECT_TRUE(vortia::detail::selectUpdateAsset(onlyNative, UpdatePlatform::MacOSArm64).isEmpty());
}

TEST(AppUpdater, OffersTheNewestReleaseWithAPackageForThisSystem) {
    using vortia::detail::UpdatePlatform;
    const QJsonArray releases{
        release("v0.9.0", {asset("Vortia-0.9.0-macos-arm64.dmg")}, true),
        release("v0.8.1", {asset("Vortia-0.8.1-macos-arm64.dmg")}),
        release("v0.8.0-rc.1", {asset("Vortia-0.8.0-rc.1-linux-x86_64.AppImage")}, false, true),
        release("v0.8.0", {asset("Vortia-0.8.0-linux-x86_64.AppImage"), asset("Vortia-0.8.0-macos-arm64.dmg")}),
    };
    EXPECT_EQ(vortia::detail::chooseUpdateRelease(releases, UpdatePlatform::MacOSArm64).value("tag_name").toString(),
              QStringLiteral("v0.8.1"));
    EXPECT_EQ(vortia::detail::chooseUpdateRelease(releases, UpdatePlatform::LinuxX86_64).value("tag_name").toString(),
              QStringLiteral("v0.8.0"))
        << "a macOS-only release is not offered on Linux";
    EXPECT_TRUE(vortia::detail::chooseUpdateRelease(QJsonArray{}, UpdatePlatform::LinuxX86_64).isEmpty());
}

TEST(AppUpdater, SealCheckIsNotStrict) {
    const QStringList args = vortia::detail::codesignVerifyArguments();
    EXPECT_FALSE(args.contains(QStringLiteral("--strict")))
        << "--strict refuses the Finder metadata every file on a disk image carries";
    EXPECT_TRUE(args.contains(QStringLiteral("--verify")) && args.contains(QStringLiteral("--deep")));
}
