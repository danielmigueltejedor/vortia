#include "vortia/ui/MainWindow.hpp"
#include "vortia/ui/ThemeManager.hpp"
#include "vortia/ui/OpenFoamRuntime.hpp"
#include "vortia/ui/MesherRuntime.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/solver/openfoam/EngineSelfTest.hpp"
#include "vortia/core/HostEnvironment.hpp"

#include <QApplication>
#include <QGuiApplication>
#include <QWidget>
#include <QIcon>
#include <QFileInfo>
#include <QOpenGLContext>
#include <QSurfaceFormat>
#include <QLocale>
#include <QSettings>
#include <QTranslator>
#include <QDebug>
#include <QCoreApplication>
#include <QAbstractButton>
#include <QMessageBox>
#include <QOffscreenSurface>
#include <QProcess>
#include <QTemporaryDir>
#include <QTimer>

#include <vector>
#ifdef Q_OS_LINUX
#include <unistd.h>
#endif

namespace {

#ifdef Q_OS_LINUX
// The viewport needs an OpenGL 3.2 core context. Probe it before building the
// main window so a broken or missing GPU driver is reported, not a crash.
bool openGlCoreContextAvailable() {
    QOffscreenSurface surface;
    surface.setFormat(QSurfaceFormat::defaultFormat());
    surface.create();
    QOpenGLContext context;
    context.setFormat(QSurfaceFormat::defaultFormat());
    if (!surface.isValid() || !context.create() || !context.makeCurrent(&surface))
        return false;
    const QSurfaceFormat actual = context.format();
    const bool sufficient = actual.majorVersion() > 3
        || (actual.majorVersion() == 3 && actual.minorVersion() >= 2);
    context.doneCurrent();
    return sufficient;
}

// Start Vortia again with Mesa's software renderer. Returns only on failure.
void restartWithSoftwareRendering(int argc, char* argv[]) {
    qputenv("VORTIA_GL_FALLBACK", "1");
    qputenv("LIBGL_ALWAYS_SOFTWARE", "1");
    // Inside the AppImage the launcher must run again: the host environment
    // has already been restored, so the bundled library paths are gone.
    const QByteArray appDir = qgetenv("APPDIR");
    const QByteArray program = appDir.isEmpty()
        ? QCoreApplication::applicationFilePath().toLocal8Bit()
        : appDir + "/AppRun";
    std::vector<char*> arguments;
    QByteArray programName = program;
    arguments.push_back(programName.data());
    for (int i = 1; i < argc; ++i)
        arguments.push_back(argv[i]);
    arguments.push_back(nullptr);
    ::execv(program.constData(), arguments.data());
}
#endif

} // namespace

