#pragma once
#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <vtkSmartPointer.h>
#include <functional>
class vtkDataObject;
class QComboBox;
class QDoubleSpinBox;
class QCheckBox;
class QLineEdit;
class QLabel;
class QPushButton;
class QTableWidget;
class QTextBrowser;
namespace vortia {
class EngineeringWorkbench final:public QDialog {
public:
    EngineeringWorkbench(vtkDataObject*,double wedgeAngle,const QJsonObject& source,
                         const QJsonObject& saved={},bool canSave=false,QWidget* parent=nullptr);
    std::function<void(const QJsonObject&)> saveToProject;
    QJsonObject workspaceState() const;
    QJsonObject currentReport() const{return report_;}
    void evaluate();
private:
    void invalidate();
    void refreshComparison();
    void importReports();
    void exportReport();
    vtkSmartPointer<vtkDataObject> data_;
    double wedgeAngle_{};
    QJsonObject source_,report_;
    QJsonArray comparisons_;
    QComboBox *axis_{},*normal_{},*densityMode_{},*pressureMode_{},*baseline_{},*metric_{};
    QDoubleSpinBox *position_{},*density_{},*referencePressure_{},*referenceDensity_{},*referenceSpeed_{},*referenceArea_{};
    QCheckBox* annulus_{};
    QLineEdit* name_{};
    QLabel *status_{},*comparisonStatus_{};
    QPushButton *add_{},*export_{},*save_{};
    QTableWidget *values_{},*comparison_{};
    QTextBrowser* reportPreview_{};
};
}
