//! B7: the numeric library (spec 4.1).

use super::helpers::{eval_ok, run, run_demo, stderr_of, stdout_of, write_source};

#[test]
fn b7_e1_quotient_remainder_modulo_demo_values() {
    assert_eq!(eval_ok("b7-e1a.ml", "(display (quotient 7 2))"), "3");
    assert_eq!(eval_ok("b7-e1b.ml", "(display (remainder 7 2))"), "1");
    assert_eq!(eval_ok("b7-e1c.ml", "(display (modulo -7 2))"), "1");
}

#[test]
fn b7_e1_remainder_and_modulo_differ_on_the_same_negative_dividend() {
    assert_eq!(eval_ok("b7-e1d.ml", "(display (remainder -7 2))"), "-1");
    assert_eq!(eval_ok("b7-e1e.ml", "(display (modulo -7 2))"), "1");
}

#[test]
fn b7_e1_dividing_by_zero_is_an_error_in_all_three() {
    for op in ["quotient", "remainder", "modulo"] {
        let file = write_source(
            &format!("b7-e1-zero-{op}.ml"),
            &format!("(display ({op} 7 0))"),
        );
        let output = run(&["eval", file.to_str().unwrap()]);
        assert!(
            !output.status.success(),
            "{op} by zero should fail, stdout: {}",
            stdout_of(&output)
        );
    }
}

#[test]
fn b7_e2_abs_min_max_demo_values() {
    assert_eq!(eval_ok("b7-e2a.ml", "(display (abs -5))"), "5");
    assert_eq!(eval_ok("b7-e2b.ml", "(display (max 1 5 3))"), "5");
    assert_eq!(eval_ok("b7-e2c.ml", "(display (min 1 5 3))"), "1");
    assert_eq!(eval_ok("b7-e2d.ml", "(display (min 3 1))"), "1");
}

#[test]
fn b7_e2_each_predicate_shown_both_ways() {
    assert_eq!(eval_ok("b7-e2e.ml", "(display (zero? 0))"), "#t");
    assert_eq!(eval_ok("b7-e2f.ml", "(display (zero? 1))"), "#f");
    assert_eq!(eval_ok("b7-e2g.ml", "(display (positive? 1))"), "#t");
    assert_eq!(eval_ok("b7-e2h.ml", "(display (positive? -1))"), "#f");
    assert_eq!(eval_ok("b7-e2i.ml", "(display (negative? -1))"), "#t");
    assert_eq!(eval_ok("b7-e2j.ml", "(display (negative? 1))"), "#f");
    assert_eq!(eval_ok("b7-e2k.ml", "(display (even? 10))"), "#t");
    assert_eq!(eval_ok("b7-e2l.ml", "(display (even? 3))"), "#f");
    assert_eq!(eval_ok("b7-e2m.ml", "(display (odd? 3))"), "#t");
    assert_eq!(eval_ok("b7-e2n.ml", "(display (odd? 10))"), "#f");
}

#[test]
fn b7_e3_floor_ceiling_round_truncate_demo_values() {
    assert_eq!(eval_ok("b7-e3a.ml", "(display (floor 2.7))"), "2.0");
    assert_eq!(eval_ok("b7-e3b.ml", "(display (round 2.5))"), "2.0");
    assert_eq!(eval_ok("b7-e3c.ml", "(display (round 3.5))"), "4.0");
}

#[test]
fn b7_e3_ceiling_and_truncate_distinguished_from_floor_on_a_negative_value() {
    assert_eq!(eval_ok("b7-e3d.ml", "(display (ceiling -2.7))"), "-2.0");
    assert_eq!(eval_ok("b7-e3e.ml", "(display (truncate -2.7))"), "-2.0");
    assert_eq!(eval_ok("b7-e3f.ml", "(display (floor -2.7))"), "-3.0");
}

#[test]
fn b7_e3_whole_number_input_is_returned_unchanged_not_promoted_to_a_float() {
    assert_eq!(eval_ok("b7-e3g.ml", "(display (floor 5))"), "5");
    assert_eq!(eval_ok("b7-e3h.ml", "(display (ceiling 5))"), "5");
    assert_eq!(eval_ok("b7-e3i.ml", "(display (round 5))"), "5");
    assert_eq!(eval_ok("b7-e3j.ml", "(display (truncate 5))"), "5");
}

#[test]
fn b7_e4_expt_and_sqrt_demo_values() {
    assert_eq!(eval_ok("b7-e4a.ml", "(display (expt 2 10))"), "1024");
    assert_eq!(eval_ok("b7-e4b.ml", "(display (sqrt 4))"), "2.0");
}

#[test]
fn b7_e4_transcendentals_spot_check_known_values() {
    assert_eq!(eval_ok("b7-e4c.ml", "(display (exp 0))"), "1.0");
    assert_eq!(eval_ok("b7-e4d.ml", "(display (log 1))"), "0.0");
    assert_eq!(eval_ok("b7-e4e.ml", "(display (sin 0))"), "0.0");
    assert_eq!(eval_ok("b7-e4f.ml", "(display (cos 0))"), "1.0");
    assert_eq!(eval_ok("b7-e4g.ml", "(display (tan 0))"), "0.0");
    assert_eq!(eval_ok("b7-e4h.ml", "(display (atan 0))"), "0.0");
}

