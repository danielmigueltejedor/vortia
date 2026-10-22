#pragma once

#include <functional>
#include <memory>
#include <mutex>
#include <string>
#include <string_view>
#include <vector>

namespace vortia {

enum class LogLevel { Trace = 0, Debug, Info, Warning, Error };

[[nodiscard]] constexpr std::string_view toString(LogLevel level) {
    switch (level) {
    case LogLevel::Trace: return "TRACE";
    case LogLevel::Debug: return "DEBUG";
    case LogLevel::Info: return "INFO";
    case LogLevel::Warning: return "WARN";
    case LogLevel::Error: return "ERROR";
    }
    return "?";
}

/// Thread-safe logger with pluggable sinks (GUI console, file, test probe).
/// The core library never writes to stdout directly.
class Logger {
public:
    using Sink = std::function<void(LogLevel, std::string_view)>;

    static Logger& instance() {
        static Logger logger;
        return logger;
    }

    /// Adds a sink; returns a handle to remove it. Thread-safe.
    std::size_t addSink(Sink sink) {
        std::lock_guard lock(mutex_);
        sinks_.push_back({nextId_, std::move(sink)});
        return nextId_++;
    }

    void removeSink(std::size_t id) {
        std::lock_guard lock(mutex_);
        for (auto it = sinks_.begin(); it != sinks_.end(); ++it) {
            if (it->id == id) {
                sinks_.erase(it);
                return;
            }
        }
    }

    void log(LogLevel level, std::string_view message) {
        std::vector<Sink> copy;
        {
            std::lock_guard lock(mutex_);
            copy.reserve(sinks_.size());
            for (const auto& s : sinks_)
                copy.push_back(s.sink);
        }
        for (const auto& sink : copy)
            sink(level, message);
    }

    void clearSinks() {
        std::lock_guard lock(mutex_);
        sinks_.clear();
    }

private:
    Logger() = default;
    struct Entry {
        std::size_t id;
        Sink sink;
    };
    std::mutex mutex_;
    std::vector<Entry> sinks_;
    std::size_t nextId_{1};
};

inline void log(LogLevel level, const std::string& message) {
    Logger::instance().log(level, message);
}

} // namespace vortia
