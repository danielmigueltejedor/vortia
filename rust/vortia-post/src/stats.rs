//! Weighted sums over faces or cells: what `patchAverage`, `patchIntegrate`
//! and `volFieldValue` report.

use crate::case::FieldValues;

/// Sums of a field with a weight per value (a face's area, a cell's volume).
struct Weighted {
    count: usize,
    weight: f64,
    integral: Vec<f64>,
    min: Vec<f64>,
    max: Vec<f64>,
}

impl Weighted {
    fn of(values: &FieldValues, weights: &[f64]) -> Option<Self> {
        if values.is_empty() || values.len() != weights.len() {
            return None;
        }
        let n = values.components();
        let mut integral = vec![0.0; n];
        let mut min = vec![f64::INFINITY; n];
        let mut max = vec![f64::NEG_INFINITY; n];
        for (i, &w) in weights.iter().enumerate() {
            let v = values.at(i)?;
            for (k, &x) in v.iter().enumerate() {
                *integral.get_mut(k)? += x * w;
                let lo = min.get_mut(k)?;
                *lo = lo.min(x);
                let hi = max.get_mut(k)?;
                *hi = hi.max(x);
            }
        }
        Some(Self {
            count: weights.len(),
            weight: weights.iter().sum(),
            integral,
            min,
            max,
        })
    }

    fn average(&self) -> Vec<f64> {
        if self.weight > 0.0 {
            self.integral.iter().map(|i| i / self.weight).collect()
        } else {
            vec![f64::NAN; self.integral.len()]
        }
    }
}

/// A field over a patch. Vectors are summed component by component.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchStats {
    pub faces: usize,
    pub area: f64,
    /// `∑ value × face area`.
    pub integral: Vec<f64>,
    /// `integral / area`: the area-weighted average.
    pub average: Vec<f64>,
    pub min: Vec<f64>,
    pub max: Vec<f64>,
}

impl PatchStats {
    pub(crate) fn of(values: &FieldValues, areas: &[f64]) -> Option<Self> {
        let w = Weighted::of(values, areas)?;
        Some(Self {
            faces: w.count,
            area: w.weight,
            average: w.average(),
            integral: w.integral,
            min: w.min,
            max: w.max,
        })
    }
}

/// A field over the whole domain.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeStats {
    pub cells: usize,
    pub volume: f64,
    /// `∑ value × cell volume`.
    pub integral: Vec<f64>,
    /// `integral / volume`: the volume-weighted average.
    pub average: Vec<f64>,
    pub min: Vec<f64>,
    pub max: Vec<f64>,
}

impl VolumeStats {
    pub(crate) fn of(values: &FieldValues, volumes: &[f64]) -> Option<Self> {
        let w = Weighted::of(values, volumes)?;
        Some(Self {
            cells: w.count,
            volume: w.weight,
            average: w.average(),
            integral: w.integral,
            min: w.min,
            max: w.max,
        })
    }
}
