//! `as_i64` reads a FIX `int`, and FIX `int` has no `+` — plan
//! `docs/plans/2026-09-23-phase-3-found-defects.md`, row D4.
//!
//! Everything sits inside `mod int`, so a filter `int` matches `int::<name>`
//! rather than matching nothing and exiting 0; the gate runs `--test int` and
//! quotes `running N tests`.
//!
//! The grammar is the session's: `FieldType::Int.accepts` is the rule the
//! session validates an `int` field with (`crates/dict/src/field_type.rs`,
//! `signed_int`). `as_i64` must accept exactly what it accepts, except where
//! the digits are a number that does not fit an `i64` — that is `Overflow`,
//! not a refusal of the syntax — and everything it refuses is `NotANumber`. Same shape as `tests/decimal.rs`'s agreement
//! test for `as_decimal` and FIX float.
//!
//! No `proptest`: `codec` has no runtime dependency and adds no
//! dev-dependency here. The generated cases come from an exhaustive
//! enumeration plus a hand-written xorshift with a fixed seed, so every run
//! tests the same strings and a red is reproducible.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing
)]

mod int {
    use fixbolt_codec::{ConvertError, as_i64, as_u32};
    use fixbolt_dict::FieldType;

    use ConvertError::{NotANumber, Overflow};

    fn show(s: &[u8]) -> String {
        String::from_utf8_lossy(s).into_owned()
    }

    /// The session's own rule: an `int` field accepts exactly this.
    fn session_accepts(s: &[u8]) -> bool {
        FieldType::Int.accepts(s)
    }

    /// Where `as_i64` and the dictionary disagree, or `None`. Exact, both
    /// ways: the dictionary accepts ⇔ `as_i64` is `Ok` or `Overflow`; the
    /// dictionary refuses ⇒ `NotANumber`. A syntax fault wins over an
    /// overflow, as it does for `as_decimal` (ADR-0120), so `99…9x` is
    /// `NotANumber`, never `Overflow`.
    fn disagrees(s: &[u8]) -> Option<(String, bool, Result<i64, ConvertError>)> {
        let session = session_accepts(s);
        let r = as_i64(s);
        let wrong = match r {
            Ok(_) | Err(Overflow) => !session,
            Err(NotANumber) => session,
            Err(_) => true,
        };
        wrong.then(|| (show(s), session, r))
    }

