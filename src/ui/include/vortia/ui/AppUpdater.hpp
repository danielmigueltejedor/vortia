#pragma once

#include <QObject>
#include <QString>
#include <QUrl>
#include <memory>

class QNetworkAccessManager;
class QNetworkReply;

namespace vortia {

/// Semantic-ish version compare: "1.2.3" / "v1.2.3". Returns <0, 0, >0.
int compareAppVersions(QString left, QString right);

struct AppUpdateInfo {
    QString version;       ///< Tag without leading 'v', e.g. "0.2.0"
    QString tag;           ///< Full tag, e.g. "v0.2.0"
    QString releaseNotes;
    QUrl releasePage;
    QUrl assetUrl;
    QString assetName;
    QString assetSha256;
    qint64 assetBytes{0};
};

/// Checks GitHub Releases for a newer build and can download + apply it
/// in place for AppImage (Linux) and .app-via-DMG (macOS).
class AppUpdater : public QObject {
    Q_OBJECT
public:
    explicit AppUpdater(QObject* parent = nullptr);
    ~AppUpdater() override;

    /// Current running version (from QCoreApplication::applicationVersion).
    static QString currentVersion();

    /// True when the process can replace itself (AppImage or macOS .app bundle).
    static bool canApplyInPlace();

    void checkForUpdates(bool userInitiated);
    void downloadAndApply(const AppUpdateInfo& info);
    void cancel();

signals:
    void checkFailed(const QString& message, bool userInitiated);
    void upToDate(const QString& currentVersion, bool userInitiated);
    void updateAvailable(const AppUpdateInfo& info);
    void downloadProgress(qint64 received, qint64 total);
    void downloadFinished();
    void applyFailed(const QString& message);
    void applyStarted(const QString& message);

private:
    void handleReleaseReply(QNetworkReply* reply, bool userInitiated);
    void handleDownloadFinished(QNetworkReply* reply, AppUpdateInfo info);
    bool applyDownloadedFile(const QString& path, const AppUpdateInfo& info, QString* error);
    AppUpdateInfo parseRelease(const QByteArray& json, QString* error) const;
    QUrl selectAssetUrl(const QJsonArray& assets, QString* name, QString* sha256, qint64* bytes) const;

    std::unique_ptr<QNetworkAccessManager> network_;
    QNetworkReply* activeReply_{nullptr};
    bool checking_{false};
    bool downloading_{false};
};

} // namespace vortia