int main(int argc, char* argv[]) {
    // The dynamic loader read LD_LIBRARY_PATH when this process started;
    // restoring the user's value now only affects the programs Vortia starts
    // (OpenFOAM, python3, gmsh), which must not load our bundled libraries.
    const vortia::EnvironmentAccess hostEnvironment = vortia::processEnvironment();
    vortia::restoreHostVariable(hostEnvironment, "LD_LIBRARY_PATH");

    const bool verifyStartup = argc > 1
        && QString::fromUtf8(argv[1]) == QStringLiteral("--verify-startup");

    // Check the packaged engine without opening a project or starting a solve.
    if (argc > 1 && QString::fromUtf8(argv[1]) == QStringLiteral("--verify-engine")) {
        QCoreApplication checkApp(argc,argv);
        vortia::PosixProcessRunner runner;
        if(!std::filesystem::is_directory(vortia::bundledOpenFoamRoot())) {
            qCritical()<<"This build has no integrated OpenFOAM engine";return 3;
        }
        const auto found=vortia::openfoam::Detector::discover(runner,{},vortia::bundledOpenFoamRoot());
        if(!found || found->empty()){qCritical()<<"Integrated engine unavailable";return 3;}
        const auto installed=vortia::openfoam::Detector::probe(found->front().rootDir);
        if(!installed){qCritical()<<QString::fromStdString(installed.error().describe());return 3;}
        const auto env=vortia::openfoam::Detector::captureEnvironment(*installed,runner);
        if(!env){qCritical()<<QString::fromStdString(env.error().describe());return 3;}
        for(const auto* name:{"blockMesh","checkMesh","rhoSimpleFoam","foamToVTK"}) {
            const auto exe=vortia::openfoam::Detector::resolveExecutable(name,*env);
            if(!exe){qCritical()<<QString::fromStdString(exe.error().describe());return 3;}
            std::error_code ec;
            const auto relative=std::filesystem::relative(*exe,installed->rootDir,ec);
            if(ec || relative.empty() || *relative.begin()==".."){
                qCritical()<<"Engine tool resolves outside packaged runtime:"<<name;return 3;
            }
            vortia::ProcessRequest request;request.executable=*exe;request.arguments={"-help"};
            request.environment=*env;request.timeout=std::chrono::seconds{15};
            const auto result=runner.run(request);
            if(!result || !result->succeeded()){qCritical()<<"Packaged engine tool failed:"<<name;return 3;}
            qInfo()<<"Verified integrated tool:"<<QString::fromStdString(exe->string());
        }
        // The tools answering -help is not enough: run a small real case,
        // serial and on two MPI processes, with the packaged engine.
        QTemporaryDir work;
        if (!work.isValid()) { qCritical() << "Cannot create a temporary directory"; return 3; }
        const auto selfTest = vortia::openfoam::runEngineSelfTest(
            *installed, runner, std::filesystem::path(work.path().toStdString()));
        if (!selfTest) {
            qCritical().noquote() << "Integrated engine self-test failed:"
                                  << QString::fromStdString(selfTest.error().describe());
            return 3;
        }
        for (const auto& line : selfTest->steps)
            qInfo().noquote() << "Engine self-test," << QString::fromStdString(line);
        qInfo() << "Integrated OpenFOAM engine verified with a real serial and parallel run";
        return 0;
    }
    // Packaged mesher: the bundled Python must load the bundled Gmsh and
    // produce a real mesh, with nothing installed on the machine.
    if (argc > 1 && QString::fromUtf8(argv[1]) == QStringLiteral("--verify-mesher")) {
        QCoreApplication checkApp(argc, argv);
        const QString python = vortia::mesherPython();
        const QString runtime = vortia::mesherGmshRuntime();
        if (python.isEmpty() || !QFileInfo::exists(runtime + QStringLiteral("/gmsh.py"))) {
            qCritical() << "No Python 3 or no Gmsh runtime for the mesher:" << python << runtime;
            return 7;
        }
        const QString script = QStringLiteral(
            "import sys\n"
            "sys.path.insert(0, sys.argv[1])\n"
            "import gmsh\n"
            "gmsh.initialize()\n"
            "gmsh.option.setNumber('General.Terminal', 0)\n"
            "gmsh.model.occ.addRectangle(0, 0, 0, 1, 1)\n"
            "gmsh.model.occ.synchronize()\n"
            "gmsh.model.mesh.generate(2)\n"
            "nodes = len(gmsh.model.mesh.getNodes()[0])\n"
            "version = gmsh.option.getString('General.Version')\n"
            "gmsh.finalize()\n"
            "print('gmsh', version, 'python', sys.version.split()[0], 'nodes', nodes)\n"
            "sys.exit(0 if nodes > 4 else 1)\n");
        QProcess mesher;
        QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
        vortia::applyMesherEnvironment(environment);
        mesher.setProcessEnvironment(environment);
        mesher.setProcessChannelMode(QProcess::MergedChannels);
        mesher.start(python, {QStringLiteral("-c"), script, runtime});
        const bool finished = mesher.waitForStarted(10'000) && mesher.waitForFinished(120'000);
        const QString output = QString::fromLocal8Bit(mesher.readAll()).trimmed();
        if (!finished || mesher.exitStatus() != QProcess::NormalExit || mesher.exitCode() != 0) {
            qCritical().noquote() << "Packaged mesher failed:" << python << output;
            return 7;
        }
        qInfo().noquote() << "Vortia mesher verified:" << output << "using" << python;
        return 0;
    }

    // Deterministic, headless verification for CI. This catches missing .qm
    // compilation AND incorrect resource paths instead of silently falling
    // back to English on shipped builds.
    if (argc > 1 && QString::fromUtf8(argv[1]) == QStringLiteral("--verify-spanish")) {
        QCoreApplication checkApp(argc, argv);
        QTranslator catalog;
        const bool loaded = catalog.load(QStringLiteral(":/translations/vortia_es.qm"));
        checkApp.installTranslator(&catalog);
        if (!loaded || vortia::MainWindow::tr("File") != QStringLiteral("Archivo")
            || vortia::ProjectExplorerPanel::tr("Geometry") != QStringLiteral("Geometría")
            || vortia::SketchEditorDialog::tr("Nozzle dimensions") != QStringLiteral("Cotas de la tobera")) {
            qCritical() << "Vortia Spanish catalog missing or invalid;"
                        << "loaded:" << loaded;
            return 2;
        }
        qInfo() << "Vortia Spanish Qt resources verified";
        return 0;
    }

    // Exercise the actual QPA plugin and display connection in release CI.
    // --verify-spanish uses QCoreApplication and cannot detect a missing
    // platform plugin: that allowed broken AppImages to be published.
    if (argc > 1 && QString::fromUtf8(argv[1]) == QStringLiteral("--verify-qt-platform")) {
        QApplication smokeApp(argc, argv);
        QWidget probe;
        probe.resize(32, 32);
        probe.show();
        smokeApp.processEvents();
        if (QGuiApplication::platformName().isEmpty() || !probe.isVisible()) {
            qCritical() << "Vortia Qt platform/window verification failed";
            return 4;
        }
        qInfo() << "Vortia graphical Qt platform verified:"
                << QGuiApplication::platformName();
        return 0;
    }

    QSurfaceFormat format;
    format.setDepthBufferSize(24);
    format.setStencilBufferSize(8);
    format.setVersion(3, 2);
    format.setProfile(QSurfaceFormat::CoreProfile);
    QSurfaceFormat::setDefaultFormat(format);

    QApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("Vortia"));
