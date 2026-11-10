//! Physical quantities in SI units, checked by the type system.
//!
//! Each quantity is its own type, so a length cannot be added to a time. Products and
//! quotients are defined only for the pairs the application uses: the C++ library derives
//! every combination from template exponents, which stable Rust cannot express, so the
//! relations are listed here one by one. Adding a new one is a single `relate!` line.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

macro_rules! quantity {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        pub struct $name(f64);

        impl $name {
            /// Builds the quantity from its value in SI units.
            pub const fn new(si_value: f64) -> Self {
                Self(si_value)
            }

            /// The value in SI units.
            pub const fn si_value(self) -> f64 {
                self.0
            }

            pub fn abs(self) -> Self {
                Self(self.0.abs())
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, o: Self) -> Self {
                Self(self.0 + o.0)
            }
        }
        impl Sub for $name {
            type Output = Self;
            fn sub(self, o: Self) -> Self {
                Self(self.0 - o.0)
            }
        }
        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }
        impl AddAssign for $name {
            fn add_assign(&mut self, o: Self) {
                self.0 += o.0;
            }
        }
        impl SubAssign for $name {
            fn sub_assign(&mut self, o: Self) {
                self.0 -= o.0;
            }
        }
        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(self, s: f64) -> Self {
                Self(self.0 * s)
            }
        }
        impl Mul<$name> for f64 {
            type Output = $name;
            fn mul(self, q: $name) -> $name {
                $name(self * q.0)
            }
        }
        impl Div<f64> for $name {
            type Output = Self;
            fn div(self, s: f64) -> Self {
                Self(self.0 / s)
            }
        }
        impl MulAssign<f64> for $name {
            fn mul_assign(&mut self, s: f64) {
                self.0 *= s;
            }
        }
        impl DivAssign<f64> for $name {
            fn div_assign(&mut self, s: f64) {
                self.0 /= s;
            }
        }
        /// The ratio of two quantities of the same kind is a plain number.
        impl Div for $name {
            type Output = f64;
            fn div(self, o: Self) -> f64 {
                self.0 / o.0
            }
        }
    };
}

/// `relate!(A * B = C)` defines `A * B`, `B * A`, `C / A` and `C / B`.
macro_rules! relate {
    ($a:ident * $b:ident = $c:ident) => {
        impl Mul<$b> for $a {
            type Output = $c;
            fn mul(self, o: $b) -> $c {
                $c::new(self.si_value() * o.si_value())
            }
        }
        impl Mul<$a> for $b {
            type Output = $c;
            fn mul(self, o: $a) -> $c {
                $c::new(self.si_value() * o.si_value())
            }
        }
        impl Div<$a> for $c {
            type Output = $b;
            fn div(self, o: $a) -> $b {
                $b::new(self.si_value() / o.si_value())
            }
        }
        impl Div<$b> for $c {
            type Output = $a;
            fn div(self, o: $b) -> $a {
                $a::new(self.si_value() / o.si_value())
            }
        }
    };
    // A quantity times itself: only one product and one quotient exist.
    ($a:ident squared = $c:ident) => {
        impl Mul for $a {
            type Output = $c;
            fn mul(self, o: $a) -> $c {
                $c::new(self.si_value() * o.si_value())
            }
        }
        impl Div<$a> for $c {
            type Output = $a;
            fn div(self, o: $a) -> $a {
                $a::new(self.si_value() / o.si_value())
            }
        }
    };
}

quantity!(
    /// kg
    Mass
);
quantity!(
    /// m
    Length
);
quantity!(
    /// m²
    Area
);
quantity!(
    /// m³
    Volume
);
quantity!(
    /// s
    Time
);
quantity!(
    /// m·s⁻¹
    Velocity
);
quantity!(
    /// m·s⁻²
    Acceleration
);
quantity!(
    /// kg·m⁻³
    Density
);
quantity!(
    /// Pa·s
    DynamicViscosity
);
quantity!(
    /// m²·s⁻¹
    KinematicViscosity
);
quantity!(
    /// Pa
    Pressure
);

relate!(Length squared = Area);
relate!(Area * Length = Volume);
relate!(Velocity * Time = Length);
relate!(Acceleration * Time = Velocity);
relate!(Velocity * Length = KinematicViscosity);
relate!(KinematicViscosity * Density = DynamicViscosity);
relate!(Density * Volume = Mass);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn same_dimension_arithmetic() {
        let (a, b) = (Length::new(2.0), Length::new(3.0));
        assert_eq!((a + b).si_value(), 5.0);
        assert_eq!((a - b).si_value(), -1.0);
        assert_eq!(b / a, 1.5); // dimensionless ratio
        assert!(b > a);
    }

    #[test]
    fn dimensional_multiplication() {
        let d: Length = Velocity::new(2.0) * Time::new(3.0);
        assert_eq!(d.si_value(), 6.0);
    }

    #[test]
    fn velocity_from_length_over_time() {
        let v: Velocity = Length::new(10.0) / Time::new(4.0);
        assert_eq!(v.si_value(), 2.5);
    }

    #[test]
    fn kinematic_viscosity_dimensions() {
        let nu: KinematicViscosity = DynamicViscosity::new(1.8e-5) / Density::new(1.2);
        assert!(close(nu.si_value(), 1.5e-5, 1e-12));
    }

    #[test]
    fn scalar_scaling() {
        let a = Length::new(1.0) * 2.5;
        assert_eq!(a.si_value(), 2.5);
        assert_eq!((a / 2.0).si_value(), 1.25);
        assert_eq!((2.0 * a).si_value(), 5.0);
    }

    #[test]
    fn reynolds_number_is_dimensionless() {
        let ul: KinematicViscosity = Velocity::new(1.0) * Length::new(0.1);
        let re: f64 = ul / KinematicViscosity::new(1.5e-5);
        assert!(close(re, 6666.6667, 1e-3));
    }

    #[test]
    fn negation_and_absolute_value() {
        assert_eq!((-Pressure::new(3.0)).abs(), Pressure::new(3.0));
        let mut m = Mass::new(1.0);
        m += Mass::new(2.0);
        m *= 2.0;
        assert_eq!(m.si_value(), 6.0);
    }
}
