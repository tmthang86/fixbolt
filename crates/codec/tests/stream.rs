//! The closing condition for step 1: the API used the way `engine` will use it.
//!
//! Real messages arrive as a byte stream, not as messages. A read returns
//! whatever the kernel has — half a field, three messages, one byte. The parser
//! has to recover every message exactly once, in order, and say `Incomplete`
//! for every prefix without ever mistaking one for a short message.
//!
//! Chunk sizes come from a fixed xorshift, so a failure is reproducible: the
//! seed is in the loop below and nothing else varies.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

mod common;

use fixbolt_codec::{FieldIndex, Parsed, Validation, parse_into};
use fixbolt_dict::Fix44;

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn upto(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize + 1
    }
}

/// Every `.def` line the parser accepts, as its own message.
fn corpus() -> Vec<Vec<u8>> {
    let mut idx: FieldIndex<64> = FieldIndex::new();
    common::load_all()
        .into_iter()
        .filter(|l| {
            parse_into::<Fix44, 64>(&l.wire, &mut idx, Validation::NONE)
                == Ok(Parsed::Complete {
                    consumed: l.wire.len(),
                })
        })
        .map(|l| l.wire)
        .collect()
}

fn drive(messages: &[Vec<u8>], seed: u64) -> Vec<Vec<u8>> {
    let stream: Vec<u8> = messages.iter().flatten().copied().collect();
    let mut rng = Xorshift(seed);
    let mut idx: FieldIndex<64> = FieldIndex::new();

    // Exactly the shape the engine will have: one buffer, one index, compact
    // after each drain. Nothing is allocated per message.
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut recovered: Vec<Vec<u8>> = Vec::new();
    let mut fed = 0usize;

    while fed < stream.len() || !buf.is_empty() {
        if fed < stream.len() {
            // A "read" of between 1 byte and about two messages.
            let n = rng.upto(300).min(stream.len() - fed);
            buf.extend_from_slice(&stream[fed..fed + n]);
            fed += n;
        } else if buf.is_empty() {
            break;
        }

        let mut consumed_total = 0usize;
        loop {
            match parse_into::<Fix44, 64>(&buf[consumed_total..], &mut idx, Validation::NONE) {
                Ok(Parsed::Complete { consumed }) => {
                    recovered.push(buf[consumed_total..consumed_total + consumed].to_vec());
                    consumed_total += consumed;
                }
                Ok(Parsed::Incomplete) => break,
                Err(e) => panic!(
                    "the stream contains only messages that parse alone, but the reader hit {e:?} \
                     after {} messages at offset {consumed_total}",
                    recovered.len()
                ),
            }
        }
        buf.drain(..consumed_total);

        if fed >= stream.len() && consumed_total == 0 {
            break; // nothing left that can complete
        }
    }
    assert!(buf.is_empty(), "{} bytes left unparsed", buf.len());
    recovered
}

#[test]
fn every_message_comes_back_exactly_once_whatever_the_chunking() {
    let messages = corpus();
    assert_eq!(messages.len(), 533);

    // Five different chunk patterns. A single seed can get lucky about where
    // boundaries fall; five cannot, and each is reproducible.
    for seed in [0x2026_0828, 1, 0xdead_beef, 0x5555_5555, 7] {
        let got = drive(&messages, seed);
        assert_eq!(
            got.len(),
            messages.len(),
            "seed {seed:#x}: recovered {} of {}",
            got.len(),
            messages.len()
        );
        for (i, (a, b)) in got.iter().zip(messages.iter()).enumerate() {
            assert_eq!(a, b, "seed {seed:#x}: message {i} came back different");
        }
    }
    println!(
        "{} messages recovered intact under 5 chunk patterns",
        messages.len()
    );
}

#[test]
fn one_byte_at_a_time_is_the_worst_case_and_still_works() {
    let messages = corpus();
    let stream: Vec<u8> = messages.iter().flatten().copied().collect();
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut n = 0usize;

    for &b in &stream {
        buf.push(b);
        // Every single-byte step must be Incomplete until the message ends.
        match parse_into::<Fix44, 64>(&buf, &mut idx, Validation::NONE) {
            Ok(Parsed::Complete { consumed }) => {
                assert_eq!(consumed, buf.len(), "consumed a partial buffer");
                assert_eq!(buf, messages[n], "message {n} differs");
                n += 1;
                buf.clear();
            }
            Ok(Parsed::Incomplete) => {}
            Err(e) => panic!("byte-at-a-time hit {e:?} on message {n}"),
        }
    }
    assert!(buf.is_empty());
    assert_eq!(n, messages.len());
    println!("{n} messages recovered one byte at a time");
}
