//! B4: general iteration and numeric semantics.

use magiclisp::exitcode::{RUNTIME_ERROR, SOURCE_ERROR, SUCCESS};

use super::helpers::{eval_ok, run, run_demo, stderr_of, write_source};

#[test]
fn b4_e1_do_loop_sums_zero_to_four_stopping_when_the_counter_reaches_five() {
    let out = eval_ok(
        "b4-e1.ml",
        "(display (do ((i 0 (+ i 1)) (s 0 (+ s i))) ((= i 5) s)))",
    );
    assert_eq!(out, "10");
}

#[test]
fn b4_e1_do_loop_with_an_omitted_step_keeps_that_variable_unchanged() {
    let out = eval_ok(
        "b4-e1b.ml",
        "(display (do ((i 0 (+ i 1)) (k 42)) ((= i 3) k)))",
    );
    assert_eq!(out, "42");
}

#[test]
fn b4_e2_reads_a_plain_decimal_float() {
    assert_eq!(eval_ok("b4-e2-decimal.ml", "(display 1.5)"), "1.5");
}

#[test]
fn b4_e2_reads_an_exponent_form_float() {
    assert_eq!(eval_ok("b4-e2-exp.ml", "(display 1e3)"), "1000.0");
}

#[test]
fn b4_e2_reads_a_decimal_and_exponent_float() {
    assert_eq!(eval_ok("b4-e2-decexp.ml", "(display 1.5e-3)"), "0.0015");
}

#[test]
fn b4_e2_reads_a_hexadecimal_integer_to_the_correct_decimal_value() {
    assert_eq!(eval_ok("b4-e2-hex.ml", "(display #x1A)"), "26");
}

#[test]
fn b4_e2_reads_a_binary_integer_to_the_correct_decimal_value() {
    assert_eq!(eval_ok("b4-e2-bin.ml", "(display #b101)"), "5");
}

#[test]
fn b4_e2_reads_an_octal_integer_to_the_correct_decimal_value() {
    assert_eq!(eval_ok("b4-e2-oct.ml", "(display #o17)"), "15");
}

