//! `Decimal`, `as_decimal` and `Decimal::format` — ADR-0120, plan
//! `docs/plans/2026-09-23-p3-decimal.md`, table *Test phải viết trước*.
//!
//! Everything sits inside `mod decimal`, so ADR-0097's gate filter
//! `cargo test -p fixbolt-codec decimal` matches `decimal::<name>` here rather
//! than matching nothing and exiting 0.
//!
//! No `proptest`: `codec` takes no dependency, dev-dependencies included. The
//! generated cases come from a hand-written xorshift with fixed seeds, so every
//! run tests the same strings and a red is reproducible.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing
)]

mod common;

mod decimal {
    use fixbolt_codec::{
        ConvertError, Decimal, FieldIndex, NoDict, Parsed, Validation, as_decimal, parse_into,
    };
    use fixbolt_dict::{FieldType, Fix44};

    use ConvertError::{NotANumber, Overflow};

    fn parse(s: &[u8]) -> Result<Decimal, ConvertError> {
        as_decimal(s)
    }

    fn ok(s: &[u8]) -> (i64, i8) {
        let d = as_decimal(s)
            .unwrap_or_else(|e| panic!("{:?} must parse, got {e:?}", String::from_utf8_lossy(s)));
        (d.mantissa(), d.exponent())
    }

    fn fmt(d: Decimal) -> Vec<u8> {
        let mut buf = [0u8; Decimal::MAX_LEN];
        d.format(&mut buf).to_vec()
    }

    fn show(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    fn zeros(n: usize) -> Vec<u8> {
        vec![b'0'; n]
    }

    /// The session's own rule: a float field accepts exactly this.
    fn session_accepts(s: &[u8]) -> bool {
        FieldType::Price.accepts(s)
    }

    /// ADR-0120 decision 3, the one sentence that binds the two grammars.
    fn codec_accepts(s: &[u8]) -> bool {
        matches!(as_decimal(s), Ok(_) | Err(Overflow))
    }

    /// xorshift64, fixed seed per test.
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
        fn range(&mut self, lo: usize, hi_inclusive: usize) -> usize {
            lo + self.below((hi_inclusive - lo + 1) as u64) as usize
        }
        fn digit(&mut self) -> u8 {
            b'0' + self.below(10) as u8
        }
        fn nonzero_digit(&mut self) -> u8 {
            b'1' + self.below(9) as u8
        }
    }

    // ---- reading -------------------------------------------------------

    #[test]
    fn parses_a_plain_price() {
        assert_eq!(ok(b"12345.6789"), (123_456_789, -4));
    }

    #[test]
    fn keeps_trailing_zeros_in_the_exponent() {
        assert_eq!(ok(b"1.50"), (150, -2));
        assert_eq!(ok(b"1.5"), (15, -1));
        assert_ne!(
            parse(b"1.50"),
            parse(b"1.5"),
            "equality is structural: 1.50 and 1.5 are different bytes on the wire"
        );
    }

    #[test]
    fn an_integer_has_exponent_zero() {
        assert_eq!(ok(b"23"), (23, 0));
    }

    #[test]
    fn a_trailing_point_is_exponent_zero() {
        assert_eq!(ok(b"23."), (23, 0));
    }

    #[test]
    fn a_leading_point_is_accepted() {
        assert_eq!(ok(b".5"), (5, -1));
        assert_eq!(ok(b"-.5"), (-5, -1));
    }

    #[test]
    fn leading_zeros_are_accepted_and_do_not_overflow() {
        assert_eq!(ok(b"002000.00"), (200_000, -2));
        assert_eq!(ok(&cat(&[&zeros(30), b"1"])), (1, 0));
    }

    #[test]
    fn negative_zero_parses_to_zero() {
        assert_eq!(ok(b"-0"), (0, 0));
        assert_eq!(ok(b"-0.00"), (0, -2));
    }

    #[test]
    fn the_largest_mantissa_parses() {
        assert_eq!(ok(b"9223372036854775807"), (i64::MAX, 0));
        assert_eq!(ok(b"-9223372036854775807"), (-i64::MAX, 0));
        assert_eq!(ok(b"922337203685477.5807"), (i64::MAX, -4));
    }

    #[test]
    fn fifteen_significant_digits_parse_exactly() {
        assert_eq!(ok(b"123456789012345"), (123_456_789_012_345, 0));
        assert_eq!(ok(b"1234567890.12345"), (123_456_789_012_345, -5));
    }

    #[test]
    fn a_fraction_of_128_digits_parses() {
        let s = cat(&[b"0.", &zeros(127), b"1"]);
        assert_eq!(ok(&s), (1, -128));
    }

