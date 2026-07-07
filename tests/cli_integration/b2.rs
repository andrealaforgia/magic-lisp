//! B2: functions, recursion, conditionals.

use magiclisp::exitcode::SUCCESS;

use super::helpers::{eval_ok, run, stderr_of, stdout_of, write_source};

#[test]
fn b2_e1_quote_prevents_evaluation_of_a_list_datum() {
    assert_eq!(
        eval_ok("b2-e1-quote.ml", "(display (quote (+ 1 2)))"),
        "(+ 1 2)"
    );
}

#[test]
fn b2_e1_quote_shorthand_prevents_evaluation_of_a_list_datum() {
    assert_eq!(
        eval_ok("b2-e1-shorthand.ml", "(display '(+ 1 2))"),
        "(+ 1 2)"
    );
}

#[test]
fn b2_e2_two_branch_if_with_true_condition_picks_then() {
    assert_eq!(
        eval_ok("b2-e2-tt.ml", "(display (if #t \"then\" \"else\"))"),
        "then"
    );
}

#[test]
fn b2_e2_two_branch_if_with_false_condition_picks_else() {
    assert_eq!(
        eval_ok("b2-e2-tf.ml", "(display (if #f \"then\" \"else\"))"),
        "else"
    );
}

#[test]
fn b2_e2_one_branch_if_with_true_condition_picks_then() {
    assert_eq!(eval_ok("b2-e2-ot.ml", "(display (if #t \"then\"))"), "then");
}

