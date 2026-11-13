#pragma once

#include <QByteArray>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>
#include <QRegularExpression>
#include <QString>

namespace vortia::detail {

inline QString githubAssetSha256(const QJsonObject& asset) {
    static const QRegularExpression digestPattern(QStringLiteral("^sha256:([0-9a-fA-F]{64})$"));
    const auto match = digestPattern.match(asset.value(QStringLiteral("digest")).toString());
    return match.hasMatch() ? match.captured(1).toLower() : QString();
}

inline bool matchesSha256(const QByteArray& data, const QString& expected) {
    static const QRegularExpression digestPattern(QStringLiteral("^[0-9a-fA-F]{64}$"));
    if (!digestPattern.match(expected).hasMatch())
        return false;
    const QByteArray actual = QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex();
    return actual == expected.toLatin1().toLower();
}

/// The systems the in-app updater installs on, and the package each one takes.
enum class UpdatePlatform { MacOSArm64, MacOSIntel, LinuxX86_64, Other };

[[nodiscard]] inline UpdatePlatform currentUpdatePlatform() {
#if defined(Q_OS_MACOS) && (defined(Q_PROCESSOR_ARM) || defined(__aarch64__))
    return UpdatePlatform::MacOSArm64;
#elif defined(Q_OS_MACOS)
    return UpdatePlatform::MacOSIntel;
#elif defined(Q_OS_LINUX) && (defined(Q_PROCESSOR_X86_64) || defined(__x86_64__))
    return UpdatePlatform::LinuxX86_64;
#else
    return UpdatePlatform::Other;
#endif
}

/// The ends of this system's package names, best first. Matched on the end of the name, so a
/// `.zsync` or `.sha256` beside the package never counts, and never for another CPU.
[[nodiscard]] inline QStringList preferredAssetSuffixes(UpdatePlatform platform) {
    switch (platform) {
    case UpdatePlatform::MacOSArm64:
        return {QStringLiteral("macos-arm64.dmg"), QStringLiteral("macos-universal.dmg"),
                QStringLiteral("macos.dmg")};
    case UpdatePlatform::MacOSIntel:
        return {QStringLiteral("macos-x86_64.dmg"), QStringLiteral("macos-intel.dmg"),
                QStringLiteral("macos-universal.dmg"), QStringLiteral("macos.dmg")};
    case UpdatePlatform::LinuxX86_64:
        return {QStringLiteral("linux-x86_64.AppImage"), QStringLiteral("linux-amd64.AppImage"),
                QStringLiteral("x86_64.AppImage")};
    case UpdatePlatform::Other:
        break;
    }
    return {};
}

/// The release carries Vortia Native (SwiftUI) beside this app as `Vortia-<v>-native-macos-…`:
/// a different app, which this one must never install over itself.
[[nodiscard]] inline bool isOtherEdition(const QString& assetName) {
    return assetName.contains(QStringLiteral("-native-"), Qt::CaseInsensitive);
}

/// This system's package among a release's assets, or an empty object.
[[nodiscard]] inline QJsonObject selectUpdateAsset(const QJsonArray& assets, UpdatePlatform platform) {
    for (const QString& suffix : preferredAssetSuffixes(platform)) {
        for (const QJsonValue& value : assets) {
            const QJsonObject asset = value.toObject();
            const QString name = asset.value(QStringLiteral("name")).toString();
            if (isOtherEdition(name) || !name.endsWith(suffix, Qt::CaseInsensitive))
                continue;
            if (asset.value(QStringLiteral("browser_download_url")).toString().isEmpty())
                continue;
            return asset;
        }
    }
    return {};
}

/// The newest published release (not a draft or pre-release) with a package for this system,
/// from the `releases` list (newest first). Not `releases/latest`: a release for one system only
/// (a macOS-only fix) must not be offered on the other. With no such release, the newest
/// published one; with none at all, an empty object (nothing to offer).
[[nodiscard]] inline QJsonObject chooseUpdateRelease(const QJsonArray& releases, UpdatePlatform platform) {
    QJsonObject newest;
    for (const QJsonValue& value : releases) {
        const QJsonObject release = value.toObject();
        if (release.value(QStringLiteral("draft")).toBool()
            || release.value(QStringLiteral("prerelease")).toBool())
            continue;
        if (newest.isEmpty())
            newest = release;
        if (!selectUpdateAsset(release.value(QStringLiteral("assets")).toArray(), platform).isEmpty())
            return release;
    }
    return newest;
}

/// How `codesign` checks an app's seal before an update. Not `--strict`: it also refuses the
/// Finder metadata every file on a disk image carries, so every good update failed with
/// "invalid code signature". Without it the signature, the sealed files and nested code are
/// still verified.
[[nodiscard]] inline QStringList codesignVerifyArguments() {
    return {QStringLiteral("--verify"), QStringLiteral("--deep"), QStringLiteral("--verbose=2")};
}

/// Identity extracted from `codesign -dv --verbose=4` stderr.
struct MacOSCodeIdentity {
    QString identifier;     ///< e.g. "dev.vortia.app"
    QString teamIdentifier; ///< Team ID, or empty / "not set" for ad-hoc
    bool adhoc{false};      ///< True when Signature=adhoc or no Team ID
};

[[nodiscard]] inline MacOSCodeIdentity parseCodesignDisplay(const QString& codesignDvStderr) {
    MacOSCodeIdentity identity;
    static const QRegularExpression teamPattern(QStringLiteral("(?:^|\\n)TeamIdentifier=([^\\r\\n]+)"));
    static const QRegularExpression identifierPattern(QStringLiteral("(?:^|\\n)Identifier=([^\\r\\n]+)"));
    static const QRegularExpression signaturePattern(QStringLiteral("(?:^|\\n)Signature=([^\\r\\n]+)"));
    const auto teamMatch = teamPattern.match(codesignDvStderr);
    const auto identifierMatch = identifierPattern.match(codesignDvStderr);
    const auto signatureMatch = signaturePattern.match(codesignDvStderr);
    if (identifierMatch.hasMatch())
        identity.identifier = identifierMatch.captured(1).trimmed();
    if (teamMatch.hasMatch())
        identity.teamIdentifier = teamMatch.captured(1).trimmed();
    const bool teamUnset =
        identity.teamIdentifier.isEmpty() || identity.teamIdentifier == QStringLiteral("not set");
    const bool signatureAdhoc =
        signatureMatch.hasMatch()
        && signatureMatch.captured(1).trimmed().compare(QStringLiteral("adhoc"), Qt::CaseInsensitive) == 0;
    identity.adhoc = signatureAdhoc || teamUnset;
    return identity;
}

/// True when an in-place update from `installed` to `downloaded` keeps the same publisher.
/// Ad-hoc builds (current public macOS releases) may update to another ad-hoc build with
/// the same bundle identifier. Developer ID teams must match exactly when present.
[[nodiscard]] inline bool sameMacOSUpdatePublisher(const MacOSCodeIdentity& installed,
                                                   const MacOSCodeIdentity& downloaded,
                                                   QString* error = nullptr) {
    if (installed.identifier.isEmpty() || downloaded.identifier.isEmpty()) {
        if (error)
            *error = QCoreApplication::translate("vortia::detail",
                                                 "Could not identify the macOS app publisher.");
        return false;
    }
    if (installed.identifier != downloaded.identifier) {
        if (error)
            *error = QCoreApplication::translate(
                "vortia::detail", "The downloaded macOS app is signed by a different publisher.");
        return false;
    }
    if (installed.adhoc && downloaded.adhoc)
        return true;
    if (installed.adhoc != downloaded.adhoc
        || installed.teamIdentifier != downloaded.teamIdentifier) {
        if (error)
            *error = QCoreApplication::translate(
                "vortia::detail", "The downloaded macOS app is signed by a different publisher.");
        return false;
    }
    return true;
}

/// Shell helper that swaps Vortia.app for the copy inside a downloaded disk
/// image once the running application has exited.
///
/// Arguments: <dmg> <mount-dir> <bundle> <parent-pid> <log-file> <result-file>
///
/// It never fails silently: every step is logged, the outcome is written to
/// <result-file> ("ok", or a short failure code read back at the next start),
/// and on failure the previous application is put back and reopened. The
/// helper gives up if the application has not exited after about a minute
/// instead of waiting for ever. `VORTIA_UPDATE_OPEN` and
/// `VORTIA_UPDATE_WAIT_STEPS` exist for the tests.
[[nodiscard]] inline QString macOSUpdateHelperScript() {
    return QStringLiteral(R"SH(#!/bin/bash
DMG=$1
MOUNT=$2
BUNDLE=$3
PARENT=$4
LOG=$5
RESULT=$6
exec >>"$LOG" 2>&1
echo "== $(date '+%Y-%m-%d %H:%M:%S') update helper: $DMG -> $BUNDLE"
case "$BUNDLE" in
  /*/Vortia.app) ;;
  *) echo "refusing unexpected bundle path"; printf 'internal\n' >"$RESULT"; exit 2 ;;
esac
NEW="$BUNDLE.vortia-new"
OLD="$BUNDLE.vortia-backup"
OPEN="${VORTIA_UPDATE_OPEN:-/usr/bin/open}"

fail() {
  echo "FAILED at: $1"
  printf '%s\n' "$1" >"$RESULT"
  /usr/bin/hdiutil detach "$MOUNT" -force >/dev/null 2>&1
  if [ ! -d "$BUNDLE" ] && [ -d "$OLD" ]; then /bin/mv "$OLD" "$BUNDLE"; fi
  [ -d "$NEW" ] && /bin/rm -rf "$NEW"
  "$OPEN" "$BUNDLE"
  exit 1
}

# The application must be gone before its files are replaced.
steps="${VORTIA_UPDATE_WAIT_STEPS:-120}"
while [ "$steps" -gt 0 ] && kill -0 "$PARENT" 2>/dev/null; do
  sleep 0.5
  steps=$((steps - 1))
done
if kill -0 "$PARENT" 2>/dev/null; then
  echo "Vortia (pid $PARENT) is still running: nothing was changed"
  printf 'still-running\n' >"$RESULT"
  exit 1
fi

# The integrated engine is a disk image inside the old application: release
# the volumes backed by images under this bundle, and only those (another
# copy of Vortia may be running from somewhere else).
/usr/bin/hdiutil info | /usr/bin/awk -v bundle="$BUNDLE/" '
  /^image-path/ { sub(/^image-path[ \t]*:[ \t]*/, ""); inside = (index($0, bundle) == 1) }
  inside && /^\/dev\// { n = split($0, part, "\t"); if (part[n] ~ /^\//) print part[n] }
' | while IFS= read -r volume; do
  echo "releasing engine volume $volume"
  /usr/bin/hdiutil detach "$volume" -force
done

/usr/bin/hdiutil detach "$MOUNT" -force >/dev/null 2>&1
/bin/mkdir -p "$MOUNT" || fail mount
/usr/bin/hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$MOUNT" || fail mount
SOURCE="$MOUNT/Vortia.app"
[ -d "$SOURCE" ] || fail contents

[ -e "$NEW" ] && /bin/rm -rf "$NEW"
[ -e "$OLD" ] && /bin/rm -rf "$OLD"
/usr/bin/ditto "$SOURCE" "$NEW" || fail copy
/bin/mv "$BUNDLE" "$OLD" || fail replace
if ! /bin/mv "$NEW" "$BUNDLE"; then
  /bin/mv "$OLD" "$BUNDLE"
  fail replace
fi
/usr/bin/hdiutil detach "$MOUNT" -force >/dev/null 2>&1
/bin/rm -rf "$OLD"
/bin/rm -f "$DMG"
printf 'ok\n' >"$RESULT"
echo "updated; starting $BUNDLE"
"$OPEN" "$BUNDLE"
/bin/rm -f "$0"
)SH");
}

} // namespace vortia::detail
