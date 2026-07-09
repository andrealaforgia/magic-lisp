//! B14: procedural macros (`define-macro`) and `gensym`.

use super::helpers::{eval_ok, run, run_demo, stderr_of, write_source};
use magiclisp::exitcode::SOURCE_ERROR;

#[test]
fn b14_e1_operands_are_handed_to_the_macro_body_as_literal_unevaluated_data() {
    // The operand references an undefined name -- if it were ever
    // evaluated before reaching the macro body, this would be a runtime
    // error instead of a clean display of the literal form.
    assert_eq!(
        eval_ok(
            "b14-e1a.ml",
            "(define-macro (show-literally x) `(quote ,x)) \
             (display (show-literally (undefined-function 1 2)))"
        ),
        "(undefined-function 1 2)"
    );
    // A rest parameter collects every trailing operand form, unevaluated,
    // as a single list of data -- none dropped, none partially evaluated.
    assert_eq!(
        eval_ok(
            "b14-e1b.ml",
            "(define-macro (collect . rest) `(quote ,rest)) \
             (display (collect (a 1) (b 2) (c 3)))"
        ),
        "((a 1) (b 2) (c 3))"
    );
}

#[test]
fn b14_e2_the_expansion_is_itself_evaluated_and_macros_are_visible_in_later_defined_functions() {
    assert_eq!(
        eval_ok(
            "b14-e2.ml",
            "(define-macro (double x) `(* ,x 2)) \
             (define (use-it n) (double n)) \
             (display (use-it 5))"
        ),
        "10"
    );
}

#[test]
fn b14_e3_a_macro_expanding_to_another_macro_call_re_expands_until_its_settled() {
    assert_eq!(
        eval_ok(
            "b14-e3a.ml",
            "(define-macro (my-when test . body) `(if ,test (begin ,@body) #f)) \
             (define-macro (my-unless test . body) `(my-when (not ,test) ,@body)) \
             (my-unless #f (display \"hi\"))"
        ),
        "hi"
    );
}

#[test]
fn b14_e3_a_macro_that_always_expands_into_another_macro_call_fails_cleanly_not_a_hang() {
    let file = write_source(
        "b14-e3b.ml",
        "(define-macro (loop-forever) `(loop-forever)) (loop-forever)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    // Specifically the macro-expansion-round guard, not merely *some* clean
    // error: an infinite macro chain also keeps incrementing the ordinary
    // expression-nesting depth on its way through `compile_expr` each
    // round, so a broken round-limit check could still fail cleanly by
    // hitting THAT unrelated guard instead, without ever exercising the
    // one this test targets.
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("macro") && stderr.contains("recursion"),
        "expected the macro-expansion-round guard's own error, got: {stderr}"
    );
}

#[test]
fn b14_e4_gensym_produces_a_symbol_distinct_from_every_other_symbol() {
    assert_eq!(
        eval_ok("b14-e4a.ml", "(display (eq? (gensym) (gensym)))"),
        "#f"
    );
    assert_eq!(
        eval_ok("b14-e4b.ml", "(display (eq? (gensym) (quote g1)))"),
        "#f"
    );
}

#[test]
fn b14_e5_a_local_variable_shadowing_a_macro_name_wins_over_the_macro() {
    // If the macro won, `(trap 1)` would expand to `(quote 1)` and display
    // "1" without ever calling the passed-in procedure. Since the local
    // parameter wins, `(trap 1)` calls the procedure bound to it -- with
    // its operand actually evaluated first, unlike the macro's own
    // behaviour -- producing 101, not 1.
    assert_eq!(
        eval_ok(
            "b14-e5.ml",
            "(define-macro (trap x) `(quote ,x)) \
             (define (f trap) (trap 1)) \
             (display (f (lambda (n) (+ n 100))))"
        ),
        "101"
    );
}

#[test]
fn b14_e5_a_letrec_bound_alias_shadowing_a_macro_name_also_wins_over_the_macro() {
    // The same rule as the ordinary-parameter case above, but exercising a
    // DIFFERENT one of the three kinds of binding that can shadow a macro
    // name: `letrec` (like `let`/named-let self-reference) binds its names
    // via an alias to a gensym'd global, not an ordinary local slot -- a
    // distinct code path from a ordinary lambda/`let` parameter. If only
    // the local-slot check were wired up, this specific shadowing kind
    // would be missed and `(m 5)` would incorrectly expand as a macro
    // call, displaying the macro's own inert result instead of ever
    // calling the letrec-bound lambda.
    assert_eq!(
        eval_ok(
            "b14-e5b.ml",
            "(define-macro (m x) `(quote not-called)) \
             (letrec ((m (lambda (x) (+ x 1)))) (display (m 5)))"
        ),
        "6"
    );
}

#[test]
fn b14_e6_the_swap_macro_uses_gensym_internally_to_avoid_colliding_with_its_own_operands() {
    assert_eq!(
        eval_ok(
            "b14-e6.ml",
            "(define-macro (swap a b) \
               (let ((tmp (gensym))) \
                 `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp)))) \
             (define x 1) (define y 2) \
             (swap x y) \
             (write (list x y))"
        ),
        "(2 1)"
    );
}

#[test]
fn b14_e7_all_four_demos_produce_exactly_the_prescribed_output() {
    assert_eq!(
        run_demo(
            "b14-e7.ml",
            "(define-macro (swap a b) \
               (let ((tmp (gensym))) \
                 `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp)))) \
             (define x 1) (define y 2) \
             (swap x y) \
             (write (list x y))"
        ),
        "(2 1)\n"
    );
    assert_eq!(
        run_demo(
            "b14-e7b.ml",
            "(define-macro (double x) `(* ,x 2)) \
             (define (use-it n) (double n)) \
             (display (use-it 5))"
        ),
        "10\n"
    );
    assert_eq!(
        run_demo(
            "b14-e7c.ml",
            "(define-macro (my-when test . body) `(if ,test (begin ,@body) #f)) \
             (define-macro (my-unless test . body) `(my-when (not ,test) ,@body)) \
             (my-unless #f (display \"hi\"))"
        ),
        "hi\n"
    );
    assert_eq!(
        run_demo("b14-e7d.ml", "(display (eq? (gensym) (gensym)))"),
        "#f\n"
    );
}