#[test]
fn b2_e2_one_branch_if_with_false_condition_has_no_visible_output_and_exits_success() {
    let file = write_source("b2-e2-of.ml", "(display (if #f \"then\"))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert!(stdout_of(&output).is_empty());
}

#[test]
fn b2_e3_define_binds_a_top_level_value_usable_afterward() {
    assert_eq!(eval_ok("b2-e3-value.ml", "(define x 42) (display x)"), "42");
}

#[test]
fn b2_e3_define_function_with_fixed_arity() {
    assert_eq!(
        eval_ok(
            "b2-e3-fixed.ml",
            "(define (add2 a b) (+ a b)) (display (add2 3 4))"
        ),
        "7"
    );
}

#[test]
fn b2_e3_define_function_with_fixed_plus_rest_collects_the_extras() {
    assert_eq!(
        eval_ok(
            "b2-e3-fixed-rest.ml",
            "(define (f a b . rest) rest) (display (f 1 2 3 4 5))"
        ),
        "(3 4 5)"
    );
}

#[test]
fn b2_e3_define_function_with_a_single_rest_arg_collects_everything() {
    assert_eq!(
        eval_ok(
            "b2-e3-all-rest.ml",
            "(define (g . args) args) (display (g 1 2 3))"
        ),
        "(1 2 3)"
    );
}

#[test]
fn b2_e4_lambda_immediately_invoked_with_a_rest_arg() {
    assert_eq!(
        eval_ok(
            "b2-e4-iife.ml",
            "(display ((lambda (a . rest) rest) 1 2 3))"
        ),
        "(2 3)"
    );
}

#[test]
fn b2_e4_lambda_stored_via_define_and_called_later() {
    assert_eq!(
        eval_ok(
            "b2-e4-stored.ml",
            "(define my-fn (lambda (a . rest) rest)) (display (my-fn 10 20 30))"
        ),
        "(20 30)"
    );
}

#[test]
fn b2_e5_begin_runs_expressions_in_order_and_yields_only_the_last_value() {
    assert_eq!(
        eval_ok(
            "b2-e5-begin.ml",
            "(display (begin (display 1) (display 2) 3))"
        ),
        "123"
    );
}

#[test]
fn b2_e6_redefining_x_is_seen_by_a_function_that_referenced_x_before_the_redefinition() {
    let out = eval_ok(
        "b2-e6-redef.ml",
        "(define (x) 1) \
         (define (a) (x)) \
         (display (a)) (newline) \
         (define (x) 2) \
         (display (a)) (newline)",
    );
    assert_eq!(out, "1\n2\n");
}

#[test]
fn b2_e7_recursive_factorial_computes_a_multi_step_case_correctly() {
    let out = eval_ok(
        "b2-e7-fact.ml",
        "(define (fact n) (if (< n 2) 1 (* n (fact (- n 1))))) (display (fact 5))",
    );
    assert_eq!(out, "120");
}

#[test]
fn b2_e8_call_arguments_are_evaluated_left_to_right_before_the_call_completes() {
    // `tap` displays its argument (a visible side effect) and returns it
    // unchanged, so the order those side effects appear in proves argument
    // evaluation order; the outer display's own "3" only appears last.
    let out = eval_ok(
        "b2-e8-order.ml",
        "(define (tap x) (display x) x) (display (+ (tap 1) (tap 2)))",
    );
    assert_eq!(out, "123");
}

#[test]
fn b2_e9_zero_is_truthy_in_a_conditional() {
    assert_eq!(
        eval_ok("b2-e9-zero.ml", "(display (if 0 \"truthy\" \"falsy\"))"),
        "truthy"
    );
}

#[test]
fn b2_e9_the_empty_list_is_truthy_in_a_conditional() {
    assert_eq!(
        eval_ok(
            "b2-e9-empty-list.ml",
            "(display (if '() \"truthy\" \"falsy\"))"
        ),
        "truthy"
    );
}

#[test]
fn b2_e10_minus_accepts_two_arguments() {
    assert_eq!(eval_ok("b2-e10-minus2.ml", "(display (- 10 3))"), "7");
}

#[test]
fn b2_e10_times_accepts_two_arguments() {
    assert_eq!(eval_ok("b2-e10-times2.ml", "(display (* 3 4))"), "12");
}

#[test]
fn b2_e10_minus_accepts_four_or_more_arguments() {
    assert_eq!(
        eval_ok("b2-e10-minus4.ml", "(display (- 20 1 2 3 4))"),
        "10"
    );
}

#[test]
fn b2_e10_times_accepts_four_or_more_arguments() {
    assert_eq!(eval_ok("b2-e10-times4.ml", "(display (* 1 2 3 4))"), "24");
}

#[test]
fn b2_e10_chained_comparison_distinguishes_a_true_increasing_chain_from_a_broken_one() {
    // A naive endpoints-only check would wrongly call (< 1 3 2) true (1 < 2).
    assert_eq!(
        eval_ok("b2-e10-lt-chain-ok.ml", "(display (< 1 2 3))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b2-e10-lt-chain-broken.ml", "(display (< 1 3 2))"),
        "#f"
    );
}

#[test]
fn b2_e11_integer_overflow_wraps_instead_of_erroring_or_growing() {
    let out = eval_ok(
        "b2-e11-overflow.ml",
        &format!("(display (+ {} 1))", i64::MAX),
    );
    assert_eq!(out, i64::MAX.wrapping_add(1).to_string());
}

#[test]
fn b2_e12_integers_display_in_ordinary_decimal_notation() {
    assert_eq!(eval_ok("b2-e12-int.ml", "(display -12345)"), "-12345");
}

#[test]
fn b2_e12_booleans_display_as_hash_t_and_hash_f() {
    assert_eq!(
        eval_ok("b2-e12-bools.ml", "(display #t) (display #f)"),
        "#t#f"
    );
}

#[test]
fn b2_e13a_demo_program_redefines_fact_then_computes_fact_10() {
    // Verbatim per the behaviour spec: a stub that would error if called
    // (it calls an undefined global), then the real recursive definition,
    // then display (fact 10) followed by a newline.
    let src = "\
        (define (fact n) (this-stub-would-error-if-called n))\n\
        (define (fact n) (if (< n 2) 1 (* n (fact (- n 1)))))\n\
        (display (fact 10))\n\
        (newline)\n";
    let file = write_source("b2-e13a.ml", src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(stdout_of(&output), "3628800\n");
}

#[test]
fn b2_e13b_demo_program_displaying_if_false_false_produces_no_visible_output() {
    let file = write_source("b2-e13b.ml", "(display (if #f #f))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(stdout_of(&output).is_empty());
}