#[test]
fn b7_e5_type_predicates_shown_both_ways() {
    assert_eq!(eval_ok("b7-e5a.ml", "(display (number? 5))"), "#t");
    assert_eq!(eval_ok("b7-e5b.ml", "(display (number? \"x\"))"), "#f");
    assert_eq!(eval_ok("b7-e5c.ml", "(display (integer? 5))"), "#t");
    assert_eq!(eval_ok("b7-e5d.ml", "(display (integer? 5.0))"), "#f");
    assert_eq!(eval_ok("b7-e5e.ml", "(display (float? 5.0))"), "#t");
    assert_eq!(eval_ok("b7-e5f.ml", "(display (float? 5))"), "#f");
}

#[test]
fn b7_e5_exact_inexact_conversions() {
    assert_eq!(eval_ok("b7-e5g.ml", "(display (exact->inexact 5))"), "5.0");
    assert_eq!(eval_ok("b7-e5h.ml", "(display (inexact->exact 5.7))"), "5");
}

#[test]
fn b7_e5_inexact_to_exact_errors_on_infinite_or_nan() {
    // qa test-design review (msg #127): assert the error names the specific
    // non-finite value, not just that it failed.
    for (expr, label) in [
        ("(/ 1.0 0.0)", "+inf.0"),
        ("(/ -1.0 0.0)", "-inf.0"),
        ("(/ 0.0 0.0)", "+nan.0"),
    ] {
        let file = write_source(
            &format!("b7-e5-nonfinite-{}.ml", expr.len()),
            &format!("(display (inexact->exact {expr}))"),
        );
        let output = run(&["eval", file.to_str().unwrap()]);
        assert!(
            !output.status.success(),
            "inexact->exact of {expr} should fail"
        );
        assert!(
            stderr_of(&output).contains(label),
            "expected the error to name {label}, got: {}",
            stderr_of(&output)
        );
    }
}

#[test]
fn b7_e6_number_to_string_and_back() {
    assert_eq!(
        eval_ok("b7-e6a.ml", "(display (string->number \"3.5\"))"),
        "3.5"
    );
    assert_eq!(
        eval_ok("b7-e6b.ml", "(display (string->number \"xyz\"))"),
        "#f"
    );
    assert_eq!(
        eval_ok(
            "b7-e6c.ml",
            "(display (string->number (number->string 42)))"
        ),
        "42"
    );
}

#[test]
fn b7_e7_every_category_is_a_first_class_procedure_value() {
    // A representative spot-check across categories: division-family,
    // predicate, rounding, transcendental, conversion -- each passed as an
    // argument to a small higher-order function, working exactly as
    // calling it directly would.
    let src = "(define (apply-to-5 f) (f 5)) \
               (display (apply-to-5 abs)) (newline) \
               (display (apply-to-5 even?)) (newline) \
               (display (apply-to-5 floor)) (newline) \
               (display (apply-to-5 exact->inexact)) (newline) \
               (define (apply-to-2-and-3 f) (f 2 3)) \
               (display (apply-to-2-and-3 quotient)) (newline)";
    let out = eval_ok("b7-e7.ml", src);
    assert_eq!(out, "5\n#f\n5\n5.0\n0\n");
}

#[test]
fn b7_e8_all_thirteen_demo_expressions_produce_exactly_the_prescribed_output() {
    assert_eq!(run_demo("b7-e8-01.ml", "(display (quotient 7 2))"), "3\n");
    assert_eq!(run_demo("b7-e8-02.ml", "(display (remainder 7 2))"), "1\n");
    assert_eq!(run_demo("b7-e8-03.ml", "(display (modulo -7 2))"), "1\n");
    assert_eq!(run_demo("b7-e8-04.ml", "(display (abs -5))"), "5\n");
    assert_eq!(run_demo("b7-e8-05.ml", "(display (max 1 5 3))"), "5\n");
    assert_eq!(run_demo("b7-e8-06.ml", "(display (even? 10))"), "#t\n");
    assert_eq!(run_demo("b7-e8-07.ml", "(display (expt 2 10))"), "1024\n");
    assert_eq!(run_demo("b7-e8-08.ml", "(display (sqrt 4))"), "2.0\n");
    assert_eq!(run_demo("b7-e8-09.ml", "(display (floor 2.7))"), "2.0\n");
    assert_eq!(run_demo("b7-e8-10.ml", "(display (round 2.5))"), "2.0\n");
    assert_eq!(run_demo("b7-e8-11.ml", "(display (round 3.5))"), "4.0\n");
    assert_eq!(
        run_demo("b7-e8-12.ml", "(display (string->number \"3.5\"))"),
        "3.5\n"
    );
    assert_eq!(
        run_demo("b7-e8-13.ml", "(display (string->number \"xyz\"))"),
        "#f\n"
    );
}
