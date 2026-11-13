//! Binary case files read to the same values as the ASCII ones.
//!
//! `tests/data/cavity-binary` is OpenFOAM v2606's icoFoam cavity (4×3 cells,
//! two time steps) written with `writeFormat binary`; `cavity-ascii` is the
//! same case after `foamFormatConvert` with `writeFormat ascii` and
//! `writePrecision 17`, so every number round-trips exactly. The binary mesh
//! has its faces as a `faceCompactList`, the ASCII one as a `faceList`.

use std::path::{Path, PathBuf};

use vortia_foamfile::{FoamFile, Format, RawPolyMesh, Tokenizer, Value, lists, polymesh};

fn data(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(case)
}

/// Every number in a value, in order: ASCII writes `0` as a label where
/// binary has the scalar 0.0, so values are compared as numbers.
fn numbers(v: &Value, out: &mut Vec<f64>) {
    match v {
        Value::Label(n) => out.push(*n as f64),
        Value::Scalar(x) => out.push(*x),
        Value::List(items) | Value::Dimensions(items) => {
            items.iter().for_each(|i| numbers(i, out));
        }
        Value::Dict(d) | Value::Keyed(_, d) => {
            for (_, e) in d.entries() {
                if let vortia_foamfile::Entry::Values(vs) = e {
                    vs.iter().for_each(|i| numbers(i, out));
                } else if let vortia_foamfile::Entry::Dict(sub) = e {
                    numbers(&Value::Dict(sub.clone()), out);
                }
            }
        }
        _ => {}
    }
}

fn field_numbers(path: &Path) -> (Format, Vec<f64>) {
    let (header, dict) = FoamFile::open(path).unwrap().dictionary().unwrap();
    let mut out = Vec::new();
    numbers(&Value::Dict(dict), &mut out);
    (header.unwrap().format, out)
}

#[test]
fn the_binary_mesh_is_the_ascii_mesh() {
    let binary = RawPolyMesh::read_case(data("cavity-binary")).unwrap();
    let ascii = RawPolyMesh::read_case(data("cavity-ascii")).unwrap();
    assert_eq!(binary.points.len(), 40);
    assert_eq!(binary.faces.len(), 55);
    assert_eq!(binary, ascii);
}

#[test]
fn binary_fields_hold_the_ascii_values_bit_for_bit() {
    for field in ["U", "p", "phi"] {
        let (bf, b) = field_numbers(&data("cavity-binary/0.01").join(field));
        let (af, a) = field_numbers(&data("cavity-ascii/0.01").join(field));
        assert_eq!((bf, af), (Format::Binary, Format::Ascii), "{field}");
        assert!(b.len() > 12, "{field}: only {} numbers", b.len());
        assert_eq!(b.len(), a.len(), "{field}");
        for (i, (x, y)) in b.iter().zip(&a).enumerate() {
            assert_eq!(x.to_bits(), y.to_bits(), "{field}, number {i}: {x} and {y}");
        }
    }
}

#[test]
fn a_compressed_binary_field_reads_the_same() {
    let (_, gz) = field_numbers(&data("cavity-binary-gz/0.01/U"));
    let (_, plain) = field_numbers(&data("cavity-binary/0.01/U"));
    assert_eq!(gz, plain);
}

