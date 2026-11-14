//! Streamlines of a velocity field: the path a massless particle follows
//! through the steady flow, found by walking from cell to cell.
//!
//! The velocity is the cell's own value (OpenFOAM's `cell` interpolation, as
//! in the line sampling), so inside a cell the path is straight and bends at
//! the faces. That is exact for the field the solver has, which is constant
//! per cell, and cheap: no search per step, only the faces of the cell the
//! particle is in.

use vortia_core::Vec3;

use crate::case::{Case, FieldValues};
use crate::error::{PostError, Result};

/// The most points one streamline takes.
pub const MAX_STREAMLINE_POINTS: usize = 100_000;

/// Why a streamline stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamlineEnd {
    /// It left the domain through a boundary face.
    Boundary,
    /// The flow stops there: no velocity, or the next cell sends it back.
    Stagnation,
    /// It reached the number of points asked for.
    Length,
}

/// A streamline: where it goes and how fast the flow is at each point.
#[derive(Debug, Clone, PartialEq)]
pub struct Streamline {
    pub points: Vec<Vec3>,
    /// The speed of the cell each point was reached in (the seed's cell for
    /// the first).
    pub speeds: Vec<f64>,
    pub end: StreamlineEnd,
}

impl Case {
    /// The streamline of the vector field `name` through `seed`, forward with
    /// the flow or `backward` against it, of at most `max_points` points.
    pub fn streamline(
        &self,
        time: &str,
        name: &str,
        seed: Vec3,
        backward: bool,
        max_points: usize,
    ) -> Result<Streamline> {
        if max_points == 0 || max_points > MAX_STREAMLINE_POINTS {
            return Err(PostError::Invalid(format!(
                "a streamline takes between 1 and {MAX_STREAMLINE_POINTS} points, not {max_points}"
            )));
        }
        if !(seed.x.is_finite() && seed.y.is_finite() && seed.z.is_finite()) {
            return Err(PostError::Invalid("the seed is not finite".into()));
        }
        let velocity = self.velocity(time, name)?;
        self.trace(&velocity, seed, backward, max_points)
    }

    /// The streamlines of the vector field `name` through each of `seeds`, in
    /// both directions, as one line each: upstream of the seed, then
    /// downstream. A seed outside the mesh gives no line. The field is read
    /// once, which is what makes a few hundred lines cheap.
    pub fn streamlines(
        &self,
        time: &str,
        name: &str,
        seeds: &[Vec3],
        max_points: usize,
    ) -> Result<Vec<Streamline>> {
        if !(2..=MAX_STREAMLINE_POINTS).contains(&max_points) {
            return Err(PostError::Invalid(format!(
                "a streamline takes between 2 and {MAX_STREAMLINE_POINTS} points, not {max_points}"
            )));
        }
        let velocity = self.velocity(time, name)?;
        let half = max_points / 2;
        let mut lines = Vec::new();
        for &seed in seeds {
            let (Ok(forward), Ok(mut back)) = (
                self.trace(&velocity, seed, false, half),
                self.trace(&velocity, seed, true, half),
            ) else {
                continue;
            };
            // The seed is the first point of both halves; keep it once.
            back.points.reverse();
            back.speeds.reverse();
            back.points.pop();
            back.speeds.pop();
            back.points.extend(forward.points);
            back.speeds.extend(forward.speeds);
            lines.push(Streamline {
                points: back.points,
                speeds: back.speeds,
                end: forward.end,
            });
        }
        Ok(lines)
    }

    fn velocity(&self, time: &str, name: &str) -> Result<Vec<Vec3>> {
        match self.field(time, name)? {
            FieldValues::Vector(v) => Ok(v),
            _ => Err(PostError::Invalid(format!("{name} is not a vector field"))),
        }
    }

    fn trace(
        &self,
        velocity: &[Vec3],
        seed: Vec3,
        backward: bool,
        max_points: usize,
    ) -> Result<Streamline> {
        let mut cell = self
            .locate(seed)
            .ok_or_else(|| PostError::Invalid("the seed is outside the mesh".into()))?;
        let sign = if backward { -1.0 } else { 1.0 };
        let fastest = velocity.iter().map(|v| v.norm()).fold(0.0, f64::max);
        let stagnant = 1.0e-9 * fastest;

        let mut pos = seed;
        let speed_of = |c: usize| velocity.get(c).map_or(0.0, |v| v.norm());
        let mut points = vec![pos];
        let mut speeds = vec![speed_of(cell)];
        let mut entered: Option<usize> = None;
        let end = loop {
            if points.len() >= max_points {
                break StreamlineEnd::Length;
            }
            let Some(&v) = velocity.get(cell) else {
                break StreamlineEnd::Boundary;
            };
            let speed = v.norm();
            if speed.is_nan() || speed <= stagnant {
                break StreamlineEnd::Stagnation;
            }
            let dir = v * (sign / speed);
            let Some((t, face)) = self.exit(cell, pos, dir) else {
                break StreamlineEnd::Stagnation;
            };
            // A cell that sends the particle back across the face it came in
            // by has no way on: the flow converges there.
            if entered == Some(face) && t <= 0.0 {
                break StreamlineEnd::Stagnation;
            }
            pos = pos + dir * t;
            points.push(pos);
            speeds.push(speed);
            let owner = self.mesh.owner().get(face).copied();
            let next = if owner == Some(cell) {
                self.mesh.neighbour().get(face).copied()
            } else {
                owner
            };
            match next {
                Some(n) if owner == Some(cell) || self.mesh.neighbour().get(face).is_some() => {
                    cell = n;
                    entered = Some(face);
                }
                _ => break StreamlineEnd::Boundary,
            }
        };
        Ok(Streamline {
            points,
            speeds,
            end,
        })
    }

    /// The face a ray from `pos` along `dir` leaves `cell` by, and the
    /// distance to it.
    fn exit(&self, cell: usize, pos: Vec3, dir: Vec3) -> Option<(f64, usize)> {
        let mut best: Option<(f64, usize)> = None;
        for &f in self.cell_faces.get(cell)? {
            let (Some(&centre), Some(&area), Some(&owner)) = (
                self.geometry.face_centres.get(f),
                self.geometry.face_areas.get(f),
                self.mesh.owner().get(f),
            ) else {
                continue;
            };
            let outward = if owner == cell { area } else { area * -1.0 };
            let denom = dir.dot(outward);
            if denom <= 1.0e-12 * outward.norm() {
                continue;
            }
            let t = ((centre - pos).dot(outward) / denom).max(0.0);
            if best.is_none_or(|(b, _)| t < b) {
                best = Some((t, f));
            }
        }
        best
    }
}
