#include "Engine.hpp"

#include "vortia/core/Version.hpp"
#include "vortia/physics/CaseSetupJson.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/CaseRunner.hpp"
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/geometry/SketchFile.hpp"
#include "vortia/ui/BoundaryMapping.hpp"
#include "vortia/ui/FlowCase.hpp"
#include "vortia/ui/FlowConfiguration.hpp"
#include "vortia/ui/FlowSetup.hpp"
#include "vortia/ui/ResultFiles.hpp"

#include <QFile>
#include <QJsonDocument>
#include <QSaveFile>
#include <QThread>
#include "vortia/visualization/FieldStatistics.hpp"
#include "vortia/visualization/LineSampling.hpp"

#include <vtkCellData.h>
#include <vtkCellDataToPointData.h>
#include <vtkDataArray.h>
#include <vtkDataSet.h>
#include <vtkDataSetSurfaceFilter.h>
#include <vtkNew.h>
#include <vtkPointData.h>
#include <vtkPolyData.h>
#include <vtkSmartPointer.h>
#include <vtkTriangleFilter.h>
#include <vtkXMLMultiBlockDataReader.h>

#include <chrono>
#include <cmath>
#include <limits>
#include <stdexcept>

namespace vortia::engine {

using nlohmann::json;

namespace {

/// A request the client got wrong, answered as an error rather than a crash.
struct RequestError : std::runtime_error {
    using std::runtime_error::runtime_error;
};

/// The lid-driven cavity every new project starts from, as the desktop app does.
CaseSetup cavity() {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {0.1, 0.1, 0.01}).value();
    s.mesh.cells = {48, 48, 1};
    s.mesh.patches = {{
        {BoxDomain::XMin, "fixedWalls", PatchKind::Wall},
        {BoxDomain::XMax, "fixedWalls2", PatchKind::Wall},
        {BoxDomain::YMin, "fixedWalls3", PatchKind::Wall},
        {BoxDomain::YMax, "movingWall", PatchKind::Wall},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.name = "air";
    s.material.nu = KinematicViscosity(0.001);
    s.boundaries = {
        {"fixedWalls", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls2", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls3", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"movingWall", VelocityBc::FixedValue, {1.0, 0.0, 0.0}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.time = {0.0, 0.5, 0.001, 100, false, 1.0};
    s.solver = SolverApp::IcoFoam;
    return s;
}

std::string text(const json& request, const char* key) {
    const auto it = request.find(key);
    if (it == request.end() || !it->is_string() || it->get<std::string>().empty())
        throw RequestError(std::string("missing \"") + key + "\"");
    return it->get<std::string>();
}

Vec3 vec(const json& value, const char* key) {
    if (!value.is_array() || value.size() != 3)
        throw RequestError(std::string("\"") + key + "\" must be three numbers");
    return {value[0].get<double>(), value[1].get<double>(), value[2].get<double>()};
}

CaseSetup parseSetup(const json& setup) {
    auto parsed = caseSetupFromJson(setup.dump());
    if (!parsed)
        throw RequestError(parsed.error().describe());
    return parsed.value();
}

json issuesOf(const CaseSetup& setup) {
    json issues = json::array();
    for (const auto& issue : setup.validate())
        issues.push_back({{"severity", issue.severity == Severity::Error ? "error" : "warning"},
                          {"message", issue.message}});
    return issues;
}

bool hasErrors(const CaseSetup& setup) {
    for (const auto& issue : setup.validate())
        if (issue.severity == Severity::Error)
            return true;
    return false;
}

/// A finite number, or null: JSON has no NaN.
json number(double value) {
    return std::isfinite(value) ? json(value) : json(nullptr);
}

/// The value of one tuple, as a magnitude for vectors.
double scalar(vtkDataArray* array, vtkIdType tuple) {
    const int components = array->GetNumberOfComponents();
    if (components == 1)
        return array->GetComponent(tuple, 0);
    double sum = 0;
    for (int c = 0; c < components; ++c) {
        const double v = array->GetComponent(tuple, c);
        sum += v * v;
    }
    return std::sqrt(sum);
}

/// Triangles as flat vertex lists with a value per vertex: what a renderer draws without an
/// index buffer, and what a cell-centred field needs, since a vertex is not shared across cells.
json triangles(vtkPolyData* surface, const std::string& field) {
    vtkNew<vtkTriangleFilter> triangulate;
    triangulate->SetInputData(surface);
    triangulate->Update();
    vtkPolyData* mesh = triangulate->GetOutput();
    vtkDataArray* pointValues = mesh->GetPointData()->GetArray(field.c_str());
    vtkDataArray* cellValues = pointValues ? nullptr : mesh->GetCellData()->GetArray(field.c_str());

    std::vector<float> positions;
    std::vector<float> values;
    double low = std::numeric_limits<double>::infinity();
    double high = -low;
    for (vtkIdType cell = 0; cell < mesh->GetNumberOfCells(); ++cell) {
        vtkIdType count = 0;
        const vtkIdType* ids = nullptr;
        mesh->GetCellPoints(cell, count, ids);
        if (count != 3)
            continue;
        for (vtkIdType k = 0; k < 3; ++k) {
            double p[3];
            mesh->GetPoint(ids[k], p);
            positions.insert(positions.end(), {float(p[0]), float(p[1]), float(p[2])});
            double v = std::numeric_limits<double>::quiet_NaN();
            if (pointValues)
                v = scalar(pointValues, ids[k]);
            else if (cellValues)
                v = scalar(cellValues, cell);
            if (std::isfinite(v)) {
                low = std::min(low, v);
                high = std::max(high, v);
            }
            values.push_back(std::isfinite(v) ? float(v) : 0.0f);
        }
    }
    return {{"positions", positions},
            {"values", values},
            {"range", std::isfinite(low) ? json{low, high} : json(nullptr)},
            {"field", field}};
}

std::string_view stateName(Project::CaseState state) { return Project::toString(state); }

} // namespace

namespace {
std::optional<std::string> readFile(const std::filesystem::path& path, std::size_t limit);
} // namespace

struct Engine::Results {
    std::string file;
    vtkSmartPointer<vtkDataObject> data;
};

Engine::Engine(Emit sink, std::filesystem::path bundledOpenFoam)
    : emit_(std::move(sink)), bundledOpenFoam_(std::move(bundledOpenFoam)) {}

Engine::~Engine() {
    if (cancel_)
        cancel_->cancel();
    join();
}

void Engine::join() {
    if (worker_.joinable())
        worker_.join();
}

json Engine::handle(const json& request) {
    json reply{{"id", request.value("id", json(nullptr))}};
    try {
        if (!request.is_object())
            throw RequestError("a request is a JSON object");
        const std::string cmd = text(request, "cmd");
        json result;
        if (cmd == "hello") result = hello();
        else if (cmd == "detect") result = detect();
        else if (cmd == "create") result = createProject(request);
        else if (cmd == "open") result = openProject(request);
        else if (cmd == "save") result = save(request);
        else if (cmd == "validate") result = validate(request);
        else if (cmd == "mesh") result = startJob(request, false);
        else if (cmd == "solve") result = startJob(request, true);
        else if (cmd == "cancel") result = cancel();
        else if (cmd == "results") result = results();
        else if (cmd == "statistics") result = statistics(request);
        else if (cmd == "line") result = line(request);
        else if (cmd == "slice") result = slice(request);
        else if (cmd == "surface") result = surface(request);
        else if (cmd == "sketch") result = sketch();
        else if (cmd == "saveSketch") result = saveSketch(request);
        else if (cmd == "nozzle") result = nozzle(request);
        else if (cmd == "flow") result = flow();
        else if (cmd == "saveFlow") result = saveFlow(request);
        else if (cmd == "flowMesh") result = startFlow({{"meshOnly", true}});
        else if (cmd == "flowSolve") result = startFlow({{"meshOnly", false}});
        else throw RequestError("unknown command \"" + cmd + "\"");
        reply["ok"] = true;
        reply["result"] = std::move(result);
    } catch (const json::exception& error) {
        reply["ok"] = false;
        reply["error"] = std::string("malformed request: ") + error.what();
    } catch (const std::exception& error) {
        reply["ok"] = false;
        reply["error"] = error.what();
    }
    return reply;
}

json Engine::hello() {
    if (!installation_ && detectionError_.empty())
        detect();
    return {{"protocol", kProtocolVersion},
            {"version", VORTIA_VERSION},
            {"openfoam", describeOpenFoam()},
            {"project", describeProject()},
            {"running", running_.load()}};
}

json Engine::detect() {
    requireIdle();
    installation_.reset();
    environment_.clear();
    caps_ = {};
    detectionError_.clear();
    PosixProcessRunner runner;
    const auto found = openfoam::Detector::discover(runner, {}, bundledOpenFoam_);
    if (!found)
        detectionError_ = found.error().describe();
    else if (found->empty())
        detectionError_ = "No OpenFOAM installation was found.";
    else
        installation_ = found->front();
    return describeOpenFoam();
}

json Engine::describeOpenFoam() const {
    if (!installation_)
        return {{"found", false}, {"error", detectionError_}};
    return {{"found", true},
            {"fingerprint", installation_->fingerprint()},
            {"distribution", std::string(openfoam::toString(installation_->distribution))},
            {"root", installation_->rootDir.string()}};
}

json Engine::describeProject() const {
    std::lock_guard lock(projectMutex_);
    if (!project_)
        return nullptr;
    json setup = json::parse(project_->setupJson(), nullptr, false);
    return {{"name", project_->name()},
            {"root", project_->rootDir().string()},
            {"state", std::string(stateName(project_->caseState()))},
            {"setup", setup.is_discarded() ? json::object() : setup}};
}

void Engine::requireProject() const {
    std::lock_guard lock(projectMutex_);
    if (!project_)
        throw RequestError("no project is open");
}

void Engine::requireIdle() const {
    if (running_.load())
        throw RequestError("a calculation is running");
}

json Engine::createProject(const json& request) {
    requireIdle();
    auto created = Project::create(text(request, "path"), text(request, "name"));
    if (!created)
        throw RequestError(created.error().describe());
    created->setSetupJson(caseSetupToJson(cavity()));
    if (auto saved = created->save(); !saved)
        throw RequestError(saved.error().describe());
    {
        std::lock_guard lock(projectMutex_);
        project_ = std::move(created.value());
    }
    results_.reset();
    return describeProject();
}

json Engine::openProject(const json& request) {
    requireIdle();
    auto opened = Project::open(text(request, "path"));
    if (!opened)
        throw RequestError(opened.error().describe());
    const std::string stored = opened->setupJson();
    if (stored.empty() || stored == "{}")
        opened->setSetupJson(caseSetupToJson(cavity()));
    else if (auto parsed = caseSetupFromJson(stored); !parsed)
        throw RequestError("the case setup cannot be read: " + parsed.error().describe());
    {
        std::lock_guard lock(projectMutex_);
        project_ = std::move(opened.value());
    }
    results_.reset();
    return describeProject();
}

json Engine::save(const json& request) {
    requireProject();
    requireIdle();
    std::lock_guard lock(projectMutex_);
    if (request.contains("setup"))
        project_->setSetupJson(caseSetupToJson(parseSetup(request.at("setup"))));
    if (request.contains("name"))
        project_->setName(text(request, "name"));
    if (auto saved = project_->save(); !saved)
        throw RequestError(saved.error().describe());
    return {{"saved", true}};
}

json Engine::validate(const json& request) {
    const CaseSetup setup = parseSetup(request.at("setup"));
    return {{"issues", issuesOf(setup)},
            {"courant", number(setup.estimateMaxCourant())},
            {"reynolds", number(setup.estimateReynolds())}};
}

bool Engine::ensureEnvironment(std::string& error) {
    if (!installation_)
        detect();
    if (!installation_) {
        error = detectionError_;
        return false;
    }
    if (!environment_.empty())
        return true;
    PosixProcessRunner runner;
    auto captured = openfoam::Detector::captureEnvironment(*installation_, runner);
    if (!captured) {
        error = captured.error().describe();
        return false;
    }
    environment_ = std::move(captured.value());
    caps_ = openfoam::Detector::verifyCapabilities(*installation_, environment_);
    return true;
}

json Engine::startJob(const json& request, bool solve) {
    requireProject();
    requireIdle();
    CaseSetup setup;
    std::filesystem::path caseDir;
    {
        std::lock_guard lock(projectMutex_);
        setup = parseSetup(json::parse(project_->setupJson()));
        caseDir = project_->caseDir();
    }
    if (hasErrors(setup))
        throw RequestError("the case setup has errors; validate it first");
    std::string error;
    if (!ensureEnvironment(error))
        throw RequestError("OpenFOAM is not available: " + error);
    auto written = openfoam::CaseWriter::write(caseDir, setup, caps_, {.overwrite = true});
    if (!written)
        throw RequestError(written.error().describe());
    {
        std::lock_guard lock(projectMutex_);
        project_->setBackendFingerprint(installation_->fingerprint());
        project_->setCaseState(Project::CaseState::Configured);
        (void)project_->save();
    }
    const unsigned processes = std::max(1u, request.value("processes", 1u));
    join();
    results_.reset();
    cancel_ = std::make_unique<CancellationToken>();
    running_.store(true);
    worker_ = std::thread([this, solve, processes] { runJob(solve, processes); });
    return {{"started", solve ? "solve" : "mesh"}};
}

void Engine::runJob(bool solve, unsigned processes) {
    const auto finish = [this](const char* phase, bool ok, const std::string& summary) {
        running_.store(false);
        emit_({{"event", "job"}, {"state", "finished"}, {"phase", phase}, {"ok", ok},
               {"summary", summary}});
    };
    try {
        CaseSetup setup;
        std::filesystem::path caseDir;
        {
            std::lock_guard lock(projectMutex_);
            setup = parseSetup(json::parse(project_->setupJson()));
            caseDir = project_->caseDir();
        }
        PosixProcessRunner processRunner;
        openfoam::CaseRunner runner(processRunner, environment_);
        openfoam::RunCallbacks callbacks;
        callbacks.onOutput = [this](OutputStream, std::string_view chunk) {
            emit_({{"event", "log"}, {"text", std::string(chunk)}});
        };
        auto lastProgress = std::chrono::steady_clock::now() - std::chrono::seconds(1);
        callbacks.onProgress = [this, &lastProgress](const openfoam::SolverProgress& p) {
            const auto now = std::chrono::steady_clock::now();
            if (!p.diverged && now - lastProgress < std::chrono::milliseconds(100))
                return;
            lastProgress = now;
            json residuals = json::array();
            for (const auto& r : p.residuals)
                residuals.push_back({{"field", r.field}, {"initial", number(r.initial)},
                                     {"final", number(r.finalResidual)},
                                     {"iterations", r.iterations}});
            emit_({{"event", "progress"}, {"time", number(p.time)},
                   {"courantMean", number(p.courantMean)}, {"courantMax", number(p.courantMax)},
                   {"residuals", residuals}});
        };

        emit_({{"event", "job"}, {"state", "running"}, {"phase", "mesh"}});
        auto meshed = runner.runMeshing(caseDir, caps_, callbacks, cancel_.get());
        if (!meshed)
            return finish("mesh", false, meshed.error().describe());
        if (!meshed->process.succeeded())
            return finish("mesh", false, meshed->process.cancelled
                                             ? "Meshing cancelled."
                                             : meshed->name + " failed (exit " +
                                                   std::to_string(meshed->process.exitCode) + ")");
        {
            std::lock_guard lock(projectMutex_);
            project_->setCaseState(Project::CaseState::Meshed);
            (void)project_->save();
        }

        std::string summary = "Mesh generated.";
        if (solve) {
            emit_({{"event", "job"}, {"state", "running"}, {"phase", "solve"}});
            auto solved = runner.runSolver(caseDir, setup, caps_, callbacks, cancel_.get(), {},
                                           processes);
            if (!solved)
                return finish("solve", false, solved.error().describe());
            if (solved->diverged)
                return finish("solve", false, "Solver stopped: " + solved->divergenceReason);
            if (solved->process.cancelled)
                return finish("solve", false, "Calculation cancelled.");
            if (!solved->process.succeeded())
                return finish("solve", false,
                              "Solver failed (exit " + std::to_string(solved->process.exitCode) + ")");
            std::lock_guard lock(projectMutex_);
            project_->setCaseState(Project::CaseState::Solved);
            (void)project_->save();
            summary = "Finished at t=" + std::to_string(solved->finalTime) + " s.";
        }

        // Results are read through VTK, so the latest time (or the bare mesh) is converted.
        ProcessRequest convert;
        convert.executable = "foamToVTK";
        convert.arguments = solve ? std::vector<std::string>{"-latestTime"}
                                  : std::vector<std::string>{"-latestTime", "-no-fields"};
        convert.workingDirectory = caseDir;
        convert.environment = environment_;
        if (auto resolved = openfoam::Detector::resolveExecutable("foamToVTK", environment_))
            convert.executable = resolved->string();
        auto converted = processRunner.run(convert, cancel_.get(), nullptr);
        if (!converted || !converted->succeeded())
            summary += " The results could not be exported for viewing.";
        finish(solve ? "solve" : "mesh", true, summary);
    } catch (const std::exception& error) {
        finish(solve ? "solve" : "mesh", false, error.what());
    }
}

json Engine::cancel() {
    if (cancel_)
        cancel_->cancel();
    return {{"cancelling", running_.load()}};
}

vtkDataObject* Engine::loadResults() {
    requireProject();
    requireIdle();
    std::string caseDir;
    {
        std::lock_guard lock(projectMutex_);
        caseDir = project_->caseDir().string();
    }
    // A compressible project's results live in its active mesh revision, not in `case`.
    if (const auto stored = readFile(flowPath(), 8 * 1024 * 1024)) {
        const auto active = QJsonDocument::fromJson(QByteArray::fromStdString(*stored)).object().value("activeCase").toString();
        if (!active.isEmpty() && std::filesystem::is_directory(active.toStdString()))
            caseDir = active.toStdString();
    }
    const std::string file = currentVtmFile(QString::fromStdString(caseDir)).toStdString();
    if (file.empty())
        throw RequestError("there are no results to show yet");
    if (results_ && results_->file == file)
        return results_->data;
    vtkNew<vtkXMLMultiBlockDataReader> reader;
    reader->SetFileName(file.c_str());
    reader->Update();
    results_ = std::make_unique<Results>();
    results_->file = file;
    results_->data = reader->GetOutputDataObject(0);
    if (!results_->data)
        throw RequestError("the results file could not be read");
    return results_->data;
}

json Engine::results() {
    vtkDataObject* data = loadResults();
    const auto bounds = fieldBounds(data);
    return {{"file", results_->file},
            {"fields", fieldNames(data)},
            {"bounds", {{bounds[0], bounds[2], bounds[4]}, {bounds[1], bounds[3], bounds[5]}}}};
}

json Engine::statistics(const json& request) {
    vtkDataObject* data = loadResults();
    const auto s = fieldStatistics(data, text(request, "field"), request.value("bins", 24));
    json histogram = json::array();
    for (const auto& bin : s.histogram)
        histogram.push_back({{"low", number(bin.low)}, {"high", number(bin.high)},
                             {"measure", number(bin.measure)}, {"cells", bin.cells}});
    return {{"minimum", number(s.minimum)}, {"maximum", number(s.maximum)},
            {"mean", number(s.mean)}, {"standardDeviation", number(s.standardDeviation)},
            {"integral", number(s.integral)}, {"measure", number(s.measure)},
            {"cells", s.cells}, {"excluded", s.excluded}, {"histogram", histogram}};
}

json Engine::line(const json& request) {
    vtkDataObject* data = loadResults();
    const int points = std::clamp(request.value("points", 100), 2, 5000);
    json samples = json::array();
    for (const auto& sample : sampleFieldLine(data, vec(request.at("start"), "start"),
                                              vec(request.at("end"), "end"), points,
                                              text(request, "field")))
        samples.push_back({{"distance", number(sample.distance)},
                           {"value", sample.valid ? number(sample.value) : json(nullptr)}});
    return {{"samples", samples}};
}

json Engine::slice(const json& request) {
    vtkDataObject* data = loadResults();
    const int axis = request.value("axis", 2);
    if (axis < 0 || axis > 2)
        throw RequestError("\"axis\" is 0, 1 or 2");
    auto cut = sliceField(data, axis, request.at("position").get<double>());
    if (!cut)
        throw RequestError("the plane does not cross the domain");
    return triangles(cut, text(request, "field"));
}

json Engine::surface(const json& request) {
    vtkDataObject* data = loadResults();
    vtkDataSet* fluid = fluidDataSet(data);
    if (!fluid)
        throw RequestError("the results hold no fluid region");
    vtkNew<vtkCellDataToPointData> toPoints;
    toPoints->SetInputData(fluid);
    toPoints->PassCellDataOn();
    vtkNew<vtkDataSetSurfaceFilter> outer;
    outer->SetInputConnection(toPoints->GetOutputPort());
    outer->Update();
    return triangles(outer->GetOutput(), text(request, "field"));
}

// ---------------------------------------------------------------------------
// Compressible flow over a sketched, axisymmetric domain
// ---------------------------------------------------------------------------

namespace {

json toJson(const QJsonObject& object) {
    return json::parse(QJsonDocument(object).toJson(QJsonDocument::Compact).toStdString());
}

QJsonObject toQt(const json& object) {
    return QJsonDocument::fromJson(QByteArray::fromStdString(object.dump())).object();
}

std::optional<std::string> readFile(const std::filesystem::path& path, std::size_t limit) {
    QFile file(QString::fromStdString(path.string()));
    if (!file.open(QIODevice::ReadOnly) || static_cast<std::size_t>(file.size()) > limit)
        return std::nullopt;
    return file.readAll().toStdString();
}

void writeFile(const std::filesystem::path& path, const std::string& bytes) {
    std::filesystem::create_directories(path.parent_path());
    QSaveFile file(QString::fromStdString(path.string()));
    const auto data = QByteArray::fromStdString(bytes);
    if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit())
        throw RequestError("could not write " + path.filename().string());
}

/// What a client draws: points, arcs as bulges, and each segment's length and whether it lies
/// on the axis (and so is the axis boundary).
json describeSketch(const SketchDocument& document) {
    const auto& s = document.sketch;
    json segments = json::array();
    for (std::size_t i = 0; i < s.segmentCount(); ++i) {
        const auto a = s.points()[i], b = s.points()[(i + 1) % s.points().size()];
        segments.push_back({{"length", s.segmentLength(i)}, {"bulge", s.segmentBulge(i)},
                            {"axis", a.yMm == 0 && b.yMm == 0 && s.segmentBulge(i) == 0}});
    }
    json points = json::array();
    for (const auto& p : s.points())
        points.push_back({p.xMm, p.yMm});
    const auto& n = document.nozzle;
    return {{"points", points}, {"closed", s.closed()}, {"segments", segments},
            {"nozzle", {{"inletDiameterMm", n.inletDiameterMm}, {"throatDiameterMm", n.throatDiameterMm},
                        {"outletDiameterMm", n.outletDiameterMm}, {"convergingLengthMm", n.convergingLengthMm},
                        {"divergingLengthMm", n.divergingLengthMm}}}};
}

} // namespace

std::filesystem::path Engine::sketchPath() const {
    std::lock_guard lock(projectMutex_);
    return project_->rootDir() / "geometry" / "working-sketch.vsketch.json";
}

std::filesystem::path Engine::flowPath() const {
    std::lock_guard lock(projectMutex_);
    return project_->rootDir() / "flow-setup.json";
}

json Engine::sketch() {
    requireProject();
    const auto text = readFile(sketchPath(), 4 * 1024 * 1024);
    if (!text)
        return {{"sketch", nullptr}};
    auto document = sketchFromJson(*text);
    if (!document)
        throw RequestError("the project's sketch cannot be read: " + document.error().describe());
    return {{"sketch", describeSketch(*document)}};
}

json Engine::saveSketch(const json& request) {
    requireProject();
    requireIdle();
    // The client sends the file format itself; the same strict reader checks it.
    auto document = sketchFromJson(request.at("sketch").dump());
    if (!document)
        throw RequestError(document.error().describe());
    // Boundaries follow the segments they were assigned to when the contour changes.
    if (const auto before = readFile(sketchPath(), 4 * 1024 * 1024)) {
        const auto old = sketchFromJson(*before);
        const auto saved = readFile(flowPath(), 8 * 1024 * 1024);
        if (old && saved) {
            const auto config = QJsonDocument::fromJson(QByteArray::fromStdString(*saved)).object();
            const auto moved = reconcileFlowBoundaries(old->sketch, document->sketch, config);
            if (!moved.isEmpty())
                writeFile(flowPath(), QJsonDocument(moved).toJson().toStdString());
        }
    }
    writeFile(sketchPath(), sketchToJson(*document));
    return {{"sketch", describeSketch(*document)}};
}

json Engine::nozzle(const json& request) {
    requireProject();
    requireIdle();
    const auto& d = request.at("dimensions");
    const NozzleDimensions dimensions{d.at("inletDiameterMm").get<double>(), d.at("throatDiameterMm").get<double>(),
                                      d.at("outletDiameterMm").get<double>(), d.at("convergingLengthMm").get<double>(),
                                      d.at("divergingLengthMm").get<double>()};
    auto profile = makeNozzleProfile(dimensions);
    if (!profile)
        throw RequestError("these dimensions do not describe a nozzle (the throat must be the narrowest)");
    SketchDocument document{*profile, dimensions};
    writeFile(sketchPath(), sketchToJson(document));
    return {{"sketch", describeSketch(document)}};
}

json Engine::flow() {
    requireProject();
    const auto text = readFile(sketchPath(), 4 * 1024 * 1024);
    if (!text)
        throw RequestError("draw or choose a fluid contour first");
    auto document = sketchFromJson(*text);
    if (!document)
        throw RequestError(document.error().describe());
    QJsonObject saved;
    if (const auto stored = readFile(flowPath(), 8 * 1024 * 1024))
        saved = QJsonDocument::fromJson(QByteArray::fromStdString(*stored)).object();
    return {{"flow", toJson(flowConfiguration(document->sketch, saved))}};
}

json Engine::saveFlow(const json& request) {
    requireProject();
    requireIdle();
    const auto text = readFile(sketchPath(), 4 * 1024 * 1024);
    auto document = text ? sketchFromJson(*text) : Expected<SketchDocument>(Error::make(ErrorCode::NotFound, "no sketch"));
    if (!document)
        throw RequestError("draw or choose a fluid contour first");
    auto config = normalizedFlowConfiguration(document->sketch, toQt(request.at("flow")));
    if (config.isEmpty())
        throw RequestError("the flow setup does not match the contour");
    writeFile(flowPath(), QJsonDocument(config).toJson().toStdString());
    return {{"flow", toJson(config)}};
}

json Engine::startFlow(const json& request) {
    requireProject();
    requireIdle();
    std::string error;
    if (!ensureEnvironment(error))
        throw RequestError("OpenFOAM is not available: " + error);
    std::string version;
    for (const auto& [key, value] : environment_)
        if (key == "WM_PROJECT_VERSION") version = value;
    if (version != "v2606" && version != "2606")
        throw RequestError("compressible flow needs OpenFOAM v2606");
    const bool meshOnly = request.value("meshOnly", false);
    join();
    results_.reset();
    cancel_ = std::make_unique<CancellationToken>();
    running_.store(true);
    worker_ = std::thread([this, meshOnly] { runFlowJob(meshOnly); });
    return {{"started", meshOnly ? "flowMesh" : "flowSolve"}};
}

void Engine::runFlowJob(bool meshOnly) {
    const char* phase = meshOnly ? "mesh" : "solve";
    const auto finish = [this, phase](bool ok, const std::string& summary) {
        running_.store(false);
        emit_({{"event", "job"}, {"state", "finished"}, {"phase", phase}, {"ok", ok}, {"summary", summary}});
    };
    try {
        emit_({{"event", "job"}, {"state", "running"}, {"phase", "mesh"}});
        const auto text = readFile(sketchPath(), 4 * 1024 * 1024);
        auto document = text ? sketchFromJson(*text) : Expected<SketchDocument>(Error::make(ErrorCode::NotFound, "no sketch"));
        if (!document || !document->sketch.closed())
            return finish(false, "Close the fluid contour before generating its mesh.");
        QJsonObject saved;
        if (const auto stored = readFile(flowPath(), 8 * 1024 * 1024))
            saved = QJsonDocument::fromJson(QByteArray::fromStdString(*stored)).object();
        auto config = normalizedFlowConfiguration(document->sketch, saved.isEmpty() ? flowConfiguration(document->sketch) : saved);
        bool inlet = false, outlet = false;
        for (const auto& value : config.value("boundaries").toArray()) {
            const auto kind = value.toObject().value("type").toString();
            if (kind == "unassigned" || kind.isEmpty())
                return finish(false, "Assign every boundary, including a total-pressure inlet and a pressure outlet.");
            inlet = inlet || kind == "inlet";
            outlet = outlet || kind == "outlet";
        }
        if (!inlet || !outlet)
            return finish(false, "Assign every boundary, including a total-pressure inlet and a pressure outlet.");

        std::string root;
        {
            std::lock_guard lock(projectMutex_);
            root = project_->rootDir().string();
        }
        auto prepared = prepareFlowCase(QString::fromStdString(sketchPath().string()), config, QString::fromStdString(root),
                                        environment_, {}, [this] { return cancel_->isCancelled(); });
        if (!prepared.diagnostic.isEmpty())
            emit_({{"event", "log"}, {"text", prepared.diagnostic.right(30000).toStdString()}});
        if (prepared.caseDir.isEmpty())
            return finish(false, prepared.error.toStdString());
        const std::filesystem::path caseDir = prepared.caseDir.toStdString();
        writeFile(caseDir / ".vortia-engine", installation_->fingerprint() + "\n");
        config.insert("activeCase", prepared.caseDir);
        config.insert("preparedEngine", QString::fromStdString(installation_->fingerprint()));
        writeFile(flowPath(), QJsonDocument(config).toJson().toStdString());
        emit_({{"event", "log"}, {"text", "New mesh revision: " + caseDir.string() + "\n"}});

        PosixProcessRunner processRunner;
        auto environment = environment_;
        for (auto& entry : flowSolverRuntime(environment_, installation_->fingerprint()))
            environment.push_back(std::move(entry));
        openfoam::CaseRunner runner(processRunner, environment);
        openfoam::RunCallbacks callbacks;
        callbacks.onOutput = [this](OutputStream, std::string_view chunk) {
            emit_({{"event", "log"}, {"text", std::string(chunk)}});
        };
        auto last = std::chrono::steady_clock::now() - std::chrono::seconds(1);
        callbacks.onProgress = [this, &last](const openfoam::SolverProgress& p) {
            const auto now = std::chrono::steady_clock::now();
            if (!p.diverged && now - last < std::chrono::milliseconds(100))
                return;
            last = now;
            json residuals = json::array();
            for (const auto& r : p.residuals)
                residuals.push_back({{"field", r.field}, {"initial", number(r.initial)},
                                     {"final", number(r.finalResidual)}, {"iterations", r.iterations}});
            json event{{"event", "progress"}, {"time", number(p.time)}, {"courantMean", number(p.courantMean)},
                       {"courantMax", number(p.courantMax)}, {"residuals", residuals}};
            if (p.convergence) {
                const auto& c = *p.convergence;
                event["convergence"] = {{"status", c.status}, {"iteration", number(c.iteration)},
                                        {"globalMass", number(c.globalMass)}, {"localMass", number(c.localMass)},
                                        {"fieldChange", number(c.fieldChange)}, {"equationError", number(c.equationError)},
                                        {"stableSamples", c.stableSamples}, {"requiredSamples", c.requiredSamples}};
            }
            emit_(event);
        };
        auto meshed = runner.runMeshing(caseDir, caps_, callbacks, cancel_.get());
        if (!meshed)
            return finish(false, meshed.error().describe());
        if (!meshed->process.succeeded())
            return finish(false, meshed->process.cancelled ? "Meshing cancelled." : meshed->name + " failed");

        std::string summary = "Mesh generated.";
        if (!meshOnly) {
            if (const auto staged = stageFlowBackend(prepared.caseDir); !staged.isEmpty())
                return finish(false, staged.toStdString());
            emit_({{"event", "job"}, {"state", "running"}, {"phase", "solve"}});
            int selected = config.value("solver").toObject().value("processes").toInt(1);
            unsigned processes = static_cast<unsigned>(std::max(selected, 1));
            if (selected == 0) {
                double cells = 0;
                if (const auto report = readFile(caseDir / "mesh-report.json", 65536))
                    cells = QJsonDocument::fromJson(QByteArray::fromStdString(*report)).object().value("cells").toDouble();
                const auto count = std::isfinite(cells) && cells > 0 && cells <= 1000000 ? static_cast<std::uint64_t>(cells) : 0;
                processes = recommendedFlowProcesses(count, static_cast<unsigned>(std::max(1, QThread::idealThreadCount())));
            }
            CaseSetup setup = cavity();
            auto solved = runner.runSolver(caseDir, setup, caps_, callbacks, cancel_.get(), "vortiaCentralFoam", processes);
            if (!solved)
                return finish(false, solved.error().describe());
            if (solved->diverged)
                return finish(false, "Solver stopped: " + solved->divergenceReason);
            if (solved->process.cancelled)
                return finish(false, "Calculation cancelled. Resume from the latest saved checkpoint.");
            if (!solved->process.succeeded())
                return finish(false, "Solver failed (exit " + std::to_string(solved->process.exitCode) + ")");
            summary = "Calculation finished at iteration " + std::to_string(static_cast<long long>(solved->finalTime)) + ".";
        }
        ProcessRequest convert;
        convert.executable = "foamToVTK";
        convert.arguments = meshOnly ? std::vector<std::string>{"-latestTime", "-no-fields"}
                                     : std::vector<std::string>{"-latestTime"};
        convert.workingDirectory = caseDir;
        convert.environment = environment_;
        if (auto resolved = openfoam::Detector::resolveExecutable("foamToVTK", environment_))
            convert.executable = resolved->string();
        auto converted = processRunner.run(convert, cancel_.get(), nullptr);
        if (!converted || !converted->succeeded())
            summary += " The results could not be exported for viewing.";
        finish(true, summary);
    } catch (const std::exception& error) {
        finish(false, error.what());
    }
}

} // namespace vortia::engine
