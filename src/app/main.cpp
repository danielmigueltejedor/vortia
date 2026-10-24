#include "vortia/ui/MainWindow.hpp"
#include "vortia/ui/ThemeManager.hpp"

#include <QApplication>
#include <QIcon>
#include <QOpenGLContext>
#include <QSurfaceFormat>
#include <QLocale>
#include <QSettings>
#include <QTranslator>
#include <QDebug>
#include <QCoreApplication>

int main(int argc, char* argv[]) {
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

    QSurfaceFormat format;
    format.setDepthBufferSize(24);
    format.setStencilBufferSize(8);
    format.setVersion(3, 2);
    format.setProfile(QSurfaceFormat::CoreProfile);
    QSurfaceFormat::setDefaultFormat(format);

    QApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("Vortia"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationName(QStringLiteral("Vortia"));
    app.setOrganizationDomain(QStringLiteral("vortia.dev"));
    // Embedded icon is available even when running the unbundled dev binary.
    const QIcon vortiaIcon(QStringLiteral(":/vortia/app-icon.png"));
    app.setWindowIcon(vortiaIcon);

    QSettings settings(QStringLiteral("Vortia"), QStringLiteral("Vortia"));
    const QString theme = settings.value(QStringLiteral("theme"), QStringLiteral("dark")).toString();
    vortia::ThemeManager::apply(app, vortia::ThemeManager::fromName(theme));

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
    window.setWindowIcon(vortiaIcon);
    window.show();

    return app.exec();
}