    /// xorshift64, fixed seed.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn a_leading_plus_is_refused() {
        assert_eq!(
            as_i64(b"+200"),
            Err(NotANumber),
            "`+200` must be NotANumber, FIX int has no plus"
        );
        assert_eq!(
            as_i64(b"+0"),
            Err(NotANumber),
            "`+0` must be NotANumber, FIX int has no plus"
        );
        assert_eq!(
            as_i64(b"+"),
            Err(NotANumber),
            "`+` must be NotANumber, FIX int has no plus"
        );
        // A sign after a sign is no better.
        assert_eq!(as_i64(b"-+5"), Err(NotANumber));
        assert_eq!(as_i64(b"+-5"), Err(NotANumber));
    }

    #[test]
    fn as_u32_refuses_a_leading_plus_too() {
        // `as_u32` takes no sign at all; recorded here so the three readers'
        // answer to `+` is held in one place (`as_decimal`'s is in
        // `tests/decimal.rs`, `a_leading_plus_is_refused`).
        assert_eq!(as_u32(b"+200"), Err(NotANumber));
        assert_eq!(as_u32(b"+"), Err(NotANumber));
    }

    #[test]
    fn a_syntax_fault_wins_over_overflow() {
        // Found by the agreement test below: the reader stopped at the digit
        // that overflowed and never saw the trailing space.
        assert_eq!(
            as_i64(b"9410947898048986560 "),
            Err(NotANumber),
            "`9410947898048986560 ` must be NotANumber: a syntax fault wins over overflow"
        );
        assert_eq!(as_i64(b"-9378876394221107889x"), Err(NotANumber));
        assert_eq!(as_i64(b"99999999999999999999+"), Err(NotANumber));
        assert_eq!(
            as_u32(b"99999999999 "),
            Err(NotANumber),
            "`99999999999 ` must be NotANumber: a syntax fault wins over overflow"
        );
        assert_eq!(as_u32(b"4294967296x"), Err(NotANumber));
        // Digits only, too large: still Overflow.
        assert_eq!(as_u32(b"4294967296"), Err(Overflow));
        assert_eq!(as_u32(b"4294967295"), Ok(u32::MAX));
    }

    #[test]
    fn a_minus_and_leading_zeros_still_parse() {
        assert_eq!(as_i64(b"-5"), Ok(-5));
        assert_eq!(as_i64(b"-0"), Ok(0));
        assert_eq!(as_i64(b"00023"), Ok(23));
        assert_eq!(as_i64(b"9223372036854775807"), Ok(i64::MAX));
        assert_eq!(as_i64(b"-9223372036854775808"), Ok(i64::MIN));
        assert_eq!(as_i64(b"9223372036854775808"), Err(Overflow));
        assert_eq!(as_i64(b"-9223372036854775809"), Err(Overflow));
        assert_eq!(as_i64(b"-"), Err(NotANumber));
        assert_eq!(as_i64(b""), Err(NotANumber));
    }

    #[test]
    fn as_i64_agrees_with_the_dictionarys_int_rule() {
        // Exhaustive over a small alphabet: every string of length 0..=6 over
        // {0,1,9,-,+,.,space}.
        const ALPHABET: &[u8] = b"019-+. ";
        let mut checked = 0usize;
        let mut accepted = 0usize;
        let mut bad: Vec<(String, bool, Result<i64, ConvertError>)> = Vec::new();
        let mut s = Vec::with_capacity(6);
        for len in 0..=6u32 {
            for mut code in 0..ALPHABET.len().pow(len) {
                s.clear();
                for _ in 0..len {
                    s.push(ALPHABET[code % ALPHABET.len()]);
                    code /= ALPHABET.len();
                }
                let session = session_accepts(&s);
                checked += 1;
                accepted += usize::from(session);
                if let Some(d) = disagrees(&s)
                    && bad.len() < 10
                {
                    bad.push(d);
                }
            }
        }

        // Long values, 1..=20 bytes, from a fixed seed: digits mostly, with a
        // sign in front sometimes and a noise byte sometimes, so both the
        // `i64` boundary (19–20 digits) and every refusal are reached.
        const NOISE: &[u8] = b"-+. x";
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let (mut oks, mut overflows, mut refused) = (0usize, 0usize, 0usize);
        for _ in 0..30_000 {
            // Half the values sit at the `i64` boundary (18–20 digits), so
            // overflow is reached as often as a clean parse.
            let len = if rng.below(2) == 0 {
                1 + rng.below(20) as usize
            } else {
                18 + rng.below(3) as usize
            };
            let mut v = Vec::with_capacity(len + 1);
            match rng.below(4) {
                0 => v.push(b'-'),
                1 => v.push(b'+'),
                _ => {}
            }
            for _ in 0..len {
                v.push(b'0' + rng.below(10) as u8);
            }
            if rng.below(4) == 0 {
                let at = rng.below(v.len() as u64) as usize;
                v[at] = NOISE[rng.below(NOISE.len() as u64) as usize];
            }
            let session = session_accepts(&v);
            let r = as_i64(&v);
            match r {
                Ok(_) => oks += 1,
                Err(Overflow) => overflows += 1,
                Err(_) => refused += 1,
            }
            checked += 1;
            accepted += usize::from(session);
            if let Some(d) = disagrees(&v)
                && bad.len() < 10
            {
                bad.push(d);
            }
        }

        println!(
            "{checked} strings checked, {accepted} accepted by the dictionary; \
             long values: {oks} parsed, {overflows} overflowed, {refused} refused"
        );
        assert_eq!(checked, 137_257 + 30_000);
        assert!(accepted > 0);
        assert!(oks > 1000 && overflows > 1000 && refused > 1000);
        assert!(
            bad.is_empty(),
            "FieldType::Int.accepts(s) must equal as_i64(s) is Ok or Overflow, and a refusal must be NotANumber: {bad:?}"
        );
    }
}
