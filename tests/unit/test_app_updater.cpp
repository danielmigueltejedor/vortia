#include "vortia/ui/AppUpdater.hpp"
#include "../../src/ui/src/AppUpdaterInternal.hpp"

#include <gtest/gtest.h>

#include <QCryptographicHash>
#include <QJsonObject>

using vortia::compareAppVersions;

TEST(AppUpdater, CompareVersions) {
    EXPECT_LT(compareAppVersions("0.1.0", "0.2.0"), 0);
    EXPECT_GT(compareAppVersions("v0.2.0", "0.1.9"), 0);
    EXPECT_EQ(compareAppVersions("0.1.0", "v0.1.0"), 0);
    EXPECT_LT(compareAppVersions("0.1.0-beta", "0.1.1"), 0);
}

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
