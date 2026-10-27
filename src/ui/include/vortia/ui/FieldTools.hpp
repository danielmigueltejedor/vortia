#pragma once
#include <QString>
class QWidget;
namespace vortia {
class VtkViewport;
QString fieldDisplayName(const QString& field);
void showPlaneSection(QWidget* parent,VtkViewport* viewport);
void showPointProbe(QWidget* parent,VtkViewport* viewport,const QString& field);
void showFieldStatistics(QWidget* parent,VtkViewport* viewport,const QString& field);
void exportViewportImage(QWidget* parent,VtkViewport* viewport);
}
