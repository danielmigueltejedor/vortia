#pragma once
#include <QString>
class QWidget;
namespace vortia {
/// Creates a NEW external OpenFOAM case; never overwrites another case.
QString createNozzlePractice(QWidget* parent);
}
