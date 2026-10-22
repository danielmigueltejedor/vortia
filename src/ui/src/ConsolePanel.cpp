#include "vortia/ui/ConsolePanel.hpp"

#include <QDateTime>
#include <QScrollBar>

namespace vortia {

ConsolePanel::ConsolePanel(QWidget* parent) : QPlainTextEdit(parent) {
    setReadOnly(true);
    setMaximumBlockCount(20'000); // bound memory on long runs
    setPlaceholderText(tr("Vortia console — application and solver output appears here."));

    loggerSinkId_ = Logger::instance().addSink([this](LogLevel level, std::string_view msg) {
        const QString text = QString::fromUtf8(msg.data(), static_cast<qsizetype>(msg.size()));
        const QString prefix = QString::fromUtf8(toString(level).data(),
                                                 static_cast<qsizetype>(toString(level).size()));
        // Marshal to the GUI thread regardless of the emitter's thread.
        QMetaObject::invokeMethod(
            this, [this, prefix, text] { appendLine("[" + prefix + "] " + text); },
            Qt::QueuedConnection);
    });
}

ConsolePanel::~ConsolePanel() {
    Logger::instance().removeSink(loggerSinkId_);
}

void ConsolePanel::appendSolverOutput(const QString& text) {
    // Solver output arrives already on the GUI thread (queued from the job).
    moveCursor(QTextCursor::End);
    insertPlainText(text);
    ensureCursorVisible();
}

void ConsolePanel::clearConsole() {
    clear();
}

void ConsolePanel::appendLine(const QString& html) {
    const QString stamp = QDateTime::currentDateTime().toString(QStringLiteral("HH:mm:ss"));
    appendPlainText(stamp + " " + html);
    QScrollBar* bar = verticalScrollBar();
    bar->setValue(bar->maximum());
}

} // namespace vortia
