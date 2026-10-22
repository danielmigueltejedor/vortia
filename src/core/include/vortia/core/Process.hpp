#pragma once

#include "vortia/core/Expected.hpp"

#include <atomic>
#include <chrono>
#include <filesystem>
#include <functional>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace vortia {

/// Cooperative cancellation token shared with running processes.
class CancellationToken {
public:
    void cancel() { cancelled_.store(true, std::memory_order_relaxed); }
    [[nodiscard]] bool isCancelled() const { return cancelled_.load(std::memory_order_relaxed); }

private:
    std::atomic<bool> cancelled_{false};
};

/// Fully-structured process invocation. NEVER goes through a shell:
/// executable + argv are passed verbatim to the OS, so user data cannot
/// inject commands.
struct ProcessRequest {
    std::filesystem::path executable;                       ///< Absolute path or PATH-resolved.
    std::vector<std::string> arguments;                     ///< argv[1..], no shell expansion.
    std::filesystem::path workingDirectory;                 ///< Must exist; empty = current dir.
    std::vector<std::pair<std::string, std::string>> environment; ///< Overlay on parent env.
    std::optional<std::chrono::milliseconds> timeout;       ///< Kill after this wall time.
};

struct ProcessResult {
    bool started{false};        ///< False when the OS refused to launch the process.
    int exitCode{-1};           ///< Valid only when started && !timedOut && !cancelled.
    bool timedOut{false};
    bool cancelled{false};
    std::chrono::milliseconds duration{0};
    std::string stdOut;
    std::string stdErr;

    [[nodiscard]] bool succeeded() const {
        return started && !timedOut && !cancelled && exitCode == 0;
    }
};

/// Incremental output stream (stdout or stderr).
enum class OutputStream { StdOut, StdErr };
using OutputCallback = std::function<void(OutputStream, std::string_view)>;

/// Process execution service. Implementations must guarantee:
/// - no shell interpretation of arguments;
/// - bounded memory while capturing output;
/// - reliable termination of the child (and its children where possible)
///   on timeout and cancellation.
class IProcessRunner {
public:
    virtual ~IProcessRunner() = default;
    virtual Expected<ProcessResult> run(const ProcessRequest& request,
                                        CancellationToken* token = nullptr,
                                        OutputCallback onOutput = nullptr) = 0;
};

/// POSIX implementation (Linux/macOS) based on posix_spawn + poll.
/// Windows is served by a dedicated runner (WSL2 backend) — pending.
class PosixProcessRunner final : public IProcessRunner {
public:
    Expected<ProcessResult> run(const ProcessRequest& request,
                                CancellationToken* token = nullptr,
                                OutputCallback onOutput = nullptr) override;
};

} // namespace vortia
