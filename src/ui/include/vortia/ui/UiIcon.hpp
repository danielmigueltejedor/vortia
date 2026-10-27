#pragma once

#include <QApplication>
#include <QIcon>
#include <QFile>
#include <utility>
#include <QIconEngine>
#include <QPainter>
#include <QPalette>

namespace vortia {

// Use the original SVG's alpha, but resolve its ink at paint time. Existing
// actions and dialogs therefore follow a live appearance change, including
// disabled states, without rebuilding actions or changing their shortcuts.
class UiIconEngine final : public QIconEngine {
public:
    explicit UiIconEngine(QString source) : source_(std::move(source)) {}
    QIconEngine* clone() const override { return new UiIconEngine(source_); }
    QString key() const override { return QStringLiteral("vortia-precision"); }
    QPixmap pixmap(const QSize& size, QIcon::Mode mode, QIcon::State state) override {
        auto image = QIcon(source_).pixmap(size);
        if (image.isNull()) return image;
        const auto palette = QApplication::palette();
        const auto ink = palette.color(mode == QIcon::Disabled ? QPalette::Disabled : QPalette::Active,
                                       mode == QIcon::Disabled ? QPalette::WindowText : mode == QIcon::Selected ? QPalette::HighlightedText : state == QIcon::On ? QPalette::Link : QPalette::WindowText);
        QPainter painter(&image);
        painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
        painter.fillRect(image.rect(), ink);
        return image;
    }
    void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
        const qreal ratio = painter->device()->devicePixelRatioF();
        auto image = pixmap(rect.size() * ratio, mode, state);
        image.setDevicePixelRatio(ratio);
        painter->drawPixmap(rect, image);
    }
private:
    QString source_;
};

inline QIcon uiIcon(const QString& source) {
    if(!QFile::exists(source))return {};
    return QIcon(new UiIconEngine(source));
}

} // namespace vortia
