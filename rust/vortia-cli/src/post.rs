//! `probe`, `sample`, `streamline`, `patch`, `flux` and `volume`: looking into a case's
//! results from the command line, with the numbers OpenFOAM's `postProcess`
//! gives for the same files.

use std::process::ExitCode;

use vortia_core::Vec3;
use vortia_post::{Case, PatchStats, VolumeStats};

use crate::{shown, usage_error};

/// Runs one of the five; `None` when `args` is not one of them.
pub fn run(args: &[&str]) -> Option<ExitCode> {
    Some(match args {
        ["probe", case, field, x, y, z, time @ ..] if time.len() <= 1 => point(&[x, y, z])
            .map_or_else(
                || usage_error("the point needs three numbers"),
                |p| finish(probe(case, field, p, time.first().copied())),
            ),
        ["sample", case, field, a @ .., points] if a.len() == 6 || a.len() == 7 => {
            // The last two words are the point count and an optional time.
            let (a, count, time) = if a.len() == 7 {
                (a.get(..6)?, *a.get(6)?, Some(*points))
            } else {
                (a, *points, None)
            };
            match (
                point(a.get(..3)?),
                point(a.get(3..6)?),
                count.parse::<usize>(),
            ) {
                (Some(start), Some(end), Ok(n)) => finish(sample(case, field, start, end, n, time)),
                _ => usage_error("the line needs two points and a number of points"),
            }
        }
        ["export", case, format, file, rest @ ..] => {
            let (time, fields) = match rest {
                [] => (None, &[][..]),
                [t, f @ ..] => (Some(*t), f),
            };
            finish(export(case, format, file, time, fields))
        }
        ["streamline", case, field, x, y, z, rest @ ..] => {
            let (backward, time) = match rest {
                [] => (false, None),
                ["-backward"] => (true, None),
                ["-backward", t] => (true, Some(*t)),
                [t] => (false, Some(*t)),
                _ => return Some(usage_error("wrong number of arguments")),
            };
            point(&[x, y, z]).map_or_else(
                || usage_error("the point needs three numbers"),
                |p| finish(streamline(case, field, p, backward, time)),
            )
        }
        ["patch", case, patch, field, time @ ..] if time.len() <= 1 => {
            finish(patch_stats(case, patch, field, time.first().copied()))
        }
        ["flux", case, patch, time @ ..] if time.len() <= 1 => {
            finish(flux(case, patch, time.first().copied()))
        }
        ["volume", case, field, time @ ..] if time.len() <= 1 => {
            finish(volume(case, field, time.first().copied()))
        }
        ["wall", case, patch, time @ ..] if time.len() <= 1 => {
            finish(wall(case, patch, time.first().copied()))
        }
        [
            "probe" | "sample" | "streamline" | "export" | "patch" | "flux" | "volume" | "wall",
            ..,
        ] => usage_error("wrong number of arguments"),
        _ => return None,
    })
}

fn point(words: &[&str]) -> Option<Vec3> {
    let n: Vec<f64> = words
        .iter()
        .map(|w| w.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()?;
    match n.as_slice() {
        [x, y, z] => Some(Vec3::new(*x, *y, *z)),
        _ => None,
    }
}

fn finish(result: Result<String, String>) -> ExitCode {
    match result {
        Ok(text) => {
            let _ = std::io::Write::write_all(&mut std::io::stdout(), text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vortia: {}", shown(&e));
            ExitCode::FAILURE
        }
    }
}

fn open(case: &str, time: Option<&str>) -> Result<(Case, String), String> {
    let case = Case::open(case).map_err(|e| e.to_string())?;
    let time = match time {
        Some(t) => t.to_owned(),
        None => case.latest_time().map_err(|e| e.to_string())?,
    };
    Ok((case, time))
}

fn numbers(values: &[f64]) -> String {
    let parts: Vec<String> = values.iter().map(|v| format!("{:e}", v + 0.0)).collect();
    if let [one] = parts.as_slice() {
        one.clone()
    } else {
        format!("({})", parts.join(" "))
    }
}

fn probe(case: &str, field: &str, at: Vec3, time: Option<&str>) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let s = case.probe(&time, field, at).map_err(|e| e.to_string())?;
    match s.cell {
        Some(cell) => Ok(format!(
            "Time {}  {}  cell {cell}  {}\n",
            shown(&time),
            shown(field),
            numbers(&s.value)
        )),
        None => Err(format!(
            "the point ({:e} {:e} {:e}) is outside the mesh",
            at.x, at.y, at.z
        )),
    }
}

fn sample(
    case: &str,
    field: &str,
    start: Vec3,
    end: Vec3,
    points: usize,
    time: Option<&str>,
) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let (values, samples) = case
        .sample_line(&time, field, start, end, points)
        .map_err(|e| e.to_string())?;
    Ok(vortia_post::line_csv(field, &values, &samples))
}

