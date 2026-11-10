//! Three-component vectors and axis-aligned boxes, in SI units.

use std::ops::{Add, Div, Index, IndexMut, Mul, Sub};

/// A point or direction in space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self::new(self.x / s, self.y / s, self.z / s)
    }
}

/// Component by index: 0 is x, 1 is y and anything else is z, as in the C++ type.
impl Index<usize> for Vec3 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => &self.z,
        }
    }
}

impl IndexMut<usize> for Vec3 {
    fn index_mut(&mut self, i: usize) -> &mut f64 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => &mut self.z,
        }
    }
}

/// An axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    /// Grows the box so it contains `p`.
    pub fn extend(&mut self, p: Vec3) {
        self.min = Vec3::new(
            self.min.x.min(p.x),
            self.min.y.min(p.y),
            self.min.z.min(p.z),
        );
        self.max = Vec3::new(
            self.max.x.max(p.x),
            self.max.y.max(p.y),
            self.max.z.max(p.z),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_products() {
        let (x, y) = (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(x.cross(y), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(x.dot(y), 0.0);
        assert_eq!(Vec3::new(3.0, 4.0, 0.0).norm(), 5.0);
        assert_eq!((x + y) * 2.0 - y, Vec3::new(2.0, 1.0, 0.0));
        assert_eq!(Vec3::new(2.0, 4.0, 6.0) / 2.0, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn indexing_follows_the_cpp_type() {
        let mut v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!((v[0], v[1], v[2], v[9]), (1.0, 2.0, 3.0, 3.0));
        v[1] = 5.0;
        assert_eq!(v.y, 5.0);
    }

    #[test]
    fn box_grows_to_contain_points() {
        let mut b = Aabb {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        b.extend(Vec3::new(-1.0, 2.0, 0.5));
        assert_eq!(b.min, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(b.max, Vec3::new(1.0, 2.0, 1.0));
        assert_eq!(b.size(), Vec3::new(2.0, 2.0, 1.0));
        assert_eq!(b.center(), Vec3::new(0.0, 1.0, 0.5));
        assert!(b.is_valid());
        assert!(
            !Aabb {
                min: Vec3::new(1.0, 0.0, 0.0),
                max: Vec3::new(0.0, 1.0, 1.0)
            }
            .is_valid()
        );
    }
}
