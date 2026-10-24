#pragma once
#include <QString>
class QWidget;
namespace vortia { class VtkViewport;
void showLineProfile(QWidget* parent,VtkViewport* viewport,const QString& field);
}
