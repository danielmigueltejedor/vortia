#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QString>

namespace vortia::detail {

QString githubAssetSha256(const QJsonObject& asset);
bool matchesSha256(const QByteArray& data, const QString& expected);

} // namespace vortia::detail
