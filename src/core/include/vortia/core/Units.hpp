#pragma once

// Compile-time dimensional analysis over SI base units (mass, length, time).
// Quantities are stored in SI base units; conversions are explicit and exact.

#include <cmath>
#include <ratio>
#include <type_traits>

namespace vortia {

/// Quantity with SI dimensions: mass^M · length^L · time^T (Rep: arithmetic type).
template <typename Rep, int M, int L, int T>
class Quantity {
    static_assert(std::is_arithmetic_v<Rep>);

public:
    using RepType = Rep;
    static constexpr int MassExp = M;
    static constexpr int LengthExp = L;
    static constexpr int TimeExp = T;

    constexpr Quantity() = default;
    constexpr explicit Quantity(Rep siValue) : value_(siValue) {}

    /// Value in SI base units (kg, m, s).
    [[nodiscard]] constexpr Rep siValue() const { return value_; }

    constexpr Quantity operator+() const { return *this; }
    constexpr Quantity operator-() const { return Quantity(-value_); }

    constexpr Quantity& operator+=(const Quantity& o) {
        value_ += o.value_;
        return *this;
    }
    constexpr Quantity& operator-=(const Quantity& o) {
        value_ -= o.value_;
        return *this;
    }
    constexpr Quantity& operator*=(Rep s) {
        value_ *= s;
        return *this;
    }
    constexpr Quantity& operator/=(Rep s) {
        value_ /= s;
        return *this;
    }

    friend constexpr Quantity operator+(Quantity a, const Quantity& b) { return a += b; }
    friend constexpr Quantity operator-(Quantity a, const Quantity& b) { return a -= b; }
    friend constexpr Quantity operator*(Quantity q, Rep s) { return q *= s; }
    friend constexpr Quantity operator*(Rep s, Quantity q) { return q *= s; }
    friend constexpr Quantity operator/(Quantity q, Rep s) { return q /= s; }

    /// Dimensionless ratio between same-dimension quantities.
    friend constexpr Rep operator/(const Quantity& a, const Quantity& b) {
        return a.value_ / b.value_;
    }

    friend constexpr bool operator==(const Quantity& a, const Quantity& b) {
        return a.value_ == b.value_;
    }
    friend constexpr bool operator!=(const Quantity& a, const Quantity& b) { return !(a == b); }
    friend constexpr bool operator<(const Quantity& a, const Quantity& b) {
        return a.value_ < b.value_;
    }
    friend constexpr bool operator<=(const Quantity& a, const Quantity& b) { return !(b < a); }
    friend constexpr bool operator>(const Quantity& a, const Quantity& b) { return b < a; }
    friend constexpr bool operator>=(const Quantity& a, const Quantity& b) { return !(a < b); }

    [[nodiscard]] constexpr Rep abs() const { return value_ < Rep{} ? -value_ : value_; }

private:
    Rep value_{};
};

/// Dimensional multiplication: (M1,L1,T1)·(M2,L2,T2).
template <typename R1, int M1, int L1, int T1, typename R2, int M2, int L2, int T2>
constexpr auto operator*(const Quantity<R1, M1, L1, T1>& a, const Quantity<R2, M2, L2, T2>& b) {
    using R = std::common_type_t<R1, R2>;
    return Quantity<R, M1 + M2, L1 + L2, T1 + T2>(a.siValue() * b.siValue());
}

/// Dimensional division.
template <typename R1, int M1, int L1, int T1, typename R2, int M2, int L2, int T2>
constexpr auto operator/(const Quantity<R1, M1, L1, T1>& a, const Quantity<R2, M2, L2, T2>& b) {
    using R = std::common_type_t<R1, R2>;
    return Quantity<R, M1 - M2, L1 - L2, T1 - T2>(a.siValue() / b.siValue());
}

// ---------------------------------------------------------------------------
// Named physical quantities used across Vortia (double precision).
// ---------------------------------------------------------------------------
using Dimensionless = Quantity<double, 0, 0, 0>;
using Mass = Quantity<double, 1, 0, 0>;               ///< kg
using Length = Quantity<double, 0, 1, 0>;             ///< m
using Area = Quantity<double, 0, 2, 0>;               ///< m²
using Volume = Quantity<double, 0, 3, 0>;             ///< m³
using Time = Quantity<double, 0, 0, 1>;               ///< s
using Velocity = Quantity<double, 0, 1, -1>;          ///< m·s⁻¹
using Acceleration = Quantity<double, 0, 1, -2>;      ///< m·s⁻²
using Density = Quantity<double, 1, -3, 0>;           ///< kg·m⁻³
using DynamicViscosity = Quantity<double, 1, -1, -1>; ///< Pa·s
using KinematicViscosity = Quantity<double, 0, 2, -1>;///< m²·s⁻¹
using Pressure = Quantity<double, 1, -1, -2>;         ///< Pa

namespace literals {

constexpr Length operator""_m(long double v) { return Length(static_cast<double>(v)); }
constexpr Length operator""_m(unsigned long long v) { return Length(static_cast<double>(v)); }
constexpr Time operator""_s(long double v) { return Time(static_cast<double>(v)); }
constexpr Time operator""_s(unsigned long long v) { return Time(static_cast<double>(v)); }
constexpr Velocity operator""_mps(long double v) { return Velocity(static_cast<double>(v)); }
constexpr Velocity operator""_mps(unsigned long long v) { return Velocity(static_cast<double>(v)); }
constexpr KinematicViscosity operator""_m2ps(long double v) {
    return KinematicViscosity(static_cast<double>(v));
}
constexpr Density operator""_kgpm3(long double v) { return Density(static_cast<double>(v)); }

} // namespace literals

} // namespace vortia