#ifndef VORTIA_VERSION
#define VORTIA_VERSION "0.4.0"
#endif
    app.setApplicationVersion(QStringLiteral(VORTIA_VERSION));
    app.setOrganizationName(QStringLiteral("Vortia"));
    app.setOrganizationDomain(QStringLiteral("vortia.dev"));

    // Qt has resolved its plugin directories by now; child processes get the
    // user's own Qt settings back.
    (void)QCoreApplication::libraryPaths();
    for (const char* name : {"QT_PLUGIN_PATH", "QT_QPA_PLATFORM_PLUGIN_PATH", "QT_QPA_PLATFORM"})
        vortia::restoreHostVariable(hostEnvironment, name);

#ifdef Q_OS_LINUX
    if (!openGlCoreContextAvailable()) {
        if (!qEnvironmentVariableIsSet("VORTIA_GL_FALLBACK")) {
            qWarning() << "Vortia: no OpenGL 3.2 context from the graphics driver;"
                       << "retrying with software rendering";
            restartWithSoftwareRendering(argc, argv);
        }
        const bool spanish = QLocale::system().language() == QLocale::Spanish;
        const QString message = spanish
            ? QStringLiteral("Vortia necesita OpenGL 3.2 y este equipo no lo ofrece, ni con el "
                             "controlador gráfico ni con el renderizado por software de Mesa.\n\n"
                             "Instala o actualiza el controlador de tu tarjeta gráfica (o el paquete "
                             "Mesa de tu distribución) y vuelve a abrir Vortia.")
            : QStringLiteral("Vortia needs OpenGL 3.2 and this computer does not provide it, neither "
                             "through the graphics driver nor through Mesa software rendering.\n\n"
                             "Install or update your graphics driver (or your distribution's Mesa "
                             "package) and open Vortia again.");
        qCritical().noquote() << message;
        if (!verifyStartup)
            QMessageBox::critical(nullptr, QStringLiteral("Vortia"), message);
        return 5;
    }
