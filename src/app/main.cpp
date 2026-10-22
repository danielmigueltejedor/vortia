#include "vortia/ui/MainWindow.hpp"
#include "vortia/ui/ThemeManager.hpp"

#include <QApplication>
#include <QOpenGLContext>
#include <QSurfaceFormat>
#include <QLocale>
#include <QSettings>
#include <QTranslator>

int main(int argc, char* argv[]) {
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

    QSettings settings(QStringLiteral("Vortia"), QStringLiteral("Vortia"));
    const QString theme = settings.value(QStringLiteral("theme"), QStringLiteral("dark")).toString();
    vortia::ThemeManager::apply(app, vortia::ThemeManager::fromName(theme));

    // i18n: try to load a translation matching the system locale.
    QTranslator translator;
    const QString locale = QLocale::system().name(); // e.g. es_ES
    if (translator.load(QStringLiteral(":/translations/vortia_") + locale.left(2))) {
        app.installTranslator(&translator);
    }

    vortia::MainWindow window;
    window.show();

    return app.exec();
}
