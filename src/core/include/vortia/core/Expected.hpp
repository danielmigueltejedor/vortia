#pragma once

#include "vortia/core/Error.hpp"

#include <optional>
#include <stdexcept>
#include <type_traits>
#include <utility>
#include <variant>

namespace vortia {

/// Minimal expected-like type for C++20 (std::expected is C++23).
/// Either holds a T or an Error. Accessing the value of an error-holding
/// instance throws std::logic_error (programmer error).
template <typename T, typename E = Error>
class Expected {
public:
    Expected(const T& value) : data_(value) {}
    Expected(T&& value) : data_(std::move(value)) {}
    Expected(const E& error) : data_(error) {}
    Expected(E&& error) : data_(std::move(error)) {}

    [[nodiscard]] bool hasValue() const { return std::holds_alternative<T>(data_); }
    explicit operator bool() const { return hasValue(); }

    T& value() & {
        if (!hasValue())
            throw std::logic_error("Expected: access to value of an error");
        return std::get<T>(data_);
    }
    const T& value() const& {
        if (!hasValue())
            throw std::logic_error("Expected: access to value of an error");
        return std::get<T>(data_);
    }
    T&& value() && {
        if (!hasValue())
            throw std::logic_error("Expected: access to value of an error");
        return std::move(std::get<T>(data_));
    }

    T* operator->() { return &value(); }
    const T* operator->() const { return &value(); }
    T& operator*() & { return value(); }
    const T& operator*() const& { return value(); }

    const E& error() const {
        if (hasValue())
            throw std::logic_error("Expected: access to error of a value");
        return std::get<E>(data_);
    }

    template <typename F>
    auto andThen(F&& f) const& -> std::invoke_result_t<F, const T&> {
        using R = std::invoke_result_t<F, const T&>;
        if (hasValue())
            return f(value());
        return R(error());
    }

private:
    std::variant<T, E> data_;
};

/// Specialization for operations that only report success/failure.
template <typename E>
class Expected<void, E> {
public:
    Expected() = default; ///< success
    Expected(const E& error) : error_(error) {}
    Expected(E&& error) : error_(std::move(error)) {}

    [[nodiscard]] bool hasValue() const { return !error_.has_value(); }
    explicit operator bool() const { return hasValue(); }

    void value() const {
        if (!hasValue())
            throw std::logic_error("Expected: access to value of an error");
    }

    const E& error() const {
        if (hasValue())
            throw std::logic_error("Expected: access to error of a value");
        return *error_;
    }

private:
    std::optional<E> error_;
};

} // namespace vortia
