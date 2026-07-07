//! B3: local bindings, mutation, conditional/sequencing forms.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::helpers::{eval_ok, run, run_demo, stderr_of, write_source};

#[test]
fn b3_e1_let_bindings_do_not_see_sibling_bindings_only_the_outer_scope() {
    // The sibling `y`'s init sees the OUTER x (1), not the sibling x (2).
    assert_eq!(
        eval_ok("b3-e1.ml", "(define x 1) (let ((x 2) (y x)) (display y))"),
        "1"
    );
}

#[test]
fn b3_e2_let_star_bindings_see_earlier_bindings_in_the_same_group() {
    assert_eq!(
        eval_ok("b3-e2.ml", "(let* ((x 1) (y (+ x 1))) (display y))"),
        "2"
    );
}

#[test]
fn b3_e3_letrec_enables_a_self_referencing_local_function() {
    let out = eval_ok(
        "b3-e3.ml",
        "(display (letrec ((fact (lambda (n) (if (< n 2) 1 (* n (fact (- n 1))))))) (fact 5)))",
    );
    assert_eq!(out, "120");
}

#[test]
fn b3_e4_named_let_sums_one_to_one_hundred() {
    let out = eval_ok(
        "b3-e4.ml",
        "(display (let loop ((i 1) (sum 0)) (if (> i 100) sum (loop (+ i 1) (+ sum i)))))",
    );
    assert_eq!(out, "5050");
}

#[test]
fn b3_e4b_named_let_iterates_well_beyond_one_hundred_without_hitting_the_call_depth_ceiling() {
    // named-let compiles to real (non-tail-call-optimized) recursive calls,
    // so it shares Vm::call_value's MAX_CALL_DEPTH ceiling — a security
    // review found that an ordinary loop summing 1..=1000 failed outright
    // against this crate's original 512/128 call-depth limits, since every
    // iteration burns a native stack frame. The VM now runs on its own
    // large dedicated stack, giving this headline iteration idiom real
    // headroom instead of silently capping it at a few hundred iterations.
    let out = eval_ok(
        "b3-e4b.ml",
        "(display (let loop ((i 1) (sum 0)) (if (> i 1000) sum (loop (+ i 1) (+ sum i)))))",
    );
    assert_eq!(out, "500500");
}

#[test]
fn b3_e5_internal_definitions_are_mutually_visible_regardless_of_order() {
    // six-times references triple, which is DEFINED AFTER it in the body.
    let out = eval_ok(
        "b3-e5.ml",
        "(define (f) \
           (define (double x) (* x 2)) \
           (define (six-times x) (double (triple x))) \
           (define (triple x) (* x 3)) \
           (six-times 5)) \
         (display (f))",
    );
    assert_eq!(out, "30");
}

#[test]
fn b3_e6a_set_mutates_an_existing_binding_and_it_is_observable_afterward() {
    assert_eq!(
        eval_ok("b3-e6a.ml", "(define v 0) (set! v 1) (display v)"),
        "1"
    );
}

