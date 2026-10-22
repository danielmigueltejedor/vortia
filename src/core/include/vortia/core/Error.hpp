#pragma once

#include <string>

namespace vortia {

/// Stable error categories used across all Vortia modules.
enum class ErrorCode {
    None = 0,
    NotFound,
    InvalidArgument,
    InvalidData,
    IoError,
    PermissionDenied,
    ParseError,
    ProcessFailed,
    Timeout,
    Cancelled,
    BackendUnavailable,
    UnsupportedVersion,
    UnsupportedFeature,
    Internal
};

/// Rich error value: category + human message + operation context.
/// Errors are values, never silent. Every failure path must return one.
struct Error {
    ErrorCode code{ErrorCode::None};
    std::string message;
    std::string context; ///< Operation, path or component that produced the error.

    [[nodiscard]] std::string describe() const {
        std::string out = message;
        if (!context.empty()) {
            out += " [" + context + "]";
        }
        return out;
    }

    [[nodiscard]] static Error make(ErrorCode c, std::string msg, std::string ctx = {}) {
        return Error{c, std::move(msg), std::move(ctx)};
    }
};

} // namespace vortia
