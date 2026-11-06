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

} // namespace vortia::detail