#[test]
fn b4_e3_an_integer_literal_too_large_for_i64_is_a_read_error_not_a_silent_wrap() {
    let file = write_source(
        "b4-e3.ml",
        &format!("(display {}0)", i64::MAX), // one digit past i64::MAX
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b4_e4a_float_display_uses_the_shortest_round_tripping_digits() {
    assert_eq!(eval_ok("b4-e4a.ml", "(display 0.1)"), "0.1");
}

#[test]
fn b4_e4b_a_whole_valued_float_still_shows_a_trailing_dot_zero() {
    let out = run_demo("b4-e4b-demo4.ml", "(display 1.0)");
    assert_eq!(out, "1.0\n");
}

#[test]
fn b4_e4c_ordinary_magnitudes_print_in_plain_notation() {
    assert_eq!(eval_ok("b4-e4c-plain.ml", "(display 12345.5)"), "12345.5");
}

#[test]
fn b4_e4c_magnitudes_far_outside_the_ordinary_range_switch_to_exponential() {
    assert_eq!(eval_ok("b4-e4c-large.ml", "(display 1e20)"), "1e20");
    assert_eq!(eval_ok("b4-e4c-small.ml", "(display 1e-20)"), "1e-20");
}

#[test]
fn b4_e4d_positive_infinity_prints_in_its_dedicated_form() {
    assert_eq!(eval_ok("b4-e4d-pinf.ml", "(display (/ 1.0 0.0))"), "+inf.0");
}

#[test]
fn b4_e4d_negative_infinity_prints_in_its_dedicated_form() {
    assert_eq!(
        eval_ok("b4-e4d-ninf.ml", "(display (/ -1.0 0.0))"),
        "-inf.0"
    );
}

#[test]
fn b4_e4d_not_a_number_prints_in_its_dedicated_form() {
    assert_eq!(eval_ok("b4-e4d-nan.ml", "(display (/ 0.0 0.0))"), "+nan.0");
}

#[test]
fn b4_e4e_negative_zero_prints_distinctly_from_positive_zero() {
    let out = run_demo("b4-e4e-demo5.ml", "(display -0.0)");
    assert_eq!(out, "-0.0\n");
    assert_eq!(eval_ok("b4-e4e-poszero.ml", "(display 0.0)"), "0.0");
}

#[test]
fn b4_e5_integer_arithmetic_still_wraps_on_overflow_in_this_slice() {
    let out = eval_ok("b4-e5.ml", &format!("(display (+ {} 1))", i64::MAX));
    assert_eq!(out, i64::MAX.wrapping_add(1).to_string());
}

#[test]
fn b4_e6_plus_with_zero_arguments_is_zero() {
    assert_eq!(eval_ok("b4-e6-plus0.ml", "(display (+))"), "0");
}

#[test]
fn b4_e6_times_with_zero_arguments_is_one() {
    assert_eq!(eval_ok("b4-e6-times0.ml", "(display (*))"), "1");
}

#[test]
fn b4_e6_minus_with_zero_arguments_is_a_runtime_error() {
    let file = write_source("b4-e6-minus0.ml", "(display (-))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
}

#[test]
fn b4_e6_divide_with_zero_arguments_is_a_runtime_error() {
    let file = write_source("b4-e6-div0.ml", "(display (/))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
}

#[test]
fn b4_e6_minus_with_one_argument_negates_it() {
    assert_eq!(eval_ok("b4-e6-minus1.ml", "(display (- 5))"), "-5");
}

#[test]
fn b4_e6_divide_with_one_argument_inverts_it() {
    assert_eq!(eval_ok("b4-e6-div1.ml", "(display (/ 4))"), "0.25");
}

#[test]
fn b4_e7a_exact_whole_number_division_is_an_integer_result() {
    let out = run_demo("b4-e7a-demo1.ml", "(display (/ 6 3))");
    assert_eq!(out, "2\n");
}

#[test]
fn b4_e7b_inexact_whole_number_division_is_a_float_result() {
    let out = run_demo("b4-e7b-demo2.ml", "(display (/ 7 2))");
    assert_eq!(out, "3.5\n");
}

#[test]
fn b4_e7c_whole_number_divided_by_a_float_is_a_float_result_even_when_exact() {
    let out = run_demo("b4-e7c-demo3.ml", "(display (/ 6 3.0))");
    assert_eq!(out, "2.0\n");
}

#[test]
fn b4_e7d_integer_divided_by_exact_zero_is_a_runtime_failure() {
    let file = write_source("b4-e7d.ml", "(display (/ 6 0))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b4_e7e_float_divided_by_zero_succeeds_with_a_special_float_result() {
    // Contrasts directly with E7d: same shape (divide by zero), but a
    // float operand means IEEE rules apply instead of a runtime failure.
    assert_eq!(eval_ok("b4-e7e.ml", "(display (/ 6.0 0))"), "+inf.0");
}

#[test]
fn b4_e8_all_six_demo_programs_produce_exactly_the_prescribed_output() {
    assert_eq!(run_demo("b4-e8-demo1.ml", "(display (/ 6 3))"), "2\n");
    assert_eq!(run_demo("b4-e8-demo2.ml", "(display (/ 7 2))"), "3.5\n");
    assert_eq!(run_demo("b4-e8-demo3.ml", "(display (/ 6 3.0))"), "2.0\n");
    assert_eq!(run_demo("b4-e8-demo4.ml", "(display 1.0)"), "1.0\n");
    assert_eq!(run_demo("b4-e8-demo5.ml", "(display -0.0)"), "-0.0\n");
    assert_eq!(
        run_demo(
            "b4-e8-demo6.ml",
            "(display (do ((i 0 (+ i 1)) (s 0 (+ s i))) ((= i 5) s)))"
        ),
        "10\n"
    );
}

#[test]
fn b4_e8_every_demo_exits_successfully() {
    let sources = [
        "(display (/ 6 3)) (newline)",
        "(display (/ 7 2)) (newline)",
        "(display (/ 6 3.0)) (newline)",
        "(display 1.0) (newline)",
        "(display -0.0) (newline)",
        "(display (do ((i 0 (+ i 1)) (s 0 (+ s i))) ((= i 5) s))) (newline)",
    ];
    for (i, src) in sources.iter().enumerate() {
        let file = write_source(&format!("b4-e8-exit-{i}.ml"), src);
        let output = run(&["eval", file.to_str().unwrap()]);
        assert_eq!(
            output.status.code(),
            Some(SUCCESS),
            "demo {i} stderr: {}",
            stderr_of(&output)
        );
    }
}
