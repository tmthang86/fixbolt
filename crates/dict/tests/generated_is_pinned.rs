//! The generated tables are pinned by their SHA-256.
//!
//! `tests/gen_matches_build.rs` compares the `codegen` library with what
//! `build.rs` wrote — but `build.rs` loads the same `src/codegen/` by
//! `#[path]`, so that is the generator compared with itself: a change to what
//! the generator emits moves both sides and stays green. This file compares
//! the output with a constant instead. The constants are what `shasum -a 256`
//! prints for `$OUT_DIR/fix44.rs` and `$OUT_DIR/fixt11_fix50sp2.rs`, the same
//! numbers the step-18 commit body recorded for the move out of `build.rs`.
//!
//! The build-script half needs no feature, so it runs under every
//! `cargo test`; the library half needs `codegen`, the pair needs `fix50sp2`.
//! A build under a `NANOFIX_*_XML` override generates from another file and
//! is red here by design.
//!
//! SHA-256 is written out below (FIPS 180-4) rather than taken from a crate:
//! the dependency would be a test's only reason to exist.
// A test file, not a library crate's source: an index that panics here is a
// failing test. The compression function reads its schedule by index.
#![allow(clippy::indexing_slicing, clippy::panic)]

const FIX44_SHA256: &str = "323eafd44ffc85d92f25e8e0db295d3d8289e139324b32dff7d9e2e6834f132d";
#[cfg(feature = "fix50sp2")]
const PAIR_SHA256: &str = "deeb5016e7ea37dd5b3f862baf2a8be9efc60cf4d96033c7726baf91dc778a1d";

const BUILT_FIX44: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fix44.rs"));

fn assert_pinned(what: &str, bytes: &[u8], want: &str) {
    let got = sha256_hex(bytes);
    assert_eq!(
        got, want,
        "{what}: the generated tables changed (sha256 {got}, pinned {want}). \
         If that is intended, update the constant in tests/generated_is_pinned.rs \
         and say why in the commit body."
    );
}

#[test]
fn the_built_fix44_tables_are_pinned() {
    assert_pinned("$OUT_DIR/fix44.rs", BUILT_FIX44, FIX44_SHA256);
}

#[cfg(feature = "fix50sp2")]
#[test]
fn the_built_pair_tables_are_pinned() {
    const BUILT_PAIR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixt11_fix50sp2.rs"));
    assert_pinned("$OUT_DIR/fixt11_fix50sp2.rs", BUILT_PAIR, PAIR_SHA256);
}

#[cfg(feature = "codegen")]
#[test]
fn the_library_fix44_tables_are_pinned() {
    let generated = fixbolt_dict::codegen::fix44_tables(include_str!("../spec/FIX44.xml"));
    match generated {
        Ok(text) => assert_pinned("codegen::fix44_tables", text.as_bytes(), FIX44_SHA256),
        Err(e) => panic!("codegen::fix44_tables refused the shipped FIX44.xml: {e}"),
    }
}

#[cfg(all(feature = "codegen", feature = "fix50sp2"))]
#[test]
fn the_library_pair_tables_are_pinned() {
    let generated = fixbolt_dict::codegen::fixt11_fix50sp2_tables(
        include_str!("../spec/FIXT11.xml"),
        include_str!("../spec/FIX50SP2.xml"),
    );
    match generated {
        Ok(text) => assert_pinned(
            "codegen::fixt11_fix50sp2_tables",
            text.as_bytes(),
            PAIR_SHA256,
        ),
        Err(e) => panic!("codegen::fixt11_fix50sp2_tables refused the shipped pair: {e}"),
    }
}

/// The hash is checked against FIPS 180-4's own examples before it is
/// trusted with anything else.
#[test]
fn sha256_matches_the_standards_examples() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let (blocks, rest) = msg.as_chunks::<64>();
    assert!(rest.is_empty(), "padding left {} stray bytes", rest.len());
    for block in blocks {
        let mut w = [0u32; 64];
        let (words, _) = block.as_chunks::<4>();
        for (i, word) in words.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