/// A binary file holding `body`, laid out as `arch` says.
fn binary_file(arch: &str, class: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "FoamFile {{ version 2.0; format binary; arch \"{arch}\"; class {class}; object x; }}\n"
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

fn list(n: usize, raw: &[u8]) -> Vec<u8> {
    let mut out = format!("\n{n}\n(").into_bytes();
    out.extend_from_slice(raw);
    out.extend_from_slice(b")\n");
    out
}

fn read_list<T>(bytes: &[u8], read: impl FnOnce(&mut Tokenizer<'_>) -> T) -> T {
    let file = FoamFile::from_bytes("x", bytes.to_vec());
    let (_, mut tok) = file.tokens().unwrap();
    read(&mut tok)
}

#[test]
fn big_endian_and_wide_labels_read_too() {
    let raw: Vec<u8> = [3i64, 1, 4].iter().flat_map(|v| v.to_be_bytes()).collect();
    let bytes = binary_file("MSB;label=64;scalar=32", "labelList", &list(3, &raw));
    assert_eq!(
        read_list(&bytes, lists::read_labels).unwrap(),
        vec![3, 1, 4]
    );

    let raw: Vec<u8> = [0.5f32, -2.0, 8.0]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    let bytes = binary_file("MSB;label=64;scalar=32", "vectorField", &list(1, &raw));
    let v = read_list(&bytes, lists::read_vectors).unwrap();
    assert_eq!((v[0].x, v[0].y, v[0].z), (0.5, -2.0, 8.0));
}

#[test]
fn an_empty_binary_list_may_have_no_parentheses() {
    let bytes = binary_file(
        "LSB;label=32;scalar=64",
        "volScalarField",
        b"internalField nonuniform List<scalar> 0;\nother nonuniform List<scalar> 0();\n",
    );
    let (_, d) = FoamFile::from_bytes("x", bytes).dictionary().unwrap();
    for key in ["internalField", "other"] {
        assert_eq!(
            d.values(key).unwrap().last(),
            Some(&Value::List(Vec::new())),
            "{key}"
        );
    }
}

#[test]
fn binary_lists_that_lie_about_their_length_are_errors() {
    let raw: Vec<u8> = [1i32, 2].iter().flat_map(|v| v.to_le_bytes()).collect();
    for n in [1, 3, 1 << 40] {
        let bytes = binary_file("LSB;label=32;scalar=64", "labelList", &list(n, &raw));
        assert!(read_list(&bytes, lists::read_labels).is_err(), "{n}");
    }
    // Faces whose offsets do not end at the number of points.
    let offsets: Vec<u8> = [0i32, 3, 5].iter().flat_map(|v| v.to_le_bytes()).collect();
    let points: Vec<u8> = [0i32, 1, 2, 3]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let mut body = list(3, &offsets);
    body.extend(list(4, &points));
    let bytes = binary_file("LSB;label=32;scalar=64", "faceCompactList", &body);
    assert!(read_list(&bytes, lists::read_compact_faces).is_err());
}

#[test]
fn every_truncation_of_a_binary_mesh_file_is_survived() {
    for name in ["faces", "points", "owner"] {
        let path = data("cavity-binary/constant/polyMesh").join(name);
        let bytes = std::fs::read(&path).unwrap();
        for cut in 0..bytes.len() {
            let file = FoamFile::from_bytes(&path, bytes[..cut].to_vec());
            if let Ok((_, mut tok)) = file.tokens() {
                let _ = lists::read_compact_faces(&mut tok);
            }
            let file = FoamFile::from_bytes(&path, bytes[..cut].to_vec());
            if let Ok((_, mut tok)) = file.tokens() {
                let _ = lists::read_vectors(&mut tok);
            }
        }
    }
    let _ = polymesh::read_boundary;
}

/// A larger check against a case on this machine, when one is named:
/// `VORTIA_BINARY_ORACLE=<binary case>:<ascii case>` (the mesh and every
/// field of the latest time in both).
#[test]
fn an_oracle_case_named_in_the_environment() {
    let Ok(pair) = std::env::var("VORTIA_BINARY_ORACLE") else {
        return;
    };
    let (bin, asc) = pair.split_once(':').unwrap();
    assert_eq!(
        RawPolyMesh::read_case(bin).unwrap(),
        RawPolyMesh::read_case(asc).unwrap()
    );
    let time = std::env::var("VORTIA_BINARY_ORACLE_TIME").unwrap_or_else(|_| "0".into());
    for entry in std::fs::read_dir(Path::new(bin).join(&time)).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap();
        let (_, b) = field_numbers(&path);
        let (_, a) = field_numbers(&Path::new(asc).join(&time).join(name));
        assert_eq!(b.len(), a.len(), "{}", path.display());
        for (x, y) in b.iter().zip(&a) {
            assert_eq!(x.to_bits(), y.to_bits(), "{}", path.display());
        }
    }
}
