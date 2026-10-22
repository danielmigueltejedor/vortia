//! Units with runtime dimensional checking.
//!
//! The C++ side uses compile-time dimensions (const generics with integer
//! exponents). Rust stable does not yet allow arithmetic in const-generic
//! type parameters, so this port checks dimensions at runtime and exposes
//! the same API surface. A future revision will move to `typenum` for
//! compile-time checking.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dim {
    pub mass: i8,
    pub length: i8,
    pub time: i8,
}

impl Dim {
    pub const fn new(mass: i8, length: i8, time: i8) -> Self {
        Self { mass, length, time }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Quantity {
    pub value: f64,
    pub dim: Dim,
}

impl Quantity {
    pub fn new(value: f64, dim: Dim) -> Self {
        Self { value, dim }
    }
    pub fn si_value(self) -> f64 {
        self.value
    }

    fn check_same(&self, other: &Quantity, op: &str) {
        if self.dim != other.dim {
            panic!("dimension mismatch in {}: {:?} vs {:?}", op, self.dim, other.dim);
        }
    }

    pub fn add(self, rhs: Quantity) -> Quantity {
        self.check_same(&rhs, "add");
        Quantity::new(self.value + rhs.value, self.dim)
    }
    pub fn sub(self, rhs: Quantity) -> Quantity {
        self.check_same(&rhs, "sub");
        Quantity::new(self.value - rhs.value, self.dim)
    }
    pub fn mul_scalar(self, s: f64) -> Quantity {
        Quantity::new(self.value * s, self.dim)
    }
    pub fn div_scalar(self, s: f64) -> Quantity {
        Quantity::new(self.value / s, self.dim)
    }
    pub fn mul(self, rhs: Quantity) -> Quantity {
        Quantity::new(
            self.value * rhs.value,
            Dim::new(
                self.dim.mass + rhs.dim.mass,
                self.dim.length + rhs.dim.length,
                self.dim.time + rhs.dim.time,
            ),
        )
    }
    pub fn div(self, rhs: Quantity) -> Quantity {
        Quantity::new(
            self.value / rhs.value,
            Dim::new(
                self.dim.mass - rhs.dim.mass,
                self.dim.length - rhs.dim.length,
                self.dim.time - rhs.dim.time,
            ),
        )
    }
    pub fn ratio(self, rhs: Quantity) -> f64 {
        self.check_same(&rhs, "ratio");
        self.value / rhs.value
    }
}

// Named dimensions.
pub const DIM_DIMENSIONLESS: Dim = Dim::new(0, 0, 0);
pub const DIM_MASS: Dim = Dim::new(1, 0, 0);
pub const DIM_LENGTH: Dim = Dim::new(0, 1, 0);
pub const DIM_TIME: Dim = Dim::new(0, 0, 1);
pub const DIM_VELOCITY: Dim = Dim::new(0, 1, -1);
pub const DIM_DENSITY: Dim = Dim::new(1, -3, 0);
pub const DIM_KINEMATIC_VISCOSITY: Dim = Dim::new(0, 2, -1);
pub const DIM_PRESSURE: Dim = Dim::new(1, -1, -2);

pub type Dimensionless = Quantity;
pub type Mass = Quantity;
pub type Length = Quantity;
pub type Time = Quantity;
pub type Velocity = Quantity;
pub type Density = Quantity;
pub type KinematicViscosity = Quantity;
pub type Pressure = Quantity;

// Convenience constructors as free functions (Rust has no const fn methods on
// aliased types easily; keep them as associated functions on Quantity).
impl Quantity {
    pub fn length(v: f64) -> Self {
        Self::new(v, DIM_LENGTH)
    }
    pub fn time(v: f64) -> Self {
        Self::new(v, DIM_TIME)
    }
    pub fn velocity(v: f64) -> Self {
        Self::new(v, DIM_VELOCITY)
    }
    pub fn kinematic_viscosity(v: f64) -> Self {
        Self::new(v, DIM_KINEMATIC_VISCOSITY)
    }
    pub fn pressure(v: f64) -> Self {
        Self::new(v, DIM_PRESSURE)
    }
    pub fn dimensionless(v: f64) -> Self {
        Self::new(v, DIM_DIMENSIONLESS)
    }
}