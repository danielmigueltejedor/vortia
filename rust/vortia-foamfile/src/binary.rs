//! OpenFOAM's binary format: the lists of a `format binary;` file.
//!
//! Only lists of fixed-size items (labels, scalars and the vector and tensor
//! types made of scalars) are binary; everything else in the file stays text.
//! Such a list is written `N` `(` then `N` items as raw bytes, then `)`. The
//! header's `arch` entry says how: byte order, and the width of a label and
//! of a scalar.

use crate::error::{FoamError, Result};

/// How a binary file lays out its numbers: `arch "LSB;label=32;scalar=64"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arch {
    pub big_endian: bool,
    /// Bytes per label: 4 or 8.
    pub label_bytes: usize,
    /// Bytes per scalar: 4 or 8.
    pub scalar_bytes: usize,
}

impl Default for Arch {
    /// What OpenFOAM assumes when a binary file names no `arch`.
    fn default() -> Self {
        Self {
            big_endian: false,
            label_bytes: 4,
            scalar_bytes: 8,
        }
    }
}

impl Arch {
    /// Parses an `arch` entry; parts it leaves out keep OpenFOAM's defaults.
    pub fn parse(text: &str) -> Result<Self> {
        let mut arch = Self::default();
        let bad = || FoamError::Unsupported(format!("binary files with arch '{text}'"));
        for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            match part.split_once('=') {
                None if part == "LSB" => arch.big_endian = false,
                None if part == "MSB" => arch.big_endian = true,
                Some(("label", bits)) => arch.label_bytes = width(bits).ok_or_else(bad)?,
                Some(("scalar", bits)) => arch.scalar_bytes = width(bits).ok_or_else(bad)?,
                _ => return Err(bad()),
            }
        }
        Ok(arch)
    }

    /// Decodes `raw` as labels.
    pub fn labels(&self, raw: &[u8]) -> Vec<i64> {
        raw.chunks_exact(self.label_bytes)
            .map(|b| match *b {
                [b0, b1, b2, b3] => {
                    let a = [b0, b1, b2, b3];
                    i64::from(if self.big_endian {
                        i32::from_be_bytes(a)
                    } else {
                        i32::from_le_bytes(a)
                    })
                }
                [b0, b1, b2, b3, b4, b5, b6, b7] => {
                    let a = [b0, b1, b2, b3, b4, b5, b6, b7];
                    if self.big_endian {
                        i64::from_be_bytes(a)
                    } else {
                        i64::from_le_bytes(a)
                    }
                }
                _ => 0,
            })
            .collect()
    }

    /// Decodes `raw` as scalars.
    pub fn scalars(&self, raw: &[u8]) -> Vec<f64> {
        raw.chunks_exact(self.scalar_bytes)
            .map(|b| match *b {
                [b0, b1, b2, b3] => {
                    let a = [b0, b1, b2, b3];
                    f64::from(if self.big_endian {
                        f32::from_be_bytes(a)
                    } else {
                        f32::from_le_bytes(a)
                    })
                }
                [b0, b1, b2, b3, b4, b5, b6, b7] => {
                    let a = [b0, b1, b2, b3, b4, b5, b6, b7];
                    if self.big_endian {
                        f64::from_be_bytes(a)
                    } else {
                        f64::from_le_bytes(a)
                    }
                }
                _ => 0.0,
            })
            .collect()
    }
}

fn width(bits: &str) -> Option<usize> {
    match bits {
        "32" => Some(4),
        "64" => Some(8),
        _ => None,
    }
}

/// What one item of a `List<T>` is in binary, for the `T` a field names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Label,
    /// A scalar, or a fixed number of them (a vector is 3, a tensor 9).
    Scalars(usize),
}

impl Item {
    /// The item for `List<T>`, if `T` is written in binary at all.
    pub fn of_list(word: &str) -> Option<Self> {
        let inner = word.strip_prefix("List<")?.strip_suffix('>')?;
        Some(match inner {
            "label" => Item::Label,
            "scalar" => Item::Scalars(1),
            "sphericalTensor" => Item::Scalars(1),
            "vector" => Item::Scalars(3),
            "symmTensor" => Item::Scalars(6),
            "tensor" => Item::Scalars(9),
            _ => return None,
        })
    }

    pub fn bytes(&self, arch: &Arch) -> usize {
        match self {
            Item::Label => arch.label_bytes,
            Item::Scalars(n) => n * arch.scalar_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_arch_openfoam_writes() {
        assert_eq!(
            Arch::parse("LSB;label=32;scalar=64").unwrap(),
            Arch::default()
        );
        let a = Arch::parse("MSB;label=64;scalar=32").unwrap();
        assert_eq!((a.big_endian, a.label_bytes, a.scalar_bytes), (true, 8, 4));
        assert!(Arch::parse("LSB;label=16").is_err());
        assert!(Arch::parse("PDP").is_err());
    }

    #[test]
    fn decodes_both_byte_orders_and_widths() {
        let lsb = Arch::default();
        let mut raw = 7i32.to_le_bytes().to_vec();
        raw.extend((-2i32).to_le_bytes());
        assert_eq!(lsb.labels(&raw), vec![7, -2]);
        assert_eq!(lsb.scalars(&1.5f64.to_le_bytes()), vec![1.5]);
        let msb = Arch::parse("MSB;label=64;scalar=32").unwrap();
        assert_eq!(msb.labels(&9i64.to_be_bytes()), vec![9]);
        assert_eq!(msb.scalars(&(-0.25f32).to_be_bytes()), vec![-0.25]);
    }

    #[test]
    fn knows_which_lists_are_binary() {
        assert_eq!(Item::of_list("List<vector>"), Some(Item::Scalars(3)));
        assert_eq!(Item::of_list("List<label>"), Some(Item::Label));
        assert_eq!(Item::of_list("List<word>"), None);
        assert_eq!(Item::of_list("labelList"), None);
    }
}
