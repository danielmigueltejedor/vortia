//! The flow of a vector field drawn over the mesh: streamlines through a grid of seeds and an
//! arrow in the cell of each, from the post-processing library, computed off the UI thread.
//!
//! The seeds lie on a grid across the two longest sides of the mesh, in the middle of the
//! third: that is the plane a 2-D or wedge case lives in, and a readable slice of a 3-D one.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use vortia_core::Vec3;
use vortia_post::{Case, FieldValues};

/// Seeds along the longest side of the mesh.
const SEEDS_ALONG: usize = 18;
/// Points of one streamline (both directions together).
const LINE_POINTS: usize = 4_000;

/// One streamline: its points and the speed at each.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub points: Vec<[f32; 3]>,
    pub speeds: Vec<f32>,
}

/// An arrow: where it sits, the direction of the flow there and its speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrow {
    pub at: [f32; 3],
    pub direction: [f32; 3],
    pub speed: f32,
}

/// The flow of a field at one time.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FlowLines {
    pub lines: Vec<Line>,
    pub arrows: Vec<Arrow>,
    /// The slowest and fastest speed among the lines and arrows.
    pub range: Option<(f32, f32)>,
}

impl FlowLines {
    /// Where a speed falls in the range, 0 to 1.
    pub fn fraction(&self, speed: f32) -> f32 {
        match self.range {
            Some((lo, hi)) if hi > lo && speed.is_finite() => {
                ((speed - lo) / (hi - lo)).clamp(0.0, 1.0)
            }
            _ => 0.5,
        }
    }
}

/// The seeds: a grid over the two longest sides of the box, in the middle of the third.
fn seeds(min: Vec3, max: Vec3) -> Vec<Vec3> {
    let low = [min.x, min.y, min.z];
    let size = [max.x - min.x, max.y - min.y, max.z - min.z];
    let get = |values: &[f64; 3], axis: usize| values.get(axis).copied().unwrap_or(0.0);
    let mut axes = [0usize, 1, 2];
    axes.sort_by(|&a, &b| get(&size, b).total_cmp(&get(&size, a)));
    let [first, second, flat] = axes;
    let longest = get(&size, first);
    if longest <= 0.0 || !longest.is_finite() {
        return Vec::new();
    }
    let n_first = SEEDS_ALONG;
    let n_second = ((SEEDS_ALONG as f64 * get(&size, second) / longest).round() as usize)
        .clamp(1, SEEDS_ALONG);
    let mut out = Vec::with_capacity(n_first * n_second);
    for i in 0..n_first {
        for j in 0..n_second {
            let mut p = low;
            let mut put = |axis: usize, value: f64| {
                if let Some(slot) = p.get_mut(axis) {
                    *slot = value;
                }
            };
            put(
                first,
                get(&low, first) + longest * (i as f64 + 0.5) / n_first as f64,
            );
            put(
                second,
                get(&low, second) + get(&size, second) * (j as f64 + 0.5) / n_second as f64,
            );
            put(flat, get(&low, flat) + get(&size, flat) * 0.5);
            let [x, y, z] = p;
            out.push(Vec3::new(x, y, z));
        }
    }
    out
}