    #[test]
    fn a_leading_plus_is_refused() {
        // The exact value `14f_IncorrectDataFormat.def` sends and expects
        // refused with `373=6`. `as_i64` takes a `+`; this must not.
        assert_eq!(parse(b"+200.00"), Err(NotANumber));
    }

    #[test]
    fn exponent_notation_is_refused() {
        for s in [&b"1E3"[..], b"1e3", b"1.5e-3"] {
            assert_eq!(parse(s), Err(NotANumber), "{}", show(s));
        }
    }

    #[test]
    fn empty_and_lone_signs_are_refused() {
        for s in [&b""[..], b"-", b".", b"-."] {
            assert_eq!(parse(s), Err(NotANumber), "{:?}", show(s));
        }
    }

    #[test]
    fn a_second_point_is_refused() {
        assert_eq!(parse(b"1.2.3"), Err(NotANumber));
    }

    #[test]
    fn whitespace_and_other_bytes_are_refused() {
        for s in [
            &b" 1"[..],
            b"1 ",
            b"1,000",
            b"0x1F",
            b"NaN",
            b"inf",
            b"1-",
            b"--1",
        ] {
            assert_eq!(parse(s), Err(NotANumber), "{:?}", show(s));
        }
    }

    #[test]
    fn nineteen_digits_past_i64_overflow() {
        for s in [
            &b"9223372036854775808"[..],
            b"-9223372036854775808",
            b"9999999999999999999",
        ] {
            assert_eq!(parse(s), Err(Overflow), "{}", show(s));
        }
    }

    #[test]
    fn a_bad_byte_after_an_overflow_is_not_a_number() {
        // Syntax is judged over the whole value before size is, so the answer
        // agrees with `FieldType::accepts`, which refuses this string.
        let s = cat(&[&[b'9'; 25], b"x"]);
        assert_eq!(parse(&s), Err(NotANumber));
    }

    #[test]
    fn trailing_zeros_count_toward_overflow() {
        // Refused, never rounded to `1`: the trailing zeros are the scale.
        let s = cat(&[b"1.", &zeros(21)]);
        assert_eq!(parse(&s), Err(Overflow));
    }

    #[test]
    fn a_fraction_over_128_digits_overflows() {
        let s = cat(&[b"0.", &zeros(128), b"1"]);
        assert_eq!(parse(&s), Err(Overflow));
    }

    // ---- writing -------------------------------------------------------

    #[test]
    fn formats_canonically() {
        let table: &[(i64, i8, &[u8])] = &[
            (123_456_789, -4, b"12345.6789"),
            (150, -2, b"1.50"),
            (5, -1, b"0.5"),
            (-5, -1, b"-0.5"),
            (5, -3, b"0.005"),
            (0, -2, b"0.00"),
            (0, 0, b"0"),
            (5, 3, b"5000"),
            (i64::MAX, 0, b"9223372036854775807"),
            (i64::MIN, 0, b"-9223372036854775808"),
        ];
        for &(m, e, want) in table {
            assert_eq!(
                show(&fmt(Decimal::new(m, e))),
                show(want),
                "({m}, {e}) formats canonically"
            );
        }
    }

    #[test]
    fn the_longest_output_is_max_len() {
        let longest = fmt(Decimal::new(i64::MIN, 127));
        assert_eq!(longest.len(), Decimal::MAX_LEN);
        assert_eq!(
            show(&longest),
            show(&cat(&[b"-9223372036854775808", &zeros(127)]))
        );
        let deepest = fmt(Decimal::new(i64::MIN, -128));
        assert_eq!(
            show(&deepest),
            show(&cat(&[b"-0.", &zeros(128 - 19), b"9223372036854775808"]))
        );
        assert!(deepest.len() <= Decimal::MAX_LEN);
    }

    // ---- the round trip, ADR-0120 decision 5 --------------------------

