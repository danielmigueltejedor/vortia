#include "vortia/core/Process.hpp"

#include <cstring>

#ifndef _WIN32
#include <cerrno>
#include <csignal>
#include <fcntl.h>
#include <poll.h>
#include <spawn.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

extern char** environ; // NOLINT
#endif

namespace vortia {

namespace {

/// Rejects arguments that cannot survive an exec call unambiguously.
Expected<void> validateRequest(const ProcessRequest& request) {
    if (request.executable.empty())
        return Error::make(ErrorCode::InvalidArgument, "process executable is empty");
    if (!request.workingDirectory.empty() &&
        !std::filesystem::is_directory(request.workingDirectory))
        return Error::make(ErrorCode::NotFound, "working directory does not exist",
                           request.workingDirectory.string());
    auto containsNull = [](const std::string& s) { return s.find('\0') != std::string::npos; };
    if (containsNull(request.executable.string()))
        return Error::make(ErrorCode::InvalidArgument, "executable contains NUL byte");
    for (const auto& arg : request.arguments)
        if (containsNull(arg))
            return Error::make(ErrorCode::InvalidArgument, "argument contains NUL byte");
    return {};
}

} // namespace

#ifndef _WIN32

Expected<ProcessResult> PosixProcessRunner::run(const ProcessRequest& request,
                                                CancellationToken* token,
                                                OutputCallback onOutput) {
    if (auto valid = validateRequest(request); !valid)
        return valid.error();

    int stdoutPipe[2] = {-1, -1};
    int stderrPipe[2] = {-1, -1};
    if (::pipe(stdoutPipe) != 0 || ::pipe(stderrPipe) != 0)
        return Error::make(ErrorCode::Internal, "pipe() failed", std::strerror(errno));

    posix_spawn_file_actions_t actions;
    ::posix_spawn_file_actions_init(&actions);
    ::posix_spawn_file_actions_adddup2(&actions, stdoutPipe[1], STDOUT_FILENO);
    ::posix_spawn_file_actions_adddup2(&actions, stderrPipe[1], STDERR_FILENO);
    ::posix_spawn_file_actions_addclose(&actions, stdoutPipe[0]);
    ::posix_spawn_file_actions_addclose(&actions, stderrPipe[0]);
    if (!request.workingDirectory.empty())
        ::posix_spawn_file_actions_addchdir_np(&actions, request.workingDirectory.c_str());

    // argv: program name + arguments, NUL-terminated. No shell involved.
    std::vector<std::string> argStorage;
    argStorage.push_back(request.executable.filename().string());
    for (const auto& arg : request.arguments)
        argStorage.push_back(arg);
    std::vector<char*> argv;
    argv.reserve(argStorage.size() + 1);
    for (auto& s : argStorage)
        argv.push_back(s.data());
    argv.push_back(nullptr);

    // envp: parent environment with overlay applied/replaced.
    std::vector<std::string> envStorage;
    for (char** env = environ; *env != nullptr; ++env) {
        std::string entry(*env);
        const auto eq = entry.find('=');
        const std::string key = entry.substr(0, eq);
        bool overridden = false;
        for (const auto& [k, v] : request.environment)
            if (k == key) {
                overridden = true;
                break;
            }
        if (!overridden)
            envStorage.push_back(std::move(entry));
    }
    for (const auto& [k, v] : request.environment)
        envStorage.push_back(k + "=" + v);
    std::vector<char*> envp;
    envp.reserve(envStorage.size() + 1);
    for (auto& s : envStorage)
        envp.push_back(s.data());
    envp.push_back(nullptr);

    const auto startTime = std::chrono::steady_clock::now();

    pid_t pid = -1;
    const std::string exe = request.executable.string();
    const int spawnRc = ::posix_spawnp(&pid, exe.c_str(), &actions, nullptr, argv.data(),
                                       envp.data());
    ::posix_spawn_file_actions_destroy(&actions);
    ::close(stdoutPipe[1]);
    ::close(stderrPipe[1]);

    if (spawnRc != 0) {
        ::close(stdoutPipe[0]);
        ::close(stderrPipe[0]);
        ProcessResult result;
        result.started = false;
        result.stdErr = std::strerror(spawnRc);
        return result;
    }

    ProcessResult result;
    result.started = true;

    // Read both pipes until EOF, watching timeout and cancellation.
    struct pollfd fds[2] = {{stdoutPipe[0], POLLIN, 0}, {stderrPipe[0], POLLIN, 0}};
    int openFds = 2;
    char buffer[8192];

    auto terminateChild = [&pid]() {
        ::kill(pid, SIGTERM);
        for (int i = 0; i < 20; ++i) { // up to ~2 s of grace
            int status = 0;
            if (::waitpid(pid, &status, WNOHANG) == pid)
                return;
            ::usleep(100'000);
        }
        ::kill(pid, SIGKILL);
    };

    bool childReaped = false;
    while (openFds > 0) {
        const int rc = ::poll(fds, 2, 50);
        if (rc < 0 && errno == EINTR)
            continue;
        if (rc < 0)
            break;

        for (int i = 0; i < 2; ++i) {
            if (fds[i].fd < 0 || fds[i].revents == 0)
                continue;
            if (fds[i].revents & (POLLIN | POLLHUP | POLLERR)) {
                const ssize_t n = ::read(fds[i].fd, buffer, sizeof(buffer));
                if (n > 0) {
                    std::string_view chunk(buffer, static_cast<std::size_t>(n));
                    if (i == 0)
                        result.stdOut.append(chunk);
                    else
                        result.stdErr.append(chunk);
                    if (onOutput)
                        onOutput(i == 0 ? OutputStream::StdOut : OutputStream::StdErr, chunk);
                } else {
                    ::close(fds[i].fd);
                    fds[i].fd = -1;
                    --openFds;
                }
            }
        }

        const auto now = std::chrono::steady_clock::now();
        const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(now - startTime);
        if (token != nullptr && token->isCancelled()) {
            terminateChild();
            childReaped = true;
            result.cancelled = true;
            break;
        }
        if (request.timeout && elapsed > *request.timeout) {
            terminateChild();
            childReaped = true;
            result.timedOut = true;
            break;
        }
    }

    if (fds[0].fd >= 0)
        ::close(fds[0].fd);
    if (fds[1].fd >= 0)
        ::close(fds[1].fd);

    if (!childReaped) {
        int status = 0;
        while (::waitpid(pid, &status, 0) < 0 && errno == EINTR) {
        }
        if (WIFEXITED(status))
            result.exitCode = WEXITSTATUS(status);
        else if (WIFSIGNALED(status))
            result.exitCode = 128 + WTERMSIG(status);
    } else {
        result.exitCode = -1;
    }

    result.duration = std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now() - startTime);
    return result;
}

#else // _WIN32

Expected<ProcessResult> PosixProcessRunner::run(const ProcessRequest&, CancellationToken*,
                                                OutputCallback) {
    return Error::make(ErrorCode::BackendUnavailable,
                       "PosixProcessRunner is not available on Windows; use the WSL2 backend");
}

#endif

} // namespace vortia