fn single(p: Vec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

/// The streamlines and arrows of the vector field `name` of `case` at `time`.
pub fn compute(case: &Path, time: &str, name: &str) -> Result<FlowLines, String> {
    let plain =
        |part: &str| !part.is_empty() && !part.contains(['/', '\\']) && part != ".." && part != ".";
    if !plain(time) || !plain(name) {
        return Err(format!("'{time}/{name}' is not a field of the case."));
    }
    let case = Case::open(case).map_err(|e| e.to_string())?;
    let bounds = case.bounds();
    let seeds = seeds(bounds.min, bounds.max);
    let velocity = match case.field(time, name).map_err(|e| e.to_string())? {
        FieldValues::Vector(v) => v,
        _ => {
            return Err(format!(
                "{name} is not a vector field, so it has no flow lines."
            ));
        }
    };
    let lines = case
        .streamlines(time, name, &seeds, LINE_POINTS)
        .map_err(|e| e.to_string())?;

    let mut out = FlowLines::default();
    let mut range: Option<(f32, f32)> = None;
    let mut note = |speed: f32| {
        if speed.is_finite() {
            range = Some(match range {
                Some((lo, hi)) => (lo.min(speed), hi.max(speed)),
                None => (speed, speed),
            });
        }
    };
    for line in lines {
        let speeds: Vec<f32> = line.speeds.iter().map(|&s| s as f32).collect();
        speeds.iter().copied().for_each(&mut note);
        out.lines.push(Line {
            points: line.points.iter().copied().map(single).collect(),
            speeds,
        });
    }
    for seed in &seeds {
        let Some(cell) = case.locate(*seed) else {
            continue;
        };
        let Some(&v) = velocity.get(cell) else {
            continue;
        };
        let speed = v.norm();
        if speed.is_finite() && speed > 0.0 {
            note(speed as f32);
            out.arrows.push(Arrow {
                at: single(*seed),
                direction: single(v * (1.0 / speed)),
                speed: speed as f32,
            });
        }
    }
    out.range = range;
    Ok(out)
}

/// Computes the flow on a thread; `wake` is called when it is ready.
pub fn spawn(
    case: PathBuf,
    time: String,
    name: String,
    wake: impl Fn() + Send + 'static,
) -> Receiver<Result<FlowLines, String>> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("vortia-flow".into())
        .spawn(move || {
            let read = std::panic::catch_unwind(|| compute(&case, &time, &name))
                .unwrap_or_else(|_| Err("The flow could not be computed.".to_owned()));
            let _ = tx.send(read);
            wake();
        });
    // No thread, no lines: the viewer keeps the picture it has.
    drop(spawned);
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box_raw;

    fn case(velocity: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let bounds = Aabb {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(4.0, 2.0, 1.0),
        };
        structured_box_raw(bounds, [4, 2, 1])
            .unwrap()
            .write_case(dir.path())
            .unwrap();
        std::fs::create_dir(dir.path().join("0")).unwrap();
        std::fs::write(
            dir.path().join("0").join("U"),
            format!(
                "FoamFile {{ version 2.0; format ascii; class volVectorField; object U; }}\n\
                 dimensions [0 1 -1 0 0 0 0];\ninternalField {velocity};\n\
                 boundaryField {{ xMin {{ type zeroGradient; }} xMax {{ type zeroGradient; }} \
                 yMin {{ type zeroGradient; }} yMax {{ type zeroGradient; }} \
                 zMin {{ type zeroGradient; }} zMax {{ type zeroGradient; }} }}\n"
            ),
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_uniform_flow_gives_straight_lines_and_arrows_along_it() {
        let dir = case("uniform (2 0 0)");
        let flow = compute(dir.path(), "0", "U").unwrap();
        assert!(!flow.lines.is_empty());
        assert_eq!(flow.lines.len(), flow.arrows.len());
        for line in &flow.lines {
            let first = line.points.first().unwrap();
            let last = line.points.last().unwrap();
            assert!(first[0].abs() < 1e-5 && (last[0] - 4.0).abs() < 1e-5);
            assert!((first[1] - last[1]).abs() < 1e-5);
        }
        for arrow in &flow.arrows {
            assert_eq!(arrow.direction, [1.0, 0.0, 0.0]);
            assert_eq!(arrow.speed, 2.0);
        }
        assert_eq!(flow.range, Some((2.0, 2.0)));
        assert_eq!(flow.fraction(2.0), 0.5);
    }

    #[test]
    fn it_refuses_what_is_not_a_vector_field_or_a_field_of_the_case() {
        let dir = case("uniform (1 0 0)");
        assert!(compute(dir.path(), "0", "../U").is_err());
        assert!(compute(dir.path(), "0", "nope").is_err());
        assert!(compute(dir.path(), "5", "U").is_err());
    }

    #[test]
    fn seeds_stay_on_the_two_long_sides() {
        let s = seeds(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.001));
        assert!(!s.is_empty());
        assert!(s.iter().all(|p| (p.z - 0.0005).abs() < 1e-12));
        assert!(seeds(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)).is_empty());
        assert!(seeds(Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)).is_empty());
    }
}
