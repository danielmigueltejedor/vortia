//! Truncated and corrupted input must give errors, never panics.

use vortia_foamfile::{Dictionary, Tokenizer, lists, polymesh};

const DICT: &str = "FoamFile { format ascii; class dictionary; }\n\
    a 1; b (1 2 (3 4)); c { d [0 1 -1 0 0 0 0]; e 2{0.5}; } f \"s\\\"q\"; #include \"x\"\n";
const FACES: &str = "2(3(0 1 2) 4(0 1 2 3))";
const POINTS: &str = "2((0 0 0) (1 -2.5e-3 3))";
const BOUNDARY: &str = "1( walls { type wall; inGroups 1(wall); nFaces 3; startFace 0; } )";

fn mutations(src: &str) -> Vec<Vec<u8>> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    for cut in 0..bytes.len() {
        out.push(bytes[..cut].to_vec());
        for b in [b'(', b')', b'{', b'}', b'"', b';', b'-', b'9', 0xff] {
            let mut m = bytes.to_vec();
            m[cut] = b;
            out.push(m);
        }
    }
    out
}

#[test]
fn every_truncation_and_byte_flip_is_survived() {
    for m in mutations(DICT) {
        let _ = Dictionary::parse(&mut Tokenizer::new(&m), false, 0);
    }
    for m in mutations(FACES) {
        let _ = lists::read_faces(&mut Tokenizer::new(&m));
    }
    for m in mutations(POINTS) {
        let _ = lists::read_vectors(&mut Tokenizer::new(&m));
    }
    for m in mutations(BOUNDARY) {
        let _ = polymesh::read_boundary(&mut Tokenizer::new(&m));
    }
}

#[test]
fn the_unmutated_inputs_parse() {
    assert!(Dictionary::parse(&mut Tokenizer::new(DICT.as_bytes()), false, 0).is_ok());
    assert_eq!(
        lists::read_faces(&mut Tokenizer::new(FACES.as_bytes()))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        lists::read_vectors(&mut Tokenizer::new(POINTS.as_bytes()))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        polymesh::read_boundary(&mut Tokenizer::new(BOUNDARY.as_bytes()))
            .unwrap()
            .len(),
        1
    );
}
