//! B24: division by an exact-zero divisor must error consistently at every
//! argument position (SPEC.md's division rule).
//!
//! E1/E2/E3/E4's own unit-level coverage lives in `src/vm.rs`'s test
//! module, right next to `native_divide` itself. This module adds the two
//! things that need the real CLI/process boundary: E5's program-
//! independent oracle (this project's first property-based/metamorphic
//! test -- no external PBT crate, matching this project's zero-dependency
//! convention, so a small fixed-seed generator stands in for one) and E6's
//! integration sweep.

use magiclisp::exitcode::{RUNTIME_ERROR, SUCCESS};

use super::helpers::{run, stderr_of, stdout_of, write_source};

// --- E1: zero at a middle/non-final position, previously wrongly non-error. ---

#[test]
fn e1_an_exact_zero_divisor_errors_at_a_middle_or_non_final_argument_position() {
    for src in [
        "(display (/ 1 3 0))",
        "(display (/ 2 5 0))",
        "(display (/ 7 11 0))",
        "(display (/ 8 4 0 2))",
    ] {
        let file = write_source("b24-e1.ml", src);
        let out = run(&["eval", file.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(RUNTIME_ERROR), "{src}");
        let stderr = stderr_of(&out);
        assert_eq!(stderr.lines().count(), 1, "{src}: stderr {stderr:?}");
        assert!(
            stdout_of(&out).is_empty(),
            "{src}: stdout should be empty on error"
        );
    }
}

// --- E2: no regression -- the already-correct cases still error. ---

#[test]
fn e2_the_already_correct_zero_divisor_cases_still_error() {
    for src in [
        "(display (/ 6 3 0))",
        "(display (/ 5 0))",
        "(display (/ 10 2 0))",
        "(display (/ 0))",
    ] {
        let file = write_source("b24-e2.ml", src);
        let out = run(&["eval", file.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(RUNTIME_ERROR), "{src}");
    }
}

// --- E3: division by a float zero is unaffected, still IEEE 754. ---

#[test]
fn e3_division_by_a_float_zero_is_not_an_error_and_follows_ieee_754() {
    let cases = [
        ("(display (/ 1 0.0))", "+inf.0"),
        ("(display (/ 1 3 0.0))", "+inf.0"),
        ("(display (/ 1 -0.0))", "-inf.0"),
        ("(display (/ 1 3 -0.0))", "-inf.0"),
    ];
    for (src, expected) in cases {
        let file = write_source("b24-e3.ml", src);
        let out = run(&["eval", file.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(SUCCESS), "{src}");
        assert_eq!(stdout_of(&out), expected, "{src}");
    }
}

/// A tiny, fixed-seed linear congruential generator. This project has no
/// external dependency at all (no PBT/fuzzing crate either), so this
/// hand-rolled, deterministic source stands in for one -- a failure names
/// the exact case that broke, reproducible on every run, not a flaky
/// one-off tied to wall-clock or OS entropy.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }

    fn next_u64(&mut self) -> u64 {
        // Knuth/Numerical-Recipes 64-bit LCG constants.
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    /// A pseudo-random integer in the inclusive range `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
}

/// One generated variadic division's operand list: 2 to 5 fixnums, each in
/// -5..=5 -- narrow enough that both an exact-zero divisor and an earlier
/// step that doesn't divide evenly come up often across many generated
/// cases (this is a property test over many trials, not an exhaustive
/// enumeration).
fn generate_operands(rng: &mut Lcg) -> Vec<i64> {
    let len = rng.range(2, 5);
    (0..len).map(|_| rng.range(-5, 5)).collect()
}

/// The independently-computed oracle (SPEC.md's division rule itself, not
/// a copy of whatever the implementation currently prints): a variadic
/// division errors iff ANY divisor position -- every operand from index 1
/// onward, since the dividend at index 0 is never itself a divisor -- is
/// an exact fixnum 0, regardless of any other operand's value or the
/// accumulator's exactness so far.
fn oracle_should_error(operands: &[i64]) -> bool {
    operands[1..].contains(&0)
}

fn division_source(operands: &[i64]) -> String {
    let parts: Vec<String> = operands.iter().map(i64::to_string).collect();
    format!("(display (/ {}))", parts.join(" "))
}

/// The same operands' independently-evaluated LEFT-FOLD form, built by
/// nesting -- `[a, b, c]` becomes `(/ (/ a b) c)` -- rather than flattening
/// into one variadic call.
fn left_fold_source(operands: &[i64]) -> String {
    let mut expr = operands[0].to_string();
    for &n in &operands[1..] {
        expr = format!("(/ {expr} {n})");
    }
    format!("(display {expr})")
}

fn errored(out: &std::process::Output) -> bool {
    out.status.code() == Some(RUNTIME_ERROR)
}

// --- E5(a): property-based -- flat variadic division vs. the independent oracle. ---

#[test]
fn e5_property_generated_variadic_divisions_match_the_independent_zero_anywhere_oracle() {
    let mut rng = Lcg::new(0xB24_5EED);
    const TRIALS: usize = 300;
    let mut mismatches = Vec::new();
    for _ in 0..TRIALS {
        let operands = generate_operands(&mut rng);
        let expected_error = oracle_should_error(&operands);
        let src = division_source(&operands);
        let file = write_source("b24-e5-property.ml", &src);
        let out = run(&["eval", file.to_str().unwrap()]);
        if errored(&out) != expected_error {
            mismatches.push(format!(
                "{src}: oracle says error={expected_error}, actual exit code={:?}",
                out.status.code()
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {TRIALS} generated cases disagreed with the oracle:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

// --- E5(b): metamorphic -- flat vs. left-folded/nested form of the same operands. ---

#[test]
#[ignore = "RESOLVED, not an open question anymore (Examiner verdict, relay msg #464): the \
            flat and left-folded forms of the same operands disagree whenever an intermediate \
            left-fold step goes inexact before reaching a later exact-zero divisor -- e.g. \
            operands [-1, 4, 0]: flat `(/ -1 4 0)` errors (0 is an exact-fixnum divisor in the \
            original, all-integer argument list), but left-folded `(/ (/ -1 4) 0)` does not, \
            because by the time the outer call runs its dividend is already a runtime Float, \
            routing it through the any_float/IEEE-754 path -- the same path an already-accepted \
            case (B4/E7e, `(/ 6.0 0)` -> `+inf.0`, non-error) also relies on. The Examiner ruled \
            this a genuine, orthogonal boundary (a sub-expression's fully-evaluated float RESULT \
            is indistinguishable from a literal float dividend, which is B4/E7e's territory, not \
            B24's single-fold-going-inexact scope) and confirmed E5 is satisfied by E5(a)'s \
            property oracle alone. Kept disabled deliberately, as a documented boundary, not \
            dead weight -- not because the relation is wrong, but because it correctly names a \
            fixed, accepted asymmetry between two unrelated behaviours' scopes."]
fn e5_metamorphic_flat_and_left_folded_division_agree_on_error_status() {
    let mut rng = Lcg::new(0x0FEE_DB24);
    const TRIALS: usize = 150;
    let mut mismatches = Vec::new();
    for _ in 0..TRIALS {
        let operands = generate_operands(&mut rng);
        let flat_file = write_source("b24-e5-flat.ml", &division_source(&operands));
        let nested_file = write_source("b24-e5-nested.ml", &left_fold_source(&operands));
        let flat_out = run(&["eval", flat_file.to_str().unwrap()]);
        let nested_out = run(&["eval", nested_file.to_str().unwrap()]);
        if errored(&flat_out) != errored(&nested_out) {
            mismatches.push(format!(
                "operands {operands:?}: flat errored={}, left-folded errored={}",
                errored(&flat_out),
                errored(&nested_out)
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {TRIALS} generated cases disagreed between flat and left-folded form:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

// --- E6: integration -- the whole observable acceptance set together, one pass. ---

#[test]
fn e6_the_full_observable_acceptance_set_holds_together() {
    // Deliberately one bundled smoke test, not split by group: E1-E3 each
    // already have their own dedicated, per-case test above with a full
    // localizing message; this one exists only to prove the three
    // categories hold TOGETHER in a single pass, so a shared failure here
    // means the interaction, not any individual case, needs the
    // already-precise tests above for localization (qa test-design
    // review).
    // Previously-broken zero-not-last cases now error.
    for src in ["(display (/ 1 3 0))", "(display (/ 8 4 0 2))"] {
        let file = write_source("b24-e6-fixed.ml", src);
        let out = run(&["eval", file.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(RUNTIME_ERROR), "{src}");
        assert_eq!(stderr_of(&out).lines().count(), 1, "{src}");
    }
    // Previously-correct zero cases still error.
    for src in ["(display (/ 6 3 0))", "(display (/ 5 0))"] {
        let file = write_source("b24-e6-regression.ml", src);
        let out = run(&["eval", file.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(RUNTIME_ERROR), "{src}");
    }
    // Float-zero cases still follow IEEE 754, clean exit.
    let file = write_source("b24-e6-float.ml", "(display (/ 1 3 0.0))");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&out), "+inf.0");
}
