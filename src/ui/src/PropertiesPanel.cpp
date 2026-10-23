#include "vortia/ui/PropertiesPanel.hpp"

#include "vortia/physics/CaseSetupJson.hpp"

#include <QComboBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QLabel>
#include <QLocale>
#include <QPainter>
#include <QPaintEvent>
#include <QSizePolicy>
#include <cmath>
#include <algorithm>
#include <cstdint>
#include <QSpinBox>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace vortia {

/// A non-solver preview of the blockMesh X/Y cell distribution. Actual mesh
/// validation still happens in OpenFOAM's blockMesh/checkMesh.
class MeshPreview2D final : public QWidget {
public:
    explicit MeshPreview2D(QWidget* parent = nullptr) : QWidget(parent) {
        setMinimumHeight(190);
        setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
        setToolTip(QObject::tr("Structured grid preview (XY). Not a computed mesh."));
    }
    void setParameters(int x, int y, double gx, double gy) {
        nx_ = x;
        ny_ = y;
        gx_ = gx;
        gy_ = gy;
        update();
    }

protected:
    void paintEvent(QPaintEvent*) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        const QRectF frame = QRectF(rect()).adjusted(10, 10, -10, -10);
        painter.fillRect(frame, palette().base());
        painter.setPen(QPen(palette().mid().color(), 1));
        painter.drawRect(frame);
        painter.setPen(QPen(palette().highlight().color(), 0.7));
        const auto coordinate = [](int i, int n, double ratio) {
            if (n <= 1 || std::abs(ratio - 1.0) < 1e-9)
                return static_cast<double>(i) / static_cast<double>(n);
            // OpenFOAM simpleGrading is a ratio of LAST/FIRST cell widths.
            const double q = std::log(ratio) / (n - 1);
            return std::expm1(i * q) / std::expm1(n * q);
        };
        for (int i = 1; i < nx_; i += std::max(1, nx_ / 80)) {
            const double t = coordinate(i, nx_, gx_);
            painter.drawLine(QPointF(frame.left() + frame.width() * t, frame.top()),
                             QPointF(frame.left() + frame.width() * t, frame.bottom()));
        }
        for (int i = 1; i < ny_; i += std::max(1, ny_ / 80)) {
            const double t = coordinate(i, ny_, gy_);
            painter.drawLine(QPointF(frame.left(), frame.top() + frame.height() * t),
                             QPointF(frame.right(), frame.top() + frame.height() * t));
        }
    }
private:
    int nx_{20};
    int ny_{20};
    double gx_{1.0};
    double gy_{1.0};
};

PropertiesPanel::PropertiesPanel(QWidget* parent) : QWidget(parent) {
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(14, 16, 14, 14);
    layout->setSpacing(12);
    stack_ = new QStackedWidget(this);
    layout->addWidget(stack_);

    buildDomainPage();
    buildMeshPage();
    buildMaterialPage();
    buildBoundaryPage();
    buildSolverPage();
    buildResultsPage();

    applyButton_ = new QPushButton(tr("Apply"), this);
    connect(applyButton_, &QPushButton::clicked, this, [this]() {
        applyEditorsToSetup();
        const auto issues = setup_.validate();
        QString report;
        for (const auto& i : issues)
            report += (i.severity == Severity::Error ? QStringLiteral("ERROR: ")
                                                   : QStringLiteral("warning: ")) +
                      QString::fromStdString(i.message) + "\n";
        if (!report.isEmpty())
            emit validationReported(report.trimmed());
        emit setupChanged(setup_);
    });
    applyButton_->setMinimumHeight(36);
    applyButton_->setToolTip(tr("Validate and apply the current simulation settings."));
    layout->addWidget(applyButton_);
}

void PropertiesPanel::buildDomainPage() {
    auto* page = new QWidget(this);
    auto* form = new QFormLayout(page);
    form->setHorizontalSpacing(12);
    form->setVerticalSpacing(10);
    form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
    dx0_ = new QDoubleSpinBox(page);
    dx1_ = new QDoubleSpinBox(page);
    dy0_ = new QDoubleSpinBox(page);
    dy1_ = new QDoubleSpinBox(page);
    dz0_ = new QDoubleSpinBox(page);
    dz1_ = new QDoubleSpinBox(page);
    for (auto* s : {dx0_, dx1_, dy0_, dy1_, dz0_, dz1_}) {
        s->setRange(-1e6, 1e6);
        s->setDecimals(4);
        s->setSingleStep(0.01);
    }
    form->addRow(tr("min x"), dx0_);
    form->addRow(tr("max x"), dx1_);
    form->addRow(tr("min y"), dy0_);
    form->addRow(tr("max y"), dy1_);
    form->addRow(tr("min z"), dz0_);
    form->addRow(tr("max z"), dz1_);
    auto* hint = new QLabel(tr("Domain extents (m)."), page);
    hint->setWordWrap(true);
    form->addRow(hint);
    stack_->addWidget(page);
}

