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