    /// A canonical string, built character by character — never by calling
    /// `format`, or the test would check the function against itself.
    ///
    /// Integer part: `0`, or a nonzero digit and up to 18 more. Fraction:
    /// absent, or 1..=128 digits. A `-` only when the digits are not all zero.
    /// Anything whose significant digits exceed `i64::MAX` is drawn again.
    fn canonical(rng: &mut Rng) -> (Vec<u8>, usize, usize) {
        loop {
            let mut int = Vec::new();
            if rng.below(3) == 0 {
                int.push(b'0');
            } else {
                int.push(rng.nonzero_digit());
                for _ in 0..rng.range(0, 18) {
                    int.push(rng.digit());
                }
            }
            let mut k = match rng.below(4) {
                0 => 0,
                1 => rng.range(1, 19),
                2 => rng.range(100, 128),
                _ => rng.range(1, 40),
            };
            let mut frac = Vec::new();
            if int == b"0" {
                let floor = k.saturating_sub(19);
                let z = rng.range(floor, k);
                frac.extend(zeros(z));
                for _ in z..k {
                    frac.push(rng.digit());
                }
            } else {
                k = k.min(19 - int.len());
                for _ in 0..k {
                    frac.push(rng.digit());
                }
            }
            let all: Vec<u8> = int.iter().chain(frac.iter()).copied().collect();
            let sig: Vec<u8> = all.iter().copied().skip_while(|&b| b == b'0').collect();
            if sig.len() > 19 || (sig.len() == 19 && sig.as_slice() > &b"9223372036854775807"[..]) {
                continue;
            }
            let mut s = Vec::new();
            if !sig.is_empty() && rng.below(2) == 0 {
                s.push(b'-');
            }
            s.extend(&int);
            if k > 0 {
                s.push(b'.');
                s.extend(&frac);
            }
            return (s, sig.len(), k);
        }
    }