fn streamline(
    case: &str,
    field: &str,
    seed: Vec3,
    backward: bool,
    time: Option<&str>,
) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let line = case
        .streamline(&time, field, seed, backward, 10_000)
        .map_err(|e| e.to_string())?;
    let mut out = String::from("x,y,z,speed\n");
    for (p, s) in line.points.iter().zip(&line.speeds) {
        out.push_str(&format!("{:e},{:e},{:e},{:e}\n", p.x, p.y, p.z, s));
    }
    eprintln!("vortia: the line ended at {}", end_name(line.end));
    Ok(out)
}

fn export(
    case: &str,
    format: &str,
    file: &str,
    time: Option<&str>,
    fields: &[&str],
) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let fields: Vec<String> = fields.iter().map(|f| (*f).to_owned()).collect();
    let text = match format {
        "csv" => case.cells_csv(&time, &fields),
        "vtu" => case.vtu(&time, &fields),
        other => {
            return Err(format!("the format is csv or vtu, not '{}'", shown(other)));
        }
    }
    .map_err(|e| e.to_string())?;
    std::fs::write(file, &text).map_err(|e| format!("could not write {}: {e}", shown(file)))?;
    Ok(format!(
        "Wrote {} ({} bytes) from time {}\n",
        shown(file),
        text.len(),
        shown(&time)
    ))
}

fn end_name(end: vortia_post::StreamlineEnd) -> &'static str {
    match end {
        vortia_post::StreamlineEnd::Boundary => "the boundary",
        vortia_post::StreamlineEnd::Stagnation => "a stagnation point",
        vortia_post::StreamlineEnd::Length => "its length limit",
    }
}

fn patch_stats(case: &str, patch: &str, field: &str, time: Option<&str>) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let s: PatchStats = case
        .patch_stats(&time, field, patch)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Time {}  patch {}  {}\n    faces    {}\n    area     {:e}\n    integral {}\n    average  {}\n    min      {}\n    max      {}\n",
        shown(&time),
        shown(patch),
        shown(field),
        s.faces,
        s.area,
        numbers(&s.integral),
        numbers(&s.average),
        numbers(&s.min),
        numbers(&s.max),
    ))
}

fn flux(case: &str, patch: &str, time: Option<&str>) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let q = case.flux(&time, patch).map_err(|e| e.to_string())?;
    Ok(format!(
        "Time {}  patch {}  flux {q:e}  (positive out of the domain)\n",
        shown(&time),
        shown(patch)
    ))
}

fn volume(case: &str, field: &str, time: Option<&str>) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let s: VolumeStats = case.volume_stats(&time, field).map_err(|e| e.to_string())?;
    Ok(format!(
        "Time {}  {}\n    cells    {}\n    volume   {:e}\n    integral {}\n    average  {}\n    min      {}\n    max      {}\n",
        shown(&time),
        shown(field),
        s.cells,
        s.volume,
        numbers(&s.integral),
        numbers(&s.average),
        numbers(&s.min),
        numbers(&s.max),
    ))
}

fn wall(case: &str, patch: &str, time: Option<&str>) -> Result<String, String> {
    let (case, time) = open(case, time)?;
    let w = case.wall(&time, patch).map_err(|e| e.to_string())?;
    let (stress, force) = if w.kinematic {
        ("m2/s2, per unit density", "m3/s2, per unit density")
    } else {
        ("Pa", "N")
    };
    let mut out = format!(
        "Time {}  patch {}\n    area            {:e}\n    mean shear      {} {stress}\n    viscous force   {} {force}\n",
        shown(&time),
        shown(patch),
        w.area,
        numbers(&[w.mean_shear().x, w.mean_shear().y, w.mean_shear().z]),
        numbers(&[w.viscous_force.x, w.viscous_force.y, w.viscous_force.z]),
    );
    if let Some(f) = w.pressure_force {
        out.push_str(&format!(
            "    pressure force  {} {force}\n",
            numbers(&[f.x, f.y, f.z])
        ));
    }
    if let Some((low, mean, high)) = w.y_plus_range() {
        out.push_str(&format!(
            "    y+              min {low:.4} mean {mean:.4} max {high:.4}\n"
        ));
    }
    if let (Some(rate), Some(q)) = (w.heat_rate(), &w.heat_flux) {
        let (lo, hi) = q
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                (l.min(*v), h.max(*v))
            });
        out.push_str(&format!(
            "    heat into fluid {rate:e} W  (flux {lo:e} to {hi:e} W/m2)\n"
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_needs_three_finite_numbers() {
        assert_eq!(point(&["1", "2", "3"]), Some(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(point(&["1", "2"]), None);
        assert_eq!(point(&["1", "nan", "3"]), None);
        assert_eq!(point(&["1", "inf", "3"]), None);
    }

    #[test]
    fn other_commands_are_not_taken() {
        assert!(run(&["solve", "case"]).is_none());
    }
}
