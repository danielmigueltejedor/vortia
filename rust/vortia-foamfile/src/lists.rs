//! Fast readers for the large lists in a mesh: labels, faces and points.
//!
//! These read straight from the tokenizer instead of building a [`Value`]
//! tree, which for a mesh of a few million cells is the difference between
//! a few hundred megabytes and a few gigabytes.
//!
//! [`Value`]: crate::dict::Value

use vortia_core::Vec3;

use crate::error::{FoamError, Result};
use crate::token::{Token, TokenKind, Tokenizer};

fn label(t: &Token) -> Result<usize> {
    match t.kind {
        TokenKind::Label(v) => {
            usize::try_from(v).map_err(|_| FoamError::parse(t.line, format!("negative label {v}")))
        }
        _ => Err(FoamError::parse(
            t.line,
            format!("expected a label, found {}", t.describe()),
        )),
    }
}

fn scalar(t: &Token) -> Result<f64> {
    t.as_scalar().ok_or_else(|| {
        FoamError::parse(t.line, format!("expected a number, found {}", t.describe()))
    })
}

/// A list header: the declared length, and whether it is `N{v}` (uniform).
fn list_start(tok: &mut Tokenizer<'_>) -> Result<(usize, bool)> {
    let t = tok.expect_any("a list length")?;
    let n = label(&t)?;
    let open = tok.expect_any("'(' or '{'")?;
    match open.kind {
        TokenKind::Punct(b'(') => Ok((n, false)),
        TokenKind::Punct(b'{') => Ok((n, true)),
        _ => Err(FoamError::parse(
            open.line,
            format!("expected '(' or '{{', found {}", open.describe()),
        )),
    }
}

/// Capacity to reserve for `n` items: never more than the bytes left could hold.
fn capacity(n: usize, tok: &Tokenizer<'_>, min_bytes_per_item: usize) -> usize {
    n.min(tok.remaining() / min_bytes_per_item.max(1) + 1)
}

/// A uniform list longer than the input is a lie or a bomb; refuse it.
fn uniform_len(n: usize, line: u32) -> Result<usize> {
    const MAX: usize = 1 << 28;
    if n > MAX {
        return Err(FoamError::parse(line, format!("uniform list of {n} items")));
    }
    Ok(n)
}

/// `labelList`: owner, neighbour, cell zones.
pub fn read_labels(tok: &mut Tokenizer<'_>) -> Result<Vec<usize>> {
    let line = tok.line();
    let (n, uniform) = list_start(tok)?;
    if uniform {
        let v = label(&tok.expect_any("a label")?)?;
        tok.expect_punct(b'}')?;
        return Ok(vec![v; uniform_len(n, line)?]);
    }
    let mut out = Vec::with_capacity(capacity(n, tok, 2));
    for _ in 0..n {
        out.push(label(&tok.expect_any("a label")?)?);
    }
    tok.expect_punct(b')')?;
    Ok(out)
}

/// Faces in compressed-row form: face `i` is
/// `points[offsets[i]..offsets[i + 1]]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FaceList {
    pub offsets: Vec<usize>,
    pub points: Vec<usize>,
}

impl FaceList {
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn face(&self, i: usize) -> &[usize] {
        match (self.offsets.get(i), self.offsets.get(i + 1)) {
            (Some(&a), Some(&b)) => self.points.get(a..b).unwrap_or_default(),
            _ => &[],
        }
    }
}

/// `faceList`: each face is `k(p0 p1 ... pk-1)`.
pub fn read_faces(tok: &mut Tokenizer<'_>) -> Result<FaceList> {
    let (n, uniform) = list_start(tok)?;
    if uniform {
        return Err(FoamError::parse(tok.line(), "a uniform face list"));
    }
    let mut faces = FaceList {
        offsets: Vec::with_capacity(capacity(n, tok, 8) + 1),
        points: Vec::with_capacity(capacity(n.saturating_mul(4), tok, 2)),
    };
    faces.offsets.push(0);
    for _ in 0..n {
        let count_tok = tok.expect_any("a face")?;
        let k = label(&count_tok)?;
        tok.expect_punct(b'(')?;
        for _ in 0..k {
            faces.points.push(label(&tok.expect_any("a point label")?)?);
        }
        tok.expect_punct(b')')?;
        faces.offsets.push(faces.points.len());
    }
    tok.expect_punct(b')')?;
    Ok(faces)
}

/// `vectorField`: points, or a nonuniform vector field's values.
pub fn read_vectors(tok: &mut Tokenizer<'_>) -> Result<Vec<Vec3>> {
    let line = tok.line();
    let (n, uniform) = list_start(tok)?;
    let read_one = |tok: &mut Tokenizer<'_>| -> Result<Vec3> {
        tok.expect_punct(b'(')?;
        let x = scalar(&tok.expect_any("x")?)?;
        let y = scalar(&tok.expect_any("y")?)?;
        let z = scalar(&tok.expect_any("z")?)?;
        tok.expect_punct(b')')?;
        Ok(Vec3::new(x, y, z))
    };
    if uniform {
        let v = read_one(tok)?;
        tok.expect_punct(b'}')?;
        return Ok(vec![v; uniform_len(n, line)?]);
    }
    let mut out = Vec::with_capacity(capacity(n, tok, 8));
    for _ in 0..n {
        out.push(read_one(tok)?);
    }
    tok.expect_punct(b')')?;
    Ok(out)
}

/// `scalarField`.
pub fn read_scalars(tok: &mut Tokenizer<'_>) -> Result<Vec<f64>> {
    let line = tok.line();
    let (n, uniform) = list_start(tok)?;
    if uniform {
        let v = scalar(&tok.expect_any("a number")?)?;
        tok.expect_punct(b'}')?;
        return Ok(vec![v; uniform_len(n, line)?]);
    }
    let mut out = Vec::with_capacity(capacity(n, tok, 2));
    for _ in 0..n {
        out.push(scalar(&tok.expect_any("a number")?)?);
    }
    tok.expect_punct(b')')?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_list_kind() {
        let mut t =
            Tokenizer::new(b"3(0 1 2) 2(3(0 1 2) 4(0 1 2 3)) 1((1 2.5 -3)) 4{7} 2{(1 0 0)}");
        assert_eq!(read_labels(&mut t).unwrap(), vec![0, 1, 2]);
        let f = read_faces(&mut t).unwrap();
        assert_eq!((f.len(), f.face(1)), (2, &[0, 1, 2, 3][..]));
        assert_eq!(
            read_vectors(&mut t).unwrap(),
            vec![Vec3::new(1.0, 2.5, -3.0)]
        );
        assert_eq!(read_labels(&mut t).unwrap(), vec![7; 4]);
        assert_eq!(read_vectors(&mut t).unwrap().len(), 2);
    }

    #[test]
    fn a_huge_declared_length_does_not_allocate_it() {
        let mut t = Tokenizer::new(b"4000000000000(1 2)");
        assert!(read_labels(&mut t).is_err());
        let mut t = Tokenizer::new(b"4000000000000{1}");
        assert!(read_labels(&mut t).is_err());
    }

    #[test]
    fn negative_labels_and_short_lists_are_errors() {
        assert!(read_labels(&mut Tokenizer::new(b"2(1 -1)")).is_err());
        assert!(read_labels(&mut Tokenizer::new(b"3(1 2)")).is_err());
        assert!(read_faces(&mut Tokenizer::new(b"1(3(1 2))")).is_err());
        assert!(read_vectors(&mut Tokenizer::new(b"1((1 2))")).is_err());
    }
}