void PropertiesPanel::buildMeshPage() {
    auto* page = new QWidget(this);
    auto* form = new QFormLayout(page);
    form->setHorizontalSpacing(12);
    form->setVerticalSpacing(10);
    form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
    cellsX_ = new QSpinBox(page);
    cellsY_ = new QSpinBox(page);
    cellsZ_ = new QSpinBox(page);
    for (auto* s : {cellsX_, cellsY_, cellsZ_}) {
        s->setRange(1, 5000);
        s->setValue(20);
        s->setToolTip(tr("Number of CFD cells in this direction. Increasing it affects solver cost."));
    }
    form->addRow(tr("Cells X"), cellsX_);
    form->addRow(tr("Cells Y"), cellsY_);
    form->addRow(tr("Cells Z"), cellsZ_);
    gradingX_ = new QDoubleSpinBox(page);
    gradingY_ = new QDoubleSpinBox(page);
    gradingZ_ = new QDoubleSpinBox(page);
    for (auto* g : {gradingX_, gradingY_, gradingZ_}) {
        g->setRange(0.05, 20.0);
        g->setDecimals(3);
        g->setSingleStep(0.1);
        g->setValue(1.0);
        g->setToolTip(tr("OpenFOAM simpleGrading: ratio of final to first cell width."));
    }
    form->addRow(tr("Grading X"), gradingX_);
    form->addRow(tr("Grading Y"), gradingY_);
    form->addRow(tr("Grading Z"), gradingZ_);
    meshEstimate_ = new QLabel(page);
    meshEstimate_->setWordWrap(true);
    form->addRow(meshEstimate_);
    meshPreview_ = new MeshPreview2D(page);
    form->addRow(tr("XY preview"), meshPreview_);

    auto update = [this]() { updateMeshEstimate(); };
    for (auto* s : {cellsX_, cellsY_, cellsZ_})
        connect(s, QOverload<int>::of(&QSpinBox::valueChanged), this, update);
    for (auto* g : {gradingX_, gradingY_, gradingZ_})
        connect(g, QOverload<double>::of(&QDoubleSpinBox::valueChanged), this, update);
    updateMeshEstimate();
    stack_->addWidget(page);
}

void PropertiesPanel::updateMeshEstimate() {
    if (!meshEstimate_ || !meshPreview_)
        return;
    const qint64 n = static_cast<qint64>(cellsX_->value()) *
                     static_cast<qint64>(cellsY_->value()) *
                     static_cast<qint64>(cellsZ_->value());
    const Vec3 size = setup_.domain.size();
    const double dx = size.x / static_cast<double>(cellsX_->value());
    const double dy = size.y / static_cast<double>(cellsY_->value());
    const double dz = size.z / static_cast<double>(cellsZ_->value());
    const QString count = QLocale().toString(n);
    meshEstimate_->setText(tr("Total cells: %1. Average size X/Y/Z: %2 / %3 / %4 mm%5")
        .arg(count)
        .arg(dx * 1000, 0, 'g', 4)
        .arg(dy * 1000, 0, 'g', 4)
        .arg(dz * 1000, 0, 'g', 4)
        .arg(n > 2000000 ? tr(". WARNING: high memory and compute cost.") : QString()));
    meshPreview_->setParameters(cellsX_->value(), cellsY_->value(),
                                gradingX_->value(), gradingY_->value());
}

void PropertiesPanel::buildMaterialPage() {
    auto* page = new QWidget(this);
    auto* form = new QFormLayout(page);
    form->setHorizontalSpacing(12);
    form->setVerticalSpacing(10);
    form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
    nuEdit_ = new QDoubleSpinBox(page);
    nuEdit_->setRange(1e-12, 1.0);
    nuEdit_->setDecimals(6);
    nuEdit_->setSingleStep(1e-5);
    form->addRow(tr("nu (m²/s)"), nuEdit_);
    stack_->addWidget(page);
}

void PropertiesPanel::buildBoundaryPage() {
    auto* page = new QWidget(this);
    auto* form = new QFormLayout(page);
    form->setHorizontalSpacing(12);
    form->setVerticalSpacing(10);
    form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
    lidUx_ = new QDoubleSpinBox(page);
    lidUx_->setRange(-1000, 1000);
    lidUx_->setDecimals(3);
    lidUx_->setSingleStep(0.1);
    form->addRow(tr("lid velocity Ux (m/s)"), lidUx_);
    auto* hint = new QLabel(tr("Cavity template: the upper lid drives the flow."), page);
    hint->setWordWrap(true);
    form->addRow(hint);
    stack_->addWidget(page);
}

