#pragma once

#include "vortia/core/Logger.hpp"

#include <QPlainTextEdit>
#include <QTimer>

namespace vortia {

/// Bottom console: mirrors the application Logger and solver stdout/stderr.
/// Logger callbacks may arrive from any thread; they are marshalled to the
/// GUI thread via queued invocation.
class ConsolePanel : public QPlainTextEdit {
    Q_OBJECT

public:
    explicit ConsolePanel(QWidget* parent = nullptr);
    ~ConsolePanel() override;

    void appendSolverOutput(const QString& text);
    void clearConsole();
    void flushOutput();

private:
    void appendLine(const QString& html);

    std::size_t loggerSinkId_{0};
    QString pendingOutput_;
    QTimer outputTimer_;
};

} // namespace vortia
