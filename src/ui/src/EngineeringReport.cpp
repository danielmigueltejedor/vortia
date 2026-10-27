#include "vortia/ui/EngineeringReport.hpp"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QJsonDocument>
#include <QLocale>
#include <QRegularExpression>
#include <cmath>
#include <stdexcept>
namespace vortia {
namespace {
QString reportTr(const char* text){return QCoreApplication::translate("vortia::Engineering",text);}
QString digest(const QJsonObject& object){return QString::fromLatin1(QCryptographicHash::hash(QJsonDocument(object).toJson(QJsonDocument::Compact),QCryptographicHash::Sha256).toHex());}
QString csvString(QString value){value.replace('"',"\"\"");if(!value.trimmed().isEmpty()&&QString("=+-@").contains(value.trimmed().front()))value.prepend('\'');return '"'+value+'"';}
}
QStringList engineeringMetricKeys(){return {"area_m2","massFlow_kg_s","forwardMassFlow_kg_s","reverseMassFlow_kg_s","volumeFlow_m3_s","meanPressure_Pa","meanDensity_kg_m3","meanNormalVelocity_m_s","momentumFlux_N","pressureLoad_N","normalResultant_N","kineticPower_W","normalCoefficient","meanCp"};}
QString engineeringMetricTitle(const QString& key){
    const QStringList titles{reportTr("Section area (m²)"),reportTr("Signed mass flow (kg/s)"),reportTr("Forward mass flow (kg/s)"),reportTr("Reverse mass flow (kg/s)"),reportTr("Signed volume flow (m³/s)"),reportTr("Area-mean pressure (Pa)"),reportTr("Area-mean density (kg/m³)"),reportTr("Area-mean normal speed (m/s)"),reportTr("Normal momentum flux (N)"),reportTr("Pressure term (N)"),reportTr("Normal section resultant (N)"),reportTr("Signed kinetic power (W)"),reportTr("Normal section coefficient"),reportTr("Mean pressure coefficient")};
    const int index=engineeringMetricKeys().indexOf(key);return index>=0?titles.at(index):key;
}
QJsonObject makeEngineeringReport(const SectionOptions& o,const SectionAnalysis& r,const QJsonObject& source,const QString& label){
    QJsonObject payload{{"algorithm","section-quadrature-v1"},{"label",label.trimmed().left(160)},{"createdUtc",QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs)},{"source",source},{"options",sectionOptionsJson(o)},{"metrics",sectionMetricsJson(r)},{"sampleFingerprint",r.sampleFingerprint},{"triangles",r.triangles},{"duplicateTriangles",r.duplicateTriangles},{"ghostTriangles",r.ghostTriangles},{"volumeBlocks",r.blocks},{"areaScale",r.areaScale},{"assessment","stored_fields_only; convergence_and_physical_validation_not_established"}};
    return {{"schema","vortia.engineering-report"},{"version",1},{"payload",payload},{"sha256",digest(payload)}};
}
bool validEngineeringReport(const QJsonObject& report){
    if(report.value("schema")!="vortia.engineering-report"||report.value("version").toInt()!=1||!report.value("payload").isObject())return false;
    const auto p=report.value("payload").toObject();if(report.value("sha256").toString()!=digest(p)||p.value("algorithm")!="section-quadrature-v1"||p.value("label").toString().size()>160)return false;
    static const QRegularExpression hex("^[0-9a-f]{64}$");if(!hex.match(p.value("sampleFingerprint").toString()).hasMatch()||!p.value("source").isObject()||!p.value("options").isObject()||!p.value("metrics").isObject())return false;
    const auto m=p.value("metrics").toObject();for(const auto& key:engineeringMetricKeys())if(!m.value(key).isDouble()||!std::isfinite(m.value(key).toDouble()))return false;
    if(m.value("area_m2").toDouble()<=0||m.value("meanDensity_kg_m3").toDouble()<=0||m.value("forwardMassFlow_kg_s").toDouble()<0||m.value("reverseMassFlow_kg_s").toDouble()<0)return false;
    const auto definition=p.value("options").toObject();
    const auto expected=sectionOptionsJson(SectionOptions{});for(auto i=expected.begin();i!=expected.end();++i)if(!definition.contains(i.key())||definition.value(i.key()).type()!=i.value().type())return false;
    if(p.value("triangles").toInt()<1||p.value("volumeBlocks").toInt()<1||!QDateTime::fromString(p.value("createdUtc").toString(),Qt::ISODateWithMs).isValid())return false;
    const auto o=sectionOptionsFromJson(p.value("options").toObject());if(o.axis<0||o.axis>2||(o.normal!=1&&o.normal!=-1)||o.referenceDensity<=0||o.referenceSpeed<=0||o.referenceArea<=0||o.constantDensity<=0)return false;
    if(o.fullAnnulus&&(o.axis!=0||o.wedgeAngle<=0||o.wedgeAngle>5))return false;
    const auto options=p.value("options").toObject();for(auto i=options.begin();i!=options.end();++i)if(i.value().isDouble()&&!std::isfinite(i.value().toDouble()))return false;
    return true;
}
QString engineeringComparisonWarning(const QJsonObject& a,const QJsonObject& b){
    const auto x=a.value("payload").toObject().value("options").toObject(),y=b.value("payload").toObject().value("options").toObject();
    for(const auto* key:{"axis","normal","position","referencePressure","referenceDensity","referenceSpeed","referenceArea","useDensityField","constantDensity","kinematicPressure","fullAnnulus","wedgeAngle"})if(x.value(key)!=y.value(key))return reportTr("Different section or reference definitions: compare the assumptions before interpreting a change.");
    return {};
}
QString engineeringReportHtml(const QJsonObject& report){
    if(!validEngineeringReport(report))throw std::runtime_error("Invalid engineering report");const auto p=report.value("payload").toObject();
    QString html="<!doctype html><html lang=\""+QLocale().name().left(2)+"\"><meta charset=\"utf-8\"><title>Vortia</title><style>body{font:15px system-ui,sans-serif;max-width:1000px;margin:40px auto;padding:0 24px;color:#25313e}table{border-collapse:collapse;width:100%;margin:20px 0}td,th{border-bottom:1px solid #d9dfe6;padding:10px;text-align:left}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f2f5f7;padding:16px}h1{color:#177886}small{overflow-wrap:anywhere}</style><h1>"+p.value("label").toString().toHtmlEscaped()+"</h1><p>"+reportTr("Engineering section analysis · stored CFD fields").toHtmlEscaped()+"</p><p>"+reportTr("The section resultant combines normal momentum flux and the pressure term. It is not a complete aircraft force, engine thrust or viscous wall-load report. Numerical convergence, model suitability and mesh independence require separate evidence.").toHtmlEscaped()+"</p><table><tr><th>"+reportTr("Quantity")+"</th><th>"+reportTr("Value")+"</th></tr>";
    if(p.value("source").toObject().contains("projectConfigurationAtAnalysis"))html.insert(html.indexOf("<table>"),"<p>"+reportTr("Project settings were captured at analysis time; their match to the producing run has not been verified.").toHtmlEscaped()+"</p>");
    const auto metrics=p.value("metrics").toObject();for(const auto& key:engineeringMetricKeys())html+="<tr><td>"+engineeringMetricTitle(key).toHtmlEscaped()+"</td><td>"+QString::number(metrics.value(key).toDouble(),'g',17)+"</td></tr>";
    html+="</table><h2>"+reportTr("Definitions and provenance")+"</h2><pre>"+QString::fromUtf8(QJsonDocument(p).toJson(QJsonDocument::Indented)).toHtmlEscaped()+"</pre><small>SHA-256: "+report.value("sha256").toString()+"</small><p>"+reportTr("The digest detects file changes; it is not an approval or a digital signature.")+"</p></html>";return html;
}
QByteArray engineeringReportCsv(const QJsonObject& report){
    if(!validEngineeringReport(report))throw std::runtime_error("Invalid engineering report");const auto p=report.value("payload").toObject(),m=p.value("metrics").toObject();
    const auto source=csvString(QString::fromUtf8(QJsonDocument(p.value("source").toObject()).toJson(QJsonDocument::Compact))),options=csvString(QString::fromUtf8(QJsonDocument(p.value("options").toObject()).toJson(QJsonDocument::Compact)));
    QString text="schema,report_sha256,label,sample_sha256,metric,value,algorithm,created_utc,assessment,source_json,options_json\n";for(const auto& key:engineeringMetricKeys())text+="vortia.engineering-report,"+report.value("sha256").toString()+','+csvString(p.value("label").toString())+','+p.value("sampleFingerprint").toString()+','+key+','+QString::number(m.value(key).toDouble(),'g',17)+','+csvString(p.value("algorithm").toString())+','+csvString(p.value("createdUtc").toString())+','+csvString(p.value("assessment").toString())+','+source+','+options+'\n';return text.toUtf8();
}
QByteArray engineeringComparisonCsv(const QJsonArray& reports,const QString& metric,int baseline){
    if(!engineeringMetricKeys().contains(metric)||baseline<0||baseline>=reports.size())throw std::runtime_error("Invalid comparison baseline");
    for(const auto& value:reports)if(!validEngineeringReport(value.toObject()))throw std::runtime_error("Invalid engineering report");
    const auto base=reports[baseline].toObject();const double reference=base.value("payload").toObject().value("metrics").toObject().value(metric).toDouble();
    QString csv="label,report_sha256,baseline_sha256,metric,value,change_percent,definition_warning,source_json,options_json\n";
    for(const auto& item:reports){const auto report=item.toObject(),p=report.value("payload").toObject();const double value=p.value("metrics").toObject().value(metric).toDouble();
        csv+=csvString(p.value("label").toString())+','+report.value("sha256").toString()+','+base.value("sha256").toString()+','+metric+','+QString::number(value,'g',17)+','+(reference!=0?QString::number(100*(value-reference)/std::abs(reference),'g',17):QString{})+','+csvString(engineeringComparisonWarning(base,report))+','+csvString(QString::fromUtf8(QJsonDocument(p.value("source").toObject()).toJson(QJsonDocument::Compact)))+','+csvString(QString::fromUtf8(QJsonDocument(p.value("options").toObject()).toJson(QJsonDocument::Compact)))+'\n';
    }return csv.toUtf8();
}
}