#endif
    // Embedded icon is available even when running the unbundled dev binary.
    const QIcon vortiaIcon(QStringLiteral(":/vortia/app-icon.png"));
    bool nativeBundleIcon=false;
#ifdef Q_OS_MACOS
    // Let AppKit use the prepared ICNS instead of replacing the Dock icon with
    // the transparent workspace-logo PNG and reintroducing a white plate.
    nativeBundleIcon=QFileInfo::exists(QCoreApplication::applicationDirPath()+QStringLiteral("/../Resources/Vortia.icns"));
#endif
    if(!nativeBundleIcon)app.setWindowIcon(vortiaIcon);

    QSettings settings(QStringLiteral("Vortia"), QStringLiteral("Vortia"));
    const QString theme = settings.value(QStringLiteral("theme"), QStringLiteral("light")).toString();
    vortia::ThemeManager::apply(app, vortia::ThemeManager::resolve(theme));

    // Explicit preference takes precedence over the system locale. English
    // uses the source strings; Spanish comes from a compiled, bundled .qm.
    const QString preferred = settings.value(QStringLiteral("language"),
                                            QStringLiteral("system")).toString();
    const QString locale = preferred == QStringLiteral("system")
        ? QLocale::system().name().left(2) : preferred;
    QTranslator translator;
    if (locale == QStringLiteral("es")) {
        const QString resource = QStringLiteral(":/translations/vortia_es.qm");
        const QString adjacent = QCoreApplication::applicationDirPath()
            + QStringLiteral("/../vortia_es.qm");
        const QString bundled = QCoreApplication::applicationDirPath()
            + QStringLiteral("/../Resources/vortia_es.qm");
        if (translator.load(resource) || translator.load(bundled)
            || translator.load(adjacent)) {
            app.installTranslator(&translator);
            qInfo() << "Vortia locale: es (catalog loaded)";
        } else {
            qWarning() << "Vortia Spanish translation could not load from"
                       << resource << bundled << adjacent;
        }
    }

    vortia::MainWindow window;
    if(!nativeBundleIcon)window.setWindowIcon(vortiaIcon);
    window.show();

    // Design review aid: `--screenshot <file.png>` saves the main window as
    // drawn by Qt (the 3D viewport may be blank) and exits.
    if (argc > 2 && QString::fromUtf8(argv[1]) == QStringLiteral("--screenshot")) {
        const QString target = QString::fromUtf8(argv[2]);
        // VORTIA_SCREENSHOT_STAGE=0..5 selects the workflow stage to capture.
        if (qEnvironmentVariableIsSet("VORTIA_SCREENSHOT_STAGE")) {
            QTimer::singleShot(1200, &app, [&window] {
                const QString name = QStringLiteral("vortia-workflow-stage-")
                    + QString::number(qEnvironmentVariableIntValue("VORTIA_SCREENSHOT_STAGE"));
                if (auto* stage = window.findChild<QAbstractButton*>(name))
                    stage->click();
            });
        }
        QTimer::singleShot(2500, &app, [&app, &window, target] {
            const bool saved = window.grab().save(target, "PNG");
            if (!saved)
                qCritical() << "Could not save the screenshot to" << target;
            app.exit(saved ? 0 : 8);
        });
    }

    // Release smoke test: the real start-up path (OpenGL context, main window,
    // VTK viewport) on a clean machine, then a clean exit instead of a wait.
    if (verifyStartup) {
        QTimer::singleShot(4000, &app, [&app, &window] {
            const bool visible = window.isVisible();
            if (visible)
                qInfo() << "Vortia startup verified on platform" << QGuiApplication::platformName();
            else
                qCritical() << "Vortia main window is not visible";
            app.exit(visible ? 0 : 6);
        });
    }

    return app.exec();
}
