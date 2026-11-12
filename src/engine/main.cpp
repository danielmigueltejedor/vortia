// vortia-engine: Vortia's case pipeline behind newline-delimited JSON on stdin/stdout, for
// interfaces that are not the Qt one (Vortia Native on macOS). The protocol is described in
// docs/architecture/native-engine.md.
//
// stdout carries protocol only: one JSON object per line. Human logs go to stderr.

#include "Engine.hpp"

#include "vortia/ui/OpenFoamRuntime.hpp"

#include <QCoreApplication>

#include <iostream>
#include <mutex>
#include <string>

int main(int argc, char** argv) {
    // Only for applicationDirPath(), which finds the OpenFOAM packaged beside the engine.
    QCoreApplication app(argc, argv);
    // The same identity as the desktop app, so both share the compiled flow solvers' cache.
    QCoreApplication::setApplicationName(QStringLiteral("Vortia"));
    QCoreApplication::setOrganizationName(QStringLiteral("Vortia"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("vortia.dev"));
    std::ios::sync_with_stdio(false);

    std::mutex out;
    const auto write = [&out](const nlohmann::json& message) {
        const std::string line = message.dump(-1, ' ', false, nlohmann::json::error_handler_t::replace);
        std::lock_guard lock(out);
        std::cout << line << '\n' << std::flush;
    };

    vortia::engine::Engine engine(write, vortia::bundledOpenFoamRoot());
    write({{"event", "ready"}, {"protocol", vortia::engine::kProtocolVersion}});

    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.find_first_not_of(" \t\r") == std::string::npos)
            continue;
        const auto request = nlohmann::json::parse(line, nullptr, false);
        if (request.is_discarded()) {
            write({{"id", nullptr}, {"ok", false}, {"error", "a line that is not JSON"}});
            continue;
        }
        if (request.value("cmd", "") == "quit") {
            write({{"id", request.value("id", nlohmann::json(nullptr))}, {"ok", true},
                   {"result", nlohmann::json::object()}});
            break;
        }
        write(engine.handle(request));
    }
    // The client went away: stop whatever runs rather than leave OpenFOAM orphaned.
    (void)engine.handle({{"cmd", "cancel"}});
    engine.join();
    return 0;
}
