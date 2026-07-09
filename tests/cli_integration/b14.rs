//! B14: procedural macros (`define-macro`) and `gensym`.

use super::helpers::{eval_ok, run, run_demo, run_with_stdin, stderr_of, write_source};
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
fn a_macro_returning_a_cons_list_hybrid_compiles_as_a_proper_list_not_a_dotted_pair() {
    // Regression test for qa test-design review msg #262: `(cons '+ '(1
    // 2 3))` is semantically the proper list `(+ 1 2 3)`, not a dotted
    // pair whose tail happens to be a list -- the existing unit test for
    // this fix only constructs the `Value::Pair`/`Value::List` tree
    // directly in Rust; this exercises the same code path through real
    // MagicLisp source, as this project's convention requires.
    assert_eq!(
        eval_ok(
            "b14-cons-list-hybrid.ml",
            "(define-macro (m) (cons (quote +) (quote (1 2 3)))) (display (m))"
        ),
        "6"
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
    // Examiner msg #281: the spec requires the round limit to be at least
    // 1000, not merely some clean error at whatever number -- assert the
    // specific number the error reports so a regression back down to a
    // too-low limit (e.g. the previous 100) is actually caught here.
    assert!(
        stderr.contains("1000"),
        "expected the round-limit error to report exactly the raised limit (>= 1000), got: {stderr}"
    );
}

#[test]
fn b14_e3_a_legitimately_long_macro_expansion_chain_still_completes() {
    // Examiner msg #281 (c): demonstrates the raised ceiling actually
    // supports more real rounds, not just a moved boundary number -- a
    // macro that re-expands itself 501 times (counting down from 500 to
    // 0), well past the old 100-round limit but comfortably under the new
    // 1000-round one, must still settle on its final value and compile.
    assert_eq!(
        eval_ok(
            "b14-e3-long-chain.ml",
            "(define-macro (count-down n) \
               (if (= n 0) 1 (list (quote count-down) (- n 1)))) \
             (display (count-down 500))"
        ),
        "1"
    );
}

#[test]
fn a_macro_body_containing_a_genuine_infinite_tail_recursive_loop_fails_cleanly_not_a_hang() {
    // Regression test for warden security review msg #260 (Critical):
    // unlike the round-limited macro-EXPANSION chain above (each round is
    // a fresh, bounded compile_expr call), this loop lives entirely
    // WITHIN one macro invocation's own execution, run via the same
    // tail-call trampoline an ordinary program uses -- so it never trips
    // MAX_CALL_DEPTH, and nothing else bounded it before this fix. Uses
    // `run_with_stdin` (not the plain `run` helper) specifically because
    // it enforces a real timeout+kill instead of blocking the whole test
    // suite indefinitely if this regresses.
    let file = write_source(
        "b14-macro-body-infinite-loop.ml",
        "(define-macro (evil) \
           (letrec ((loop (lambda () (loop)))) \
             (loop))) \
         (evil)",
    );
    let output = run_with_stdin(&["eval", file.to_str().unwrap()], b"");
    assert_eq!(
        output.status.code(),
        Some(SOURCE_ERROR),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_re_expanding_across_many_rounds_cannot_multiply_its_step_budget_by_the_round_count() {
    // Regression test for warden security review msg #265: the per-round
    // step budget alone isn't enough -- a macro that legitimately
    // re-expands into itself (the same mechanism b14_e3 above tests) and
    // burns close to a full budget's worth of trampoline hops on EACH of
    // its rounds could previously cost up to (budget x round count), and
    // that cost multiplies further with however many independent call
    // sites a file contains, since nothing tracked cumulative hops across
    // rounds or call sites. A tiny source file could still reach tens of
    // seconds of compile time despite no single round ever exceeding its
    // own bound. Uses `run_with_stdin` for the same real timeout+kill
    // safety net as the single-round case above.
    let file = write_source(
        "b14-macro-cumulative-step-budget.ml",
        "(define-macro (loopy k) \
           (letrec ((burn (lambda (n) (if (= n 0) 0 (burn (- n 1)))))) \
             (if (= k 0) \
                 42 \
                 (begin (burn 999000) (list (quote loopy) (- k 1)))))) \
         (loopy 99)",
    );
    let output = run_with_stdin(&["eval", file.to_str().unwrap()], b"");
    assert_eq!(
        output.status.code(),
        Some(SOURCE_ERROR),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_re_expanding_across_many_rounds_cannot_multiply_its_conversion_cost_by_the_round_count()
{
    // Regression test for warden security review msgs #292/#293: unlike
    // the trampoline-step budget above (which a bulk-allocating native
    // like `make-vector` bypasses almost entirely, since it returns
    // directly from `call_native` without re-entering the trampoline loop
    // that budget decrements), the conversion cost of turning each round's
    // returned near-`MAX_MACRO_RESULT_ELEMENTS`-sized value back into
    // source code was NOT capped cumulatively -- a macro re-expanding
    // hundreds of times, each round returning a fresh large vector, paid
    // its full per-round conversion cost every round with nothing summing
    // the total. Independently confirmed to complete in ~2s (previously
    // capped at 0.56s under the old 100-round expansion limit) before this
    // fix; must now fail cleanly and fast instead.
    let file = write_source(
        "b14-macro-cumulative-conversion-budget.ml",
        "(define-macro (bomb n v) \
           (if (= n 0) 42 (list (quote bomb) (- n 1) (make-vector 99999)))) \
         (display (bomb 999 0))",
    );
    let output = run_with_stdin(&["eval", file.to_str().unwrap()], b"");
    assert_eq!(
        output.status.code(),
        Some(SOURCE_ERROR),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_returning_a_self_referential_vector_fails_cleanly_not_a_crash() {
    // Regression test for qa test-design WARNING msg #259: `value_to_sexpr`
    // (converting a macro's returned data back into code) had cycle
    // detection for a self-referential `Pair` chain but not for a
    // self-referential `Vector` (`vector-set!`ing one of its own elements
    // back to itself) -- confirmed to crash the compiling process outright
    // before the fix.
    let file = write_source(
        "b14-vector-cycle.ml",
        "(define-macro (bad) \
           (let ((v (vector 1 2))) (vector-set! v 0 v) v)) \
         (bad)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_returning_a_deeply_nested_non_cyclic_value_fails_cleanly_not_a_crash() {
    // Regression test for qa test-design WARNING msg #259: a macro
    // returning a value nested (not cyclic) far deeper than
    // `MAX_NESTING_DEPTH` crashed the compiling process outright before
    // the fix -- `value_to_sexpr` had no depth bound of its own, and
    // `compile_expr`'s own downstream guard on the fully-converted tree
    // never got a chance to run if converting it was itself what crashed.
    let file = write_source(
        "b14-deeply-nested-macro-result.ml",
        "(define-macro (deep) \
           (let loop ((n 1000) (acc 1)) \
             (if (= n 0) acc (loop (- n 1) (list acc))))) \
         (deep)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_returning_an_excessively_large_flat_list_fails_cleanly_not_a_disproportionate_cost() {
    // Regression test for qa/warden msg #260: unlike quasiquote templates
    // or `make-vector`, nothing bounded how large a FLAT (not deep --
    // `deep` above targets nesting specifically) list/vector a macro
    // could build via ordinary recursive `cons` and return, letting a
    // tiny source file force disproportionate compile-time cost purely by
    // choosing a large numeric literal.
    let file = write_source(
        "b14-oversized-flat-macro-result.ml",
        "(define-macro (huge) \
           (let loop ((n 200000) (acc (quote ()))) \
             (if (= n 0) (cons (quote quote) (cons acc (quote ()))) (loop (- n 1) (cons n acc))))) \
         (huge)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_returning_an_oversized_vector_directly_fails_cleanly_not_a_disproportionate_cost() {
    // Regression test distinguishing the size cap's own up-front check
    // (for a `Value::Vector` already built to its full size in one step,
    // e.g. via `make-vector`) from the incremental check the `cons`-chain
    // case above exercises -- a macro whose result is a `Vector` never
    // passes through any smaller intermediate size on its way to this
    // one, so a check that only fires at an exact element count (rather
    // than any count past the limit) would never observe it.
    let file = write_source(
        "b14-oversized-vector-macro-result.ml",
        "(define-macro (huge) (make-vector 200000)) (huge)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn a_macro_returning_an_oversized_flat_list_directly_fails_cleanly_not_a_disproportionate_cost() {
    // Same distinction as the vector case above, but for a `Value::List`
    // built directly to its full size in one step, rather than
    // incrementally via `cons` -- NOT via `vector->list` (warden security
    // review msg #269: `vector->list` is implemented through
    // `vec_to_list`, which builds a `Pair` CHAIN terminating in an empty
    // list, the same shape the incremental cons-loop test already
    // exercises, never reaching the up-front `Value::List`-arm check at
    // all). A rest parameter is bound as a genuine flat `Value::List`
    // directly (see `bind_arguments`), so spreading a large list of
    // arguments onto a rest-only lambda via `apply` produces one in a
    // single step instead.
    let file = write_source(
        "b14-oversized-list-macro-result.ml",
        "(define-macro (huge) \
           (apply (lambda args args) \
             (let loop ((n 200000) (acc (quote ()))) \
               (if (= n 0) acc (loop (- n 1) (cons n acc)))))) \
         (huge)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    // Specifically the size-cap's own error, not just any clean
    // SOURCE_ERROR (warden security review msg #272): once the expansion
    // isn't rejected for size, it becomes an ordinary 199,999-argument
    // call, which an entirely unrelated, coincidental limit
    // (compile_expr's own argument-count check, 255) also rejects with
    // the same exit code -- a bare exit-code assertion can't tell "the
    // intended check fired" apart from "some other limit happened to
    // produce the same class of failure."
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("more than 100000 elements"),
        "expected the size-cap's own error, got: {stderr}"
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
fn gensym_results_from_two_separate_macros_do_not_collide_through_the_real_compiler_path() {
    // Regression test for qa test-design review msg #264: the existing
    // coverage for gensym's cross-invocation hygiene fix only threads a
    // counter through two direct `eval_top_level_function` calls -- it
    // never goes through `compile_macro_call` (the real path an ordinary
    // program actually exercises) at all. Two SEPARATE macro definitions,
    // each calling `(gensym)` during their own compile-time expansion,
    // is what would have silently collided (both producing the identical
    // symbol) before the fix.
    assert_eq!(
        eval_ok(
            "b14-gensym-cross-macro-hygiene.ml",
            "(define-macro (g1) (list (quote quote) (gensym))) \
             (define-macro (g2) (list (quote quote) (gensym))) \
             (display (eq? (g1) (g2)))"
        ),
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