void PropertiesPanel::buildSolverPage() {
    auto* page = new QWidget(this);
    auto* form = new QFormLayout(page);
    form->setHorizontalSpacing(12);
    form->setVerticalSpacing(10);
    form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
    solverCombo_ = new QComboBox(page);
    solverCombo_->addItem(tr("Transient laminar (icoFoam)"), static_cast<int>(SolverApp::IcoFoam));
    solverCombo_->addItem(tr("Steady laminar (simpleFoam)"), static_cast<int>(SolverApp::SimpleFoam));
    endTimeEdit_ = new QDoubleSpinBox(page);
    endTimeEdit_->setRange(0.0, 1e9);
    endTimeEdit_->setDecimals(4);
    deltaTEdit_ = new QDoubleSpinBox(page);
    deltaTEdit_->setRange(1e-9, 1e3);
    deltaTEdit_->setDecimals(6);
    writeIntervalEdit_ = new QSpinBox(page);
    writeIntervalEdit_->setRange(1, 100000);
    form->addRow(tr("Solver"), solverCombo_);
    form->addRow(tr("End time (s)"), endTimeEdit_);
    form->addRow(tr("Delta T (s)"), deltaTEdit_);
    form->addRow(tr("Write interval (steps)"), writeIntervalEdit_);
    stack_->addWidget(page);
}

void PropertiesPanel::buildResultsPage() {
    auto* page = new QWidget(this);
    auto* layout = new QVBoxLayout(page);
    layout->addWidget(new QLabel(tr("Run a simulation to see results here."), page));
    stack_->addWidget(page);
}

void PropertiesPanel::setSetup(const CaseSetup& setup) {
    setup_ = setup;
    loadEditorsFromSetup();
}

void PropertiesPanel::showNode(int nodeId) {
    // Geometry=0, Mesh=1, Materials=2, BC=3, Solver=4, Results=5
    if (nodeId >= 0 && nodeId < stack_->count())
        stack_->setCurrentIndex(nodeId);
}

void PropertiesPanel::loadEditorsFromSetup() {
    const Vec3 mn = setup_.domain.min();
    const Vec3 mx = setup_.domain.max();
    dx0_->setValue(mn.x);
    dx1_->setValue(mx.x);
    dy0_->setValue(mn.y);
    dy1_->setValue(mx.y);
    dz0_->setValue(mn.z);
    dz1_->setValue(mx.z);

    cellsX_->setValue(setup_.mesh.cells[0]);
    cellsY_->setValue(setup_.mesh.cells[1]);
    cellsZ_->setValue(setup_.mesh.cells[2]);
    gradingX_->setValue(setup_.mesh.grading[0]);
    gradingY_->setValue(setup_.mesh.grading[1]);
    gradingZ_->setValue(setup_.mesh.grading[2]);
    updateMeshEstimate();

    nuEdit_->setValue(setup_.material.nu.siValue());

    double lidU = 0.0;
    for (const auto& b : setup_.boundaries)
        if (b.velocity == VelocityBc::FixedValue)
            lidU = b.velocityValue.x;
    lidUx_->setValue(lidU);

    solverCombo_->setCurrentIndex(solverCombo_->findData(static_cast<int>(setup_.solver)));
    endTimeEdit_->setValue(setup_.time.endTime);
    deltaTEdit_->setValue(setup_.time.deltaT);
    writeIntervalEdit_->setValue(setup_.time.writeInterval);
}

void PropertiesPanel::applyEditorsToSetup() {
    auto domain = BoxDomain::fromExtents({dx0_->value(), dy0_->value(), dz0_->value()},
                                         {dx1_->value(), dy1_->value(), dz1_->value()});
    if (domain) {
        setup_.domain = domain.value();
    } else {
        emit validationReported(tr("ERROR: Domain dimensions must be positive along every axis."));
        return;
    }

    setup_.mesh.cells = {cellsX_->value(), cellsY_->value(), cellsZ_->value()};
    setup_.mesh.grading = {gradingX_->value(), gradingY_->value(), gradingZ_->value()};

    setup_.material.nu = KinematicViscosity(nuEdit_->value());

    // Apply lid velocity to the first fixed-value boundary (cavity template).
    for (auto& b : setup_.boundaries)
        if (b.velocity == VelocityBc::FixedValue) {
            b.velocityValue = {lidUx_->value(), 0.0, 0.0};
            break;
        }

    setup_.solver = static_cast<SolverApp>(solverCombo_->currentData().toInt());
    setup_.time.endTime = endTimeEdit_->value();
    setup_.time.deltaT = deltaTEdit_->value();
    setup_.time.writeInterval = writeIntervalEdit_->value();
    if (setup_.solver == SolverApp::SimpleFoam)
        setup_.numerics.ddt = TimeScheme::SteadyState;
    else
        setup_.numerics.ddt = TimeScheme::Euler;
    updateMeshEstimate();
}

} // namespace vortia