    #[test]
    fn canonical_strings_round_trip_byte_identical() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let (mut n, mut nineteen, mut deepest) = (0usize, 0usize, 0usize);
        let mut bad: Vec<(String, String)> = Vec::new();
        for _ in 0..150_000 {
            let (s, sig, k) = canonical(&mut rng);
            n += 1;
            nineteen += usize::from(sig == 19);
            deepest += usize::from(k == 128);
            let back = match as_decimal(&s) {
                Ok(d) => fmt(d),
                Err(e) => format!("{e:?}").into_bytes(),
            };
            if back != s && bad.len() < 10 {
                bad.push((show(&s), show(&back)));
            }
        }
        println!(
            "{n} canonical strings; {nineteen} with 19 significant digits; {deepest} with 128 fraction digits"
        );
        assert!(n >= 100_000);
        assert!(
            nineteen > 0,
            "the generator must reach 19 significant digits"
        );
        assert!(deepest > 0, "the generator must reach 128 fraction digits");
        assert!(bad.is_empty(), "format(as_decimal(s)) != s: {bad:?}");
    }

    #[test]
    fn every_non_positive_exponent_round_trips() {
        let mut cases: Vec<Decimal> = Vec::new();
        for m in [i64::MAX, -i64::MAX, 0, 1, -1, 10, -10] {
            for e in [0i8, -1, -18, -19, -20, -127, -128] {
                cases.push(Decimal::new(m, e));
            }
        }
        let mut rng = Rng(0xD1B5_4A32_D192_ED03);
        for _ in 0..200_000 {
            let digits = rng.range(1, 19) as u32;
            let magnitude = if digits == 19 {
                rng.below(i64::MAX as u64 + 1) as i64
            } else {
                rng.below(10u64.pow(digits)) as i64
            };
            let m = if rng.below(2) == 0 {
                magnitude
            } else {
                -magnitude
            };
            let e = -(rng.below(129) as i16) as i8;
            cases.push(Decimal::new(m, e));
        }
        let mut bad: Vec<(Decimal, String, Result<Decimal, ConvertError>)> = Vec::new();
        for &d in &cases {
            let text = fmt(d);
            let back = as_decimal(&text);
            if back != Ok(d) && bad.len() < 10 {
                bad.push((d, show(&text), back));
            }
        }
        println!("{} decimals with exponent <= 0 round-tripped", cases.len());
        assert!(bad.is_empty(), "as_decimal(format(d)) != Ok(d): {bad:?}");
    }

    #[test]
    fn a_positive_exponent_reads_back_as_its_integer() {
        // FIX has no exponent notation, so `(5, 3)` can only be written as the
        // integer it is — and reading that integer back says nothing about how
        // many of its zeros were scale. The value survives; the pair does not.
        // ADR-0120 decision 5 promises (b) for `exponent <= 0` only, for this.
        let d = Decimal::new(5, 3);
        let text = fmt(d);
        assert_eq!(show(&text), "5000");
        let back = as_decimal(&text);
        assert_eq!(back, Ok(Decimal::new(5000, 0)));
        assert_ne!(back, Ok(d));
    }

    #[test]
    fn non_canonical_inputs_format_canonically() {
        let table: &[(&[u8], &[u8])] = &[
            (b"002000.00", b"2000.00"),
            (b"23.", b"23"),
            (b".5", b"0.5"),
            (b"-0.00", b"0.00"),
            (b"-.5", b"-0.5"),
        ];
        for &(input, want) in table {
            let d =
                as_decimal(input).unwrap_or_else(|e| panic!("{} must parse: {e:?}", show(input)));
            assert_eq!(show(&fmt(d)), show(want), "{}", show(input));
        }
    }

    // ---- the grammar is the session's ----------------------------------

    #[test]
    fn agrees_with_the_session_float_syntax_exhaustively() {
        const ALPHABET: &[u8] = b"019-.+e";
        let mut checked = 0usize;
        let mut accepted = 0usize;
        let mut bad: Vec<(String, bool, Result<Decimal, ConvertError>)> = Vec::new();
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
                if session != codec_accepts(&s) && bad.len() < 10 {
                    bad.push((show(&s), session, as_decimal(&s)));
                }
            }
        }
        println!("{checked} strings of length 0..=6 over {{0,1,9,-,.,+,e}}; {accepted} accepted");
        assert_eq!(checked, 137_257);
        assert!(accepted > 0);
        assert!(
            bad.is_empty(),
            "accepts(s) must equal as_decimal(s) is Ok or Overflow: {bad:?}"
        );
    }

    #[test]
    fn agrees_with_the_session_float_syntax_on_long_values() {
        const NOISE: &[u8] = b".-+e 0x\x01,9";
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let (mut oks, mut overflows, mut refused) = (0usize, 0usize, 0usize);
        let mut bad: Vec<(String, bool, Result<Decimal, ConvertError>)> = Vec::new();
        for _ in 0..30_000 {
            let len = rng.range(15, 140);
            // How often a digit is `0`: mostly-zero strings stay inside i64 and
            // parse; mostly-nonzero ones overflow. Both halves must be reached.
            let zero_per_mille = [100u64, 900, 990, 999][rng.below(4) as usize];
            let mut s = Vec::with_capacity(len + 1);
            if rng.below(2) == 0 {
                s.push(b'-');
            }
            let point_at = rng.range(0, len);
            for i in 0..len {
                if i == point_at && rng.below(4) != 0 {
                    s.push(b'.');
                } else if rng.below(1000) < zero_per_mille {
                    s.push(b'0');
                } else {
                    s.push(rng.nonzero_digit());
                }
            }
            if rng.below(3) == 0 {
                let at = rng.range(0, s.len() - 1);
                s[at] = NOISE[rng.below(NOISE.len() as u64) as usize];
            }
            let session = session_accepts(&s);
            let r = as_decimal(&s);
            match r {
                Ok(_) => oks += 1,
                Err(Overflow) => overflows += 1,
                Err(_) => refused += 1,
            }
            if session != codec_accepts(&s) && bad.len() < 10 {
                bad.push((show(&s), session, r));
            }
        }
        println!("long values: {oks} parsed, {overflows} overflowed, {refused} refused");
        assert!(oks > 1000 && overflows > 1000 && refused > 1000);
        assert!(
            bad.is_empty(),
            "accepts(s) must equal as_decimal(s) is Ok or Overflow: {bad:?}"
        );
    }

    fn is_float(t: FieldType) -> bool {
        matches!(
            t,
            FieldType::Float
                | FieldType::Qty
                | FieldType::Price
                | FieldType::PriceOffset
                | FieldType::Amt
                | FieldType::Percentage
        )
    }

    #[test]
    fn every_float_value_in_the_corpus_parses() {
        let lines = crate::common::load_all();
        let mut idx: FieldIndex<64> = FieldIndex::new();
        let mut values = 0usize;
        let mut refused: Vec<(String, usize, u32, String, ConvertError)> = Vec::new();
        for l in &lines {
            let Ok(Parsed::Complete { .. }) =
                parse_into::<NoDict, 64>(&l.wire, &mut idx, Validation::NONE)
            else {
                continue; // six lines the parser refuses; `defs.rs` owns them
            };
            let view = idx.view(&l.wire);
            for i in 0..view.len() {
                let (tag, value) = view.field_at(i).expect("in range");
                if !Fix44::field_type(tag).is_some_and(is_float) {
                    continue;
                }
                values += 1;
                if let Err(e) = as_decimal(value) {
                    refused.push((l.file.clone(), l.line_no, tag, show(value), e));
                }
            }
        }
        println!("{values} float values in the FIX 4.4 corpus; refused: {refused:?}");
        assert!(
            values > 0,
            "a corpus test that reads no value proves nothing"
        );
        assert_eq!(refused.len(), 1, "exactly one refusal: {refused:?}");
        let (file, _, tag, value, err) = &refused[0];
        assert!(file.starts_with("14f_"), "{file}");
        assert_eq!((*tag, value.as_str(), *err), (38, "+200.00", NotANumber));
    }
}
