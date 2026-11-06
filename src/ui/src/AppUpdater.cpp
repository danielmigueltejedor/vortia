#include "vortia/ui/AppUpdater.hpp"
#include "AppUpdaterInternal.hpp"

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDesktopServices>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QRegularExpression>
#include <QSaveFile>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QVersionNumber>

#include <algorithm>

namespace vortia {
namespace {

// SHA-256 / macOS publisher helpers live in AppUpdaterInternal.hpp (header-only).

constexpr auto kReleasesApi =
    "https://api.github.com/repos/danielmigueltejedor/vortia/releases/latest";
constexpr auto kUserAgent = "Vortia-AppUpdater";

QString stripVersionPrefix(QString v) {
    v = v.trimmed();
    if (v.startsWith(QLatin1Char('v')) || v.startsWith(QLatin1Char('V')))
        v.remove(0, 1);
    // Drop pre-release / build metadata for ordering ("0.2.0-beta" → "0.2.0").
    const int cut = v.indexOf(QRegularExpression(QStringLiteral("[+-]")));
    if (cut > 0)
        v = v.left(cut);
    return v;
}

QString downloadDir() {
    const QString base = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    const QString dir = base + QStringLiteral("/update");
    QDir().mkpath(dir);
    QFile::setPermissions(dir, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    return dir;
}

QString currentAppImagePath() {
    const QByteArray env = qgetenv("APPIMAGE");
    return env.isEmpty() ? QString() : QString::fromUtf8(env);
}

QString macOSBundlePath() {
#ifdef Q_OS_MACOS
    // …/Vortia.app/Contents/MacOS/vortia → …/Vortia.app
    const QDir macOS(QCoreApplication::applicationDirPath());
    QDir contents(macOS);
    if (!contents.cdUp())
        return {};
    QDir bundle(contents);
    if (!bundle.cdUp())
        return {};
    if (!bundle.path().endsWith(QStringLiteral(".app"), Qt::CaseInsensitive))
        return {};
    return bundle.absolutePath();
#else
    return {};
#endif
}

#ifdef Q_OS_MACOS
bool readCodeIdentity(const QString& bundle, detail::MacOSCodeIdentity* identity, QString* error) {
    QProcess verify;
    verify.start(QStringLiteral("/usr/bin/codesign"),
                 {QStringLiteral("--verify"), QStringLiteral("--deep"), QStringLiteral("--strict"), bundle});
    if (!verify.waitForStarted() || !verify.waitForFinished(30'000) || verify.exitStatus() != QProcess::NormalExit
        || verify.exitCode() != 0) {
        if (error)
            *error = QObject::tr("The downloaded macOS app has an invalid code signature.");
        return false;
    }

    QProcess display;
    display.start(QStringLiteral("/usr/bin/codesign"), {QStringLiteral("-dv"), QStringLiteral("--verbose=4"), bundle});
    if (!display.waitForStarted() || !display.waitForFinished(30'000) || display.exitStatus() != QProcess::NormalExit
        || display.exitCode() != 0) {
        if (error)
            *error = QObject::tr("Could not identify the macOS app publisher.");
        return false;
    }
    const detail::MacOSCodeIdentity parsed =
        detail::parseCodesignDisplay(QString::fromLocal8Bit(display.readAllStandardError()));
    if (parsed.identifier.isEmpty()) {
        if (error)
            *error = QObject::tr("Could not identify the macOS app publisher.");
        return false;
    }
    if (identity)
        *identity = parsed;
    return true;
}

bool verifyMacOSUpdateSignature(const QString& dmgPath, QString* error) {
    const QString installedBundle = macOSBundlePath();
    QTemporaryDir mountDir(downloadDir() + QStringLiteral("/verify-XXXXXX"));
    if (!mountDir.isValid()) {
        if (error)
            *error = QObject::tr("Could not create a temporary mount point for signature verification.");
        return false;
    }

    QProcess attach;
    attach.start(QStringLiteral("/usr/bin/hdiutil"),
                 {QStringLiteral("attach"), QStringLiteral("-nobrowse"), QStringLiteral("-readonly"),
                  QStringLiteral("-mountpoint"), mountDir.path(), dmgPath});
    if (!attach.waitForStarted() || !attach.waitForFinished(60'000) || attach.exitStatus() != QProcess::NormalExit
        || attach.exitCode() != 0) {
        if (error)
            *error = QObject::tr("Could not mount the downloaded macOS update for verification.");
        return false;
    }

    QString verificationError;
    detail::MacOSCodeIdentity downloadedIdentity;
    detail::MacOSCodeIdentity installedIdentity;
    QString appPath;
    QDirIterator it(mountDir.path(), {QStringLiteral("Vortia.app")},
                    QDir::Dirs | QDir::NoDotAndDotDot, QDirIterator::Subdirectories);
    if (it.hasNext())
        appPath = it.next();
    bool valid = !appPath.isEmpty();
    if (!valid)
        verificationError = QObject::tr("The downloaded disk image does not contain Vortia.app.");
    if (valid)
        valid = readCodeIdentity(appPath, &downloadedIdentity, &verificationError);
    if (valid)
        valid = readCodeIdentity(installedBundle, &installedIdentity, &verificationError);
    if (valid)
        valid = detail::sameMacOSUpdatePublisher(installedIdentity, downloadedIdentity, &verificationError);

    QProcess detach;
    detach.start(QStringLiteral("/usr/bin/hdiutil"),
                 {QStringLiteral("detach"), mountDir.path(), QStringLiteral("-force")});
    const bool detached = detach.waitForStarted() && detach.waitForFinished(30'000)
        && detach.exitStatus() == QProcess::NormalExit && detach.exitCode() == 0;
    if (!detached && valid) {
        valid = false;
        verificationError = QObject::tr("Could not unmount the downloaded macOS update after verification.");
    }
    if (!valid && error)
        *error = verificationError;
    return valid;
}
#endif

} // namespace

int compareAppVersions(QString left, QString right) {
    const QVersionNumber a = QVersionNumber::fromString(stripVersionPrefix(std::move(left)));
    const QVersionNumber b = QVersionNumber::fromString(stripVersionPrefix(std::move(right)));
    return QVersionNumber::compare(a, b);
}

AppUpdater::AppUpdater(QObject* parent) : QObject(parent), network_(std::make_unique<QNetworkAccessManager>()) {}

AppUpdater::~AppUpdater() {
    cancel();
}

QString AppUpdater::currentVersion() {
    const QString v = QCoreApplication::applicationVersion();
    return v.isEmpty() ? QStringLiteral("0.0.0") : v;
}

bool AppUpdater::canApplyInPlace() {
#if defined(Q_OS_LINUX)
    return !currentAppImagePath().isEmpty();
#elif defined(Q_OS_MACOS)
    return !macOSBundlePath().isEmpty();
#else
    return false;
#endif
}

void AppUpdater::cancel() {
    if (QNetworkReply* reply = activeReply_) {
        activeReply_ = nullptr;
        disconnect(reply, nullptr, this, nullptr);
        reply->abort();
        reply->deleteLater();
    }
    checking_ = false;
    downloading_ = false;
}

void AppUpdater::checkForUpdates(bool userInitiated) {
    if (checking_ || downloading_)
        return;
    checking_ = true;
    QNetworkRequest req{QUrl(QString::fromUtf8(kReleasesApi))};
    req.setHeader(QNetworkRequest::UserAgentHeader, QString::fromUtf8(kUserAgent));
    req.setRawHeader("Accept", "application/vnd.github+json");
    req.setTransferTimeout(30'000);
    activeReply_ = network_->get(req);
    connect(activeReply_, &QNetworkReply::finished, this, [this, userInitiated]() {
        QNetworkReply* reply = activeReply_;
        activeReply_ = nullptr;
        checking_ = false;
        if (!reply)
            return;
        reply->deleteLater();
        handleReleaseReply(reply, userInitiated);
    });
}

void AppUpdater::handleReleaseReply(QNetworkReply* reply, bool userInitiated) {
    if (reply->error() != QNetworkReply::NoError) {
        emit checkFailed(tr("Could not reach GitHub Releases: %1").arg(reply->errorString()), userInitiated);
        return;
    }
    QString error;
    const AppUpdateInfo info = parseRelease(reply->readAll(), &error);
    if (!error.isEmpty()) {
        emit checkFailed(error, userInitiated);
        return;
    }
    if (compareAppVersions(info.version, currentVersion()) <= 0) {
        emit upToDate(currentVersion(), userInitiated);
        return;
    }
    if (info.assetUrl.isEmpty()) {
        emit checkFailed(tr("A new version (%1) is available, but no package for this platform was found.")
                             .arg(info.version),
                         userInitiated);
        return;
    }
    emit updateAvailable(info);
}

AppUpdateInfo AppUpdater::parseRelease(const QByteArray& json, QString* error) const {
    AppUpdateInfo info;
    const QJsonDocument doc = QJsonDocument::fromJson(json);
    if (!doc.isObject()) {
        if (error)
            *error = tr("Unexpected response from GitHub Releases.");
        return info;
    }
    const QJsonObject root = doc.object();
    info.tag = root.value(QStringLiteral("tag_name")).toString();
    info.version = stripVersionPrefix(info.tag);
    info.releaseNotes = root.value(QStringLiteral("body")).toString();
    info.releasePage = QUrl(root.value(QStringLiteral("html_url")).toString());
    if (info.version.isEmpty()) {
        if (error)
            *error = tr("Latest release has no version tag.");
        return info;
    }
    info.assetUrl = selectAssetUrl(root.value(QStringLiteral("assets")).toArray(), &info.assetName,
                                   &info.assetSha256, &info.assetBytes);
    return info;
}

QUrl AppUpdater::selectAssetUrl(const QJsonArray& assets, QString* name, QString* sha256, qint64* bytes) const {
    QStringList preferred;
#if defined(Q_OS_MACOS)
    #if defined(Q_PROCESSOR_ARM) || defined(__aarch64__)
    preferred << QStringLiteral("macos-arm64.dmg") << QStringLiteral("macos-universal.dmg")
              << QStringLiteral("macos.dmg");
    #else
    preferred << QStringLiteral("macos-x86_64.dmg") << QStringLiteral("macos-intel.dmg")
              << QStringLiteral("macos-universal.dmg") << QStringLiteral("macos.dmg");
    #endif
#elif defined(Q_OS_LINUX)
    preferred << QStringLiteral("linux-x86_64.AppImage") << QStringLiteral("linux-amd64.AppImage")
              << QStringLiteral("x86_64.AppImage") << QStringLiteral(".AppImage");
#endif
    for (const QString& needle : preferred) {
        for (const QJsonValue& value : assets) {
            const QJsonObject asset = value.toObject();
            const QString assetName = asset.value(QStringLiteral("name")).toString();
            if (!assetName.contains(needle, Qt::CaseInsensitive))
                continue;
            if (name)
                *name = assetName;
            if (sha256)
                *sha256 = detail::githubAssetSha256(asset);
            if (bytes)
                *bytes = static_cast<qint64>(asset.value(QStringLiteral("size")).toDouble());
            return QUrl(asset.value(QStringLiteral("browser_download_url")).toString());
        }
    }
    return {};
}

void AppUpdater::downloadAndApply(const AppUpdateInfo& info) {
    if (downloading_ || info.assetUrl.isEmpty())
        return;

    static const QRegularExpression digestPattern(QStringLiteral("^[0-9a-fA-F]{64}$"));
    if (!digestPattern.match(info.assetSha256).hasMatch()) {
        emit applyFailed(tr("The release does not provide a trusted SHA-256 digest for this package."));
        return;
    }

    if (!canApplyInPlace()) {
        // Dev builds / unpackaged installs: open the release page instead.
        if (!info.releasePage.isEmpty())
            QDesktopServices::openUrl(info.releasePage);
        emit applyFailed(tr("This installation cannot be updated in place. Open the release page to download the new package."));
        return;
    }

    downloading_ = true;
    emit applyStarted(tr("Downloading Vortia %1…").arg(info.version));

    QNetworkRequest req{info.assetUrl};
    req.setHeader(QNetworkRequest::UserAgentHeader, QString::fromUtf8(kUserAgent));
    req.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    req.setTransferTimeout(0); // large packages
    activeReply_ = network_->get(req);
    connect(activeReply_, &QNetworkReply::downloadProgress, this, &AppUpdater::downloadProgress);
    connect(activeReply_, &QNetworkReply::finished, this, [this, info]() {
        QNetworkReply* reply = activeReply_;
        activeReply_ = nullptr;
        downloading_ = false;
        if (!reply)
            return;
        reply->deleteLater();
        handleDownloadFinished(reply, info);
    });
}

void AppUpdater::handleDownloadFinished(QNetworkReply* reply, AppUpdateInfo info) {
    emit downloadFinished();

    if (reply->error() != QNetworkReply::NoError) {
        emit applyFailed(tr("Download failed: %1").arg(reply->errorString()));
        return;
    }

    const QString suffix = QFileInfo(info.assetName).completeSuffix().isEmpty()
        ? QFileInfo(info.assetUrl.path()).fileName()
        : info.assetName;
    const QString dest = downloadDir() + QLatin1Char('/') + (suffix.isEmpty() ? QStringLiteral("vortia-update.bin") : QFileInfo(suffix).fileName());
    {
        QSaveFile file(dest);
        if (!file.open(QIODevice::WriteOnly)) {
            emit applyFailed(tr("Could not write update package: %1").arg(file.errorString()));
            return;
        }
        const QByteArray data = reply->readAll();
        if (!detail::matchesSha256(data, info.assetSha256)) {
            QFile::remove(dest);
            emit applyFailed(tr("The downloaded update failed its SHA-256 integrity check."));
            return;
        }
        if (file.write(data) != data.size() || !file.commit()) {
            emit applyFailed(tr("Could not save update package."));
            return;
        }
    }
    QFile::setPermissions(dest, QFile::permissions(dest) | QFileDevice::ExeUser | QFileDevice::ExeGroup | QFileDevice::ExeOther);

    QString error;
#ifdef Q_OS_MACOS
    if (!verifyMacOSUpdateSignature(dest, &error)) {
        QFile::remove(dest);
        emit applyFailed(error);
        return;
    }
#endif
    if (!applyDownloadedFile(dest, info, &error)) {
        emit applyFailed(error.isEmpty() ? tr("Could not apply the update.") : error);
        return;
    }
}

bool AppUpdater::applyDownloadedFile(const QString& path, const AppUpdateInfo& info, QString* error) {
#if defined(Q_OS_LINUX)
    const QString appImage = currentAppImagePath();
    if (appImage.isEmpty()) {
        if (error)
            *error = tr("Not running from an AppImage.");
        return false;
    }
    const QString backup = appImage + QStringLiteral(".vortia-backup");
    QFile::remove(backup);
    if (!QFile::rename(appImage, backup)) {
        if (error)
            *error = tr("Could not move the current AppImage aside.");
        return false;
    }
    if (!QFile::copy(path, appImage)) {
        QFile::rename(backup, appImage);
        if (error)
            *error = tr("Could not install the new AppImage.");
        return false;
    }
    QFile::setPermissions(appImage, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner
                                        | QFileDevice::ReadGroup | QFileDevice::ExeGroup
                                        | QFileDevice::ReadOther | QFileDevice::ExeOther);
    QFile::remove(path);
    emit applyStarted(tr("Restarting Vortia %1…").arg(info.version));
    if (!QProcess::startDetached(appImage, {})) {
        QFile::remove(appImage);
        QFile::rename(backup, appImage);
        if (error)
            *error = tr("Could not launch the updated AppImage.");
        return false;
    }
    QFile::remove(backup);
    QCoreApplication::quit();
    return true;
#elif defined(Q_OS_MACOS)
    const QString bundle = macOSBundlePath();
    if (bundle.isEmpty()) {
        if (error)
            *error = tr("Not running from a Vortia.app bundle.");
        return false;
    }
    // Helper waits for this process to exit, then swaps the .app from the DMG.
    const QString scriptPath = downloadDir() + QStringLiteral("/apply-macos-update.sh");
    const QString mountPoint = downloadDir() + QStringLiteral("/dmg-mount");
    QDir().mkpath(mountPoint);
    const QString script = QStringLiteral(
        "#!/bin/bash\n"
        "set -euo pipefail\n"
        "DMG=$1\n"
        "MOUNT=$2\n"
        "BUNDLE=$3\n"
        "PARENT=$4\n"
        "sleep 2\n"
        "while kill -0 \"$PARENT\" 2>/dev/null; do sleep 0.5; done\n"
        "hdiutil detach \"$MOUNT\" -force >/dev/null 2>&1 || true\n"
        "rm -rf \"$MOUNT\"\n"
        "mkdir -p \"$MOUNT\"\n"
        "hdiutil attach \"$DMG\" -nobrowse -readonly -mountpoint \"$MOUNT\"\n"
        "SRC=$(find \"$MOUNT\" -maxdepth 2 -name 'Vortia.app' -type d | head -n1)\n"
        "if [[ -z \"$SRC\" ]]; then hdiutil detach \"$MOUNT\" -force || true; exit 1; fi\n"
        "TMP=\"${BUNDLE}.vortia-new\"\n"
        "BAK=\"${BUNDLE}.vortia-backup\"\n"
        "rm -rf \"$TMP\" \"$BAK\"\n"
        "ditto \"$SRC\" \"$TMP\"\n"
        "mv \"$BUNDLE\" \"$BAK\"\n"
        "if ! mv \"$TMP\" \"$BUNDLE\"; then mv \"$BAK\" \"$BUNDLE\"; exit 1; fi\n"
        "hdiutil detach \"$MOUNT\" -force >/dev/null 2>&1 || true\n"
        "rm -rf \"$BAK\" \"$DMG\"\n"
        "open \"$BUNDLE\"\n"
        "rm -f \"$0\"\n");
    {
        QFile scriptFile(scriptPath);
        if (!scriptFile.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
            if (error)
                *error = tr("Could not write the macOS update helper.");
            return false;
        }
        scriptFile.write(script.toUtf8());
        scriptFile.close();
        scriptFile.setPermissions(scriptFile.permissions() | QFileDevice::ExeUser);
    }
    emit applyStarted(tr("Installing Vortia %1 and restarting…").arg(info.version));
    if (!QProcess::startDetached(QStringLiteral("/bin/bash"),
                                 {scriptPath, path, mountPoint, bundle,
                                  QString::number(QCoreApplication::applicationPid())})) {
        if (error)
            *error = tr("Could not start the macOS update helper.");
        return false;
    }
    QCoreApplication::quit();
    return true;
#else
    Q_UNUSED(path);
    Q_UNUSED(info);
    if (error)
        *error = tr("In-place updates are not supported on this platform.");
    return false;
#endif
}

} // namespace vortia