#[test]
fn b3_e6b_mutating_an_undefined_name_is_a_runtime_failure_with_the_runtime_error_exit_code() {
    let file = write_source("b3-e6b.ml", "(set! never-defined 1)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

// A test-design review (qa msg #49) flagged that let/let*/letrec/named-let/
// set! only had happy-path coverage, with none of the scope-edge cases
// below actually exercised -- exactly where binding-form bugs live. Adding
// tests for them surfaced a genuine, previously-unknown bug (below), fixed
// in Ctx::next_slot (src/compiler.rs).

#[test]
fn b3_scope_nested_let_shadows_outer_then_outer_resumes_after_inner_closes() {
    let out = eval_ok(
        "b3-scope-shadow.ml",
        "(display (let ((x 1)) (let ((x 2)) (display x)) (newline) x))",
    );
    assert_eq!(out, "2\n1");
}

#[test]
fn b3_scope_set_bang_on_an_outer_let_local_from_a_nested_scope_mutates_it() {
    let out = eval_ok(
        "b3-scope-nested-set.ml",
        "(display (let ((x 1)) (let ((y 2)) (set! x 99)) x))",
    );
    assert_eq!(out, "99");
}

#[test]
fn b3_scope_letrec_referencing_a_not_yet_initialized_binding_is_a_clean_runtime_error() {
    let file = write_source(
        "b3-scope-letrec-uninit.ml",
        "(display (letrec ((a b) (b 1)) a))",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b3_scope_lambda_body_cannot_see_an_enclosing_lets_locals() {
    // Documented in compile_lambda's own comment: a lambda compiles to a
    // separate chunk with no access to the enclosing frame's locals, so a
    // free reference to an enclosing let's binding falls back to (and
    // fails as) an unbound global, rather than resolving lexically.
    let file = write_source(
        "b3-scope-lambda-no-enclosing.ml",
        "(display (let ((x 5)) ((lambda () x))))",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b3_scope_sequential_sibling_lets_do_not_collide_on_the_same_runtime_slot() {
    // Regression test for a genuine bug found while adding this coverage:
    // Ctx::next_slot was a plain u8, copied (not shared) whenever Ctx was
    // cloned for a new `let`'s extended scope. Two *sequential* (sibling,
    // not nested) lets in the same body each cloned from the same
    // original next_slot and so were assigned the identical slot number
    // at compile time -- but at runtime PUSH_LOCAL only ever grows one
    // flat, never-popped Vec<Value>, so the second let's binding actually
    // landed one slot further along than the compiled GET_LOCAL expected,
    // silently reading the first let's stale value instead of its own.
    let out = eval_ok(
        "b3-scope-sequential-lets.ml",
        "(define (f) (let ((a 1)) (display a)) (newline) (let ((b 2)) (display b))) (f)",
    );
    assert_eq!(out, "1\n2");
}

#[test]
fn b3_e7a_cond_checks_tests_in_order_and_falls_back_to_else() {
    assert_eq!(
        eval_ok("b3-e7a.ml", "(display (cond (#f 1) (#f 2) (else 3)))"),
        "3"
    );
}

#[test]
fn b3_e7b_cond_arrow_variant_applies_a_function_to_the_test_value() {
    assert_eq!(
        eval_ok("b3-e7b.ml", "(display (cond (5 => (lambda (x) (* x 2)))))"),
        "10"
    );
}

#[test]
fn b3_e8a_case_matches_a_group_of_candidate_values() {
    assert_eq!(
        eval_ok(
            "b3-e8a.ml",
            "(display (case 2 ((1 2 3) \"hi\") (else \"bye\")))"
        ),
        "hi"
    );
}

#[test]
fn b3_e8b_case_falls_through_to_else_when_the_key_matches_nothing() {
    assert_eq!(
        eval_ok(
            "b3-e8b.ml",
            "(display (case 99 ((1 2 3) \"hi\") (else \"bye\")))"
        ),
        "bye"
    );
}

#[test]
fn b3_e9a_and_all_truthy_returns_the_last_value() {
    assert_eq!(eval_ok("b3-e9a.ml", "(display (and 1 2 3))"), "3");
}

#[test]
fn b3_e9b_and_short_circuits_on_the_first_falsy_value_without_evaluating_the_rest() {
    let out = eval_ok(
        "b3-e9b.ml",
        "(define fired #f) (and #f (begin (set! fired #t) 1)) (display fired)",
    );
    assert_eq!(out, "#f");
}

#[test]
fn b3_e10a_or_returns_the_first_truthy_value() {
    assert_eq!(eval_ok("b3-e10a.ml", "(display (or #f 'x 'y))"), "x");
}

#[test]
fn b3_e10b_or_all_falsy_returns_the_last_value() {
    // (or #f #f 3) would pass even under a naive "first truthy" rule (3 is
    // truthy), so it can't distinguish "returns 3 because nothing else was
    // truthy" from "returns 3 because it's the first/only truthy value it
    // hit" -- an examiner review on B3 (msg #48) flagged exactly this. Since
    // #f is the only falsy value in this language (per B2), a genuinely
    // all-falsy call needs every argument to be #f.
    assert_eq!(eval_ok("b3-e10b.ml", "(display (or #f #f #f))"), "#f");
}

#[test]
fn b3_e10c_or_short_circuits_on_the_first_truthy_value_without_evaluating_the_rest() {
    let out = eval_ok(
        "b3-e10c.ml",
        "(define fired #f) (or 1 (begin (set! fired #t) 2)) (display fired)",
    );
    assert_eq!(out, "#f");
}

#[test]
fn b3_e11_when_true_runs_the_body() {
    assert_eq!(eval_ok("b3-e11-wt.ml", "(display (when #t 1))"), "1");
}

#[test]
fn b3_e11_when_false_does_not_run_the_body() {
    let out = eval_ok(
        "b3-e11-wf.ml",
        "(define ran #f) (when #f (set! ran #t)) (display ran)",
    );
    assert_eq!(out, "#f");
}

#[test]
fn b3_e11_unless_false_runs_the_body() {
    assert_eq!(eval_ok("b3-e11-uf.ml", "(display (unless #f 1))"), "1");
}

#[test]
fn b3_e11_unless_true_does_not_run_the_body() {
    let out = eval_ok(
        "b3-e11-ut.ml",
        "(define ran #f) (unless #t (set! ran #t)) (display ran)",
    );
    assert_eq!(out, "#f");
}

// E12: the eight DEMO programs, each run as its own process, each producing
// exactly the prescribed output with a trailing newline and exit 0. The
// expectation message gave a description and exact output for each demo but
// not verbatim source text, so these are faithful reconstructions matching
// both the described behaviour and the exact prescribed result.

#[test]
fn b3_e12_demo1_named_loop_sum_one_to_one_hundred() {
    let out = run_demo(
        "b3-e12-demo1.ml",
        "(display (let loop ((i 1) (sum 0)) (if (> i 100) sum (loop (+ i 1) (+ sum i)))))",
    );
    assert_eq!(out, "5050\n");
}

#[test]
fn b3_e12_demo2_sequential_local_bindings_later_depends_on_earlier() {
    let out = run_demo("b3-e12-demo2.ml", "(display (let* ((x 2) (y (* x 3))) y))");
    assert_eq!(out, "6\n");
}

#[test]
fn b3_e12_demo3_cond_apply_function_to_test_value() {
    let out = run_demo(
        "b3-e12-demo3.ml",
        "(display (cond (5 => (lambda (x) (* x 2)))))",
    );
    assert_eq!(out, "10\n");
}

#[test]
fn b3_e12_demo4_case_matching_a_group() {
    let out = run_demo(
        "b3-e12-demo4.ml",
        "(display (case 2 ((1 2 3) \"hi\") (else \"bye\")))",
    );
    assert_eq!(out, "hi\n");
}

#[test]
fn b3_e12_demo5_and_all_truthy_returns_last() {
    let out = run_demo("b3-e12-demo5.ml", "(display (and 1 2 3))");
    assert_eq!(out, "3\n");
}

#[test]
fn b3_e12_demo6_or_returns_first_truthy() {
    let out = run_demo("b3-e12-demo6.ml", "(display (or #f 'x 'y))");
    assert_eq!(out, "x\n");
}

#[test]
fn b3_e12_demo7_mutate_a_variable_then_display_new_value() {
    let out = run_demo("b3-e12-demo7.ml", "(define v 0) (set! v 1) (display v)");
    assert_eq!(out, "1\n");
}

#[test]
fn b3_e12_demo8_mutually_referencing_local_definitions() {
    let out = run_demo(
        "b3-e12-demo8.ml",
        "(define (f) \
           (define (double x) (* x 2)) \
           (define (six-times x) (double (triple x))) \
           (define (triple x) (* x 3)) \
           (six-times 5)) \
         (display (f))",
    );
    assert_eq!(out, "30\n");
}
