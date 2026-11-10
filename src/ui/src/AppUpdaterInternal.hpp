#pragma once

#include <QByteArray>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QJsonObject>
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
