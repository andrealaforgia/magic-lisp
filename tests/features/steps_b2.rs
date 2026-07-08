//! Step definitions for features/B2-functions-recursion-conditionals.feature.

use super::registry::Registry;
use super::world::{eval_ok, run, run_pending, stdout_of, write_source};

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- shared: E1, E2, and E9 all phrase their When step as exactly
        // "each is evaluated" with a different Given each time, so the
        // Given populates a pending queue and this one handler runs it.
        .step("each is evaluated", |w, _text, _| {
            run_pending(w, "b2-shared-eval");
        })
        // --- E1 ---
        .step(
            "the expressions \"(display (quote (+ 1 2)))\" and \"(display '(+ 1 2))\"",
            |w, _text, _| {
                w.pending = vec![
                    "(display (quote (+ 1 2)))".to_string(),
                    "(display '(+ 1 2))".to_string(),
                ];
            },
        )
        .step(
            "both display the literal list \"(+ 1 2)\", not the number \"3\"",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(+ 1 2)");
                assert_eq!(w.notes[1], "(+ 1 2)");
            },
        )
        // --- E2 ---
        .step(
            "the four combinations of a true/false condition with a two-branch or one-branch if",
            |w, _text, _| {
                w.pending = vec![
                    "(if #t \"then\" \"else\")".to_string(),
                    "(if #f \"then\" \"else\")".to_string(),
                    "(if #t \"then\")".to_string(),
                    "(if #f \"then\")".to_string(),
                ];
            },
        )
        .step(
            "(if #t \"then\" \"else\") yields \"then\", (if #f \"then\" \"else\") yields \"else\", (if #t \"then\") yields \"then\", and (if #f \"then\") yields the unspecified value which produces no visible output when displayed",
            |w, _text, _| {
                assert_eq!(w.notes[0], "then");
                assert_eq!(w.notes[1], "else");
                assert_eq!(w.notes[2], "then");
                assert_eq!(w.notes[3], "");
            },
        )
        .step("all four exit 0", |_w, _text, _| { /* eval_ok already asserts success for each */
        })
        // --- E3 ---
        .step(
            "a top-level value binding, a fixed-arity function, a fixed-plus-rest function, and an all-rest function",
            |_w, _text, _| {},
        )
        .step("each is defined and called", |w, _text, _| {
            w.notes = vec![
                eval_ok("b2-e3-value.ml", "(define x 42) (display x)"),
                eval_ok(
                    "b2-e3-fixed.ml",
                    "(define (add2 a b) (+ a b)) (display (add2 3 4))",
                ),
                eval_ok(
                    "b2-e3-fixed-rest.ml",
                    "(define (f a b . rest) rest) (display (f 1 2 3 4 5))",
                ),
                eval_ok(
                    "b2-e3-all-rest.ml",
                    "(define (g . args) args) (display (g 1 2 3))",
                ),
            ];
        })
        .step(
            "the fixed-arity call returns the correct value, the fixed-plus-rest call collects the extra arguments into the rest parameter, and the all-rest call collects every argument into its single parameter",
            |w, _text, _| {
                assert_eq!(w.notes[0], "42");
                assert_eq!(w.notes[1], "7");
                assert_eq!(w.notes[2], "(3 4 5)");
                assert_eq!(w.notes[3], "(1 2 3)");
            },
        )
        // --- E4 ---
        .step(
            "a lambda with a fixed-plus-rest formals shape, invoked immediately and also bound via define and called later",
            |_w, _text, _| {},
        )
        .step("each is called", |w, _text, _| {
            w.notes = vec![
                eval_ok(
                    "b2-e4-iife.ml",
                    "(display ((lambda (a . rest) rest) 1 2 3))",
                ),
                eval_ok(
                    "b2-e4-stored.ml",
                    "(define my-fn (lambda (a . rest) rest)) (display (my-fn 10 20 30))",
                ),
            ];
        })
        .step(
            "the rest parameter correctly collects the extra arguments in both cases",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(2 3)");
                assert_eq!(w.notes[1], "(20 30)");
            },
        )
        // --- E5 ---
        .step(
            "\"(begin (display 1) (display 2) 3)\" wrapped in an outer display",
            |w, _text, _| {
                w.notes = vec!["(display (begin (display 1) (display 2) 3))".to_string()];
            },
        )
        .step("it is evaluated", |w, _text, _| {
            let src = w.notes[0].clone();
            w.notes.push(eval_ok("b2-e5.ml", &src));
        })
        .step(
            "the side effects appear in order and only the final expression's value (3) is the begin's own result",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "123");
            },
        )
        // --- E6 ---
        .step(
            "a function X defined first, a function A defined afterward that calls X, a call to A, then a redefinition of X, then another call to A",
            |_w, _text, _| {},
        )
        .step("A is called before and after X's redefinition", |w, _text, _| {
            let out = eval_ok(
                "b2-e6-redef.ml",
                "(define (x) 1) \
                 (define (a) (x)) \
                 (display (a)) (newline) \
                 (define (x) 2) \
                 (display (a)) (newline)",
            );
            w.notes = vec![out];
        })
        .step(
            "A returns the old X's result the first time and the new X's result the second time",
            |w, _text, _| {
                assert_eq!(w.notes[0], "1\n2\n");
            },
        )
        // --- E7 ---
        .step("a recursive factorial function", |_w, _text, _| {})
        .step(
            "it is called with an argument requiring multiple recursive steps (not just the base case)",
            |w, _text, _| {
                let out = eval_ok(
                    "b2-e7-fact.ml",
                    "(define (fact n) (if (< n 2) 1 (* n (fact (- n 1))))) (display (fact 5))",
                );
                w.notes = vec![out];
            },
        )
        .step(
            "it terminates and returns the mathematically correct result",
            |w, _text, _| {
                assert_eq!(w.notes[0], "120");
            },
        )
        // --- E8 ---
        .step(
            "a function `tap` that displays its argument and returns it, called as two arguments to an outer +, itself displayed",
            |_w, _text, _| {},
        )
        .step("\"(display (+ (tap 1) (tap 2)))\" is evaluated", |w, _text, _| {
            let out = eval_ok(
                "b2-e8-order.ml",
                "(define (tap x) (display x) x) (display (+ (tap 1) (tap 2)))",
            );
            w.notes = vec![out];
        })
        .step(
            "the visible output is \"123\" — tap(1)'s effect, then tap(2)'s effect, then the outer display of the sum",
            |w, _text, _| {
                assert_eq!(w.notes[0], "123");
            },
        )
        // --- E9 ---
        .step(
            "\"(if 0 \"truthy\" \"falsy\")\" and \"(if '() \"truthy\" \"falsy\")\"",
            |w, _text, _| {
                w.pending = vec![
                    "(if 0 \"truthy\" \"falsy\")".to_string(),
                    "(if '() \"truthy\" \"falsy\")".to_string(),
                ];
            },
        )
        .step("both take the then-branch and yield \"truthy\"", |w, _text, _| {
            assert_eq!(w.notes[0], "truthy");
            assert_eq!(w.notes[1], "truthy");
        })
        // --- E10 ---
        .step(
            "`-` and `*` called with 2 and with 4+ numeric arguments, and each of `=`, `<`, `<=`, `>`, `>=` called with 2 args, with 4 args holding across the whole sequence, and with a chain-breaking case where the two endpoints alone would give the wrong answer",
            |_w, _text, _| {},
        )
        .step(
            "`-`/`*` compute the correct variadic result, and each comparison operator correctly reports true only when the relation holds across every adjacent pair in the sequence",
            |_w, _text, _| {
                assert_eq!(eval_ok("b2-e10-m2.ml", "(display (- 10 3))"), "7");
                assert_eq!(eval_ok("b2-e10-m4.ml", "(display (- 20 1 2 3 4))"), "10");
                assert_eq!(eval_ok("b2-e10-t2.ml", "(display (* 3 4))"), "12");
                assert_eq!(eval_ok("b2-e10-t4.ml", "(display (* 1 2 3 4))"), "24");
                assert_eq!(eval_ok("b2-e10-lt-ok.ml", "(display (< 1 2 3))"), "#t");
                assert_eq!(eval_ok("b2-e10-lt-bad.ml", "(display (< 1 3 2))"), "#f");
                assert_eq!(eval_ok("b2-e10-eq2.ml", "(display (= 2 2))"), "#t");
                assert_eq!(eval_ok("b2-e10-eq4.ml", "(display (= 2 2 2 2))"), "#t");
                assert_eq!(eval_ok("b2-e10-eq-bad.ml", "(display (= 2 3 2))"), "#f");
                assert_eq!(eval_ok("b2-e10-le2.ml", "(display (<= 1 2))"), "#t");
                assert_eq!(eval_ok("b2-e10-le4.ml", "(display (<= 1 2 2 3))"), "#t");
                assert_eq!(eval_ok("b2-e10-le-bad.ml", "(display (<= 1 3 2))"), "#f");
                assert_eq!(eval_ok("b2-e10-gt2.ml", "(display (> 3 1))"), "#t");
                assert_eq!(eval_ok("b2-e10-gt4.ml", "(display (> 5 3 2 1))"), "#t");
                assert_eq!(eval_ok("b2-e10-gt-bad.ml", "(display (> 5 1 3))"), "#f");
                assert_eq!(eval_ok("b2-e10-ge2.ml", "(display (>= 2 1))"), "#t");
                assert_eq!(eval_ok("b2-e10-ge4.ml", "(display (>= 5 5 4 4))"), "#t");
                assert_eq!(eval_ok("b2-e10-ge-bad.ml", "(display (>= 5 4 5))"), "#f");
            },
        )
        // --- E11 ---
        .step("the maximum representable integer plus one", |_w, _text, _| {})
        .step(
            "\"(display (+ 9223372036854775807 1))\" is evaluated",
            |w, _text, _| {
                let out = eval_ok(
                    "b2-e11.ml",
                    "(display (+ 9223372036854775807 1))",
                );
                w.notes = vec![out];
            },
        )
        .step(
            "the result wraps to the minimum representable integer, not an error and not a bignum",
            |w, _text, _| {
                assert_eq!(w.notes[0], i64::MIN.to_string());
            },
        )
        // --- E12 ---
        .step("a negative whole number and both boolean values", |_w, _text, _| {})
        .step("each is displayed", |w, _text, _| {
            w.notes = vec![
                eval_ok("b2-e12-int.ml", "(display -12345)"),
                eval_ok("b2-e12-bools.ml", "(display #t) (display #f)"),
            ];
        })
        .step(
            "the number prints in ordinary decimal form and the booleans print as \"#t\" and \"#f\"",
            |w, _text, _| {
                assert_eq!(w.notes[0], "-12345");
                assert_eq!(w.notes[1], "#t#f");
            },
        )
        // --- E13 ---
        .step(
            "a program that defines `fact` twice at the top level (a stub, then the real recursive definition using if/</*/-), then displays (fact 10) followed by a newline",
            |w, _text, _| {
                w.notes = vec![
                    "(define (fact n) (this-stub-would-error-if-called n))\n\
                     (define (fact n) (if (< n 2) 1 (* n (fact (- n 1)))))\n\
                     (display (fact 10))\n\
                     (newline)\n"
                        .to_string(),
                ];
            },
        )
        .step("it is run", |w, _text, _| {
            let src = w.notes.last().unwrap().clone();
            let file = write_source("b2-e13.ml", &src);
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step(
            "stdout is exactly \"3628800\\n\" and the process exits 0, proving redefinition (E6), recursion (E7), conditionals (E2), and variadic arithmetic (E10) all compose correctly together",
            |w, _text, _| {
                assert_eq!(stdout_of(w.last_output()), "3628800\n");
                assert_eq!(w.last_output().status.code(), Some(0));
            },
        )
        .step("a program that displays the result of (if #f #f)", |w, _text, _| {
            w.notes.push("(display (if #f #f))".to_string());
        })
        .step(
            "no visible output is produced for that value and the process exits 0",
            |w, _text, _| {
                assert_eq!(stdout_of(w.last_output()), "");
                assert_eq!(w.last_output().status.code(), Some(0));
            },
        )
}
