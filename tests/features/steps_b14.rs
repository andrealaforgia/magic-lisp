//! Step definitions for features/B14-macros-and-gensym.feature.

use super::registry::Registry;
use super::world::{eval_ok, run, stderr_of};
use magiclisp::exitcode::SOURCE_ERROR;

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a macro whose operand references an undefined name, and a macro with a rest parameter given several trailing operand forms",
            |_w, _text, _| {},
        )
        .step("each macro is called", |w, _text, _| {
            w.notes.push(eval_ok(
                "b14-e1a",
                "(define-macro (show-literally x) `(quote ,x)) \
                 (display (show-literally (undefined-function 1 2)))",
            ));
            w.notes.push(eval_ok(
                "b14-e1b",
                "(define-macro (collect . rest) `(quote ,rest)) \
                 (display (collect (a 1) (b 2) (c 3)))",
            ));
        })
        .step(
            "the undefined-name operand is returned as literal data without erroring (proving it was never evaluated), and all trailing forms are collected as unevaluated data via the rest parameter",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(undefined-function 1 2)");
                assert_eq!(w.notes[1], "((a 1) (b 2) (c 3))");
            },
        )
        // --- E2 ---
        .step(
            "a macro expanding to an arithmetic expression, used inside a function defined after the macro",
            |_w, _text, _| {},
        )
        .step("the function is called", |w, _text, _| {
            w.notes.push(eval_ok(
                "b14-e2",
                "(define-macro (double x) `(* ,x 2)) \
                 (define (use-it n) (double n)) \
                 (display (use-it 5))",
            ));
        })
        .step(
            "real arithmetic happens on the expanded code, proving both genuine evaluation and forward visibility into the later-defined function body",
            |w, _text, _| {
                assert_eq!(w.notes[0], "10");
            },
        )
        // --- E3 ---
        .step(
            "a macro that expands into another macro call (two legitimate rounds), a macro engineered to expand into itself forever, and a macro that legitimately re-expands 500 times before settling",
            |_w, _text, _| {},
        )
        .step("each is compiled and run", |w, _text, _| {
            w.notes.push(eval_ok(
                "b14-e3a",
                "(define-macro (my-when test . body) `(if ,test (begin ,@body) #f)) \
                 (define-macro (my-unless test . body) `(my-when (not ,test) ,@body)) \
                 (my-unless #f (display \"hi\"))",
            ));
            let file = super::world::write_source(
                "b14-e3b",
                "(define-macro (loop-forever) `(loop-forever)) (loop-forever)",
            );
            w.outputs
                .push(run(&["eval", file.to_str().unwrap()]));
            w.notes.push(eval_ok(
                "b14-e3c",
                "(define-macro (count-down n) \
                   (if (= n 0) 1 (list (quote count-down) (- n 1)))) \
                 (display (count-down 500))",
            ));
        })
        .step(
            "the two-round case completes correctly, the infinite case fails cleanly with a distinct non-zero exit code at a limit of at least 1000 (not a hang or crash), and the 500-round legitimate case completes successfully — proving the raised ceiling supports real additional rounds, not just a relocated failure boundary",
            |w, _text, _| {
                assert_eq!(w.notes[0], "hi");
                let runaway = w.last_output();
                assert_eq!(runaway.status.code(), Some(SOURCE_ERROR));
                let stderr = stderr_of(runaway);
                assert!(
                    stderr.contains("macro") && stderr.contains("recursion"),
                    "expected the macro-expansion-round guard's own error, got: {stderr}"
                );
                assert!(
                    stderr.contains("1000"),
                    "expected the round-limit error to report at least 1000, got: {stderr}"
                );
                assert_eq!(w.notes[1], "1");
            },
        )
        // --- E4 ---
        .step(
            "two separate gensym calls, and a gensym result compared against an ordinary source-written symbol",
            |_w, _text, _| {},
        )
        .step("identity is checked in each case", |w, _text, _| {
            w.notes
                .push(eval_ok("b14-e4a", "(display (eq? (gensym) (gensym)))"));
            w.notes.push(eval_ok(
                "b14-e4b",
                "(display (eq? (gensym) (quote g1)))",
            ));
        })
        .step(
            "both comparisons report unequal, proving the uniqueness guarantee is genuinely global, not just relative to other gensym calls",
            |w, _text, _| {
                assert_eq!(w.notes[0], "#f");
                assert_eq!(w.notes[1], "#f");
            },
        )
        // --- E5 ---
        .step(
            "a macro name also bound as an ordinary function parameter, called with a procedure argument",
            |_w, _text, _| {},
        )
        .step(
            "the parameter name is used as an operator inside the function body",
            |w, _text, _| {
                w.notes.push(eval_ok(
                    "b14-e5",
                    "(define-macro (trap x) `(quote ,x)) \
                     (define (f trap) (trap 1)) \
                     (display (f (lambda (n) (+ n 100))))",
                ));
            },
        )
        .step(
            "the local parameter's value is used (its operand IS evaluated normally, unlike the macro's own unevaluated-operand behavior) — the macro never triggers within that scope",
            |w, _text, _| {
                assert_eq!(w.notes[0], "101");
            },
        )
        // --- E6 ---
        .step(
            "two variables and a swap macro that generates its own temporary name via gensym",
            |_w, _text, _| {},
        )
        .step(
            "the variables are swapped via the macro and their new values printed",
            |w, _text, _| {
                w.notes.push(eval_ok(
                    "b14-e6",
                    "(define-macro (swap a b) \
                       (let ((tmp (gensym))) \
                         `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp)))) \
                     (define x 1) (define y 2) \
                     (swap x y) \
                     (write (list x y))",
                ));
            },
        )
        .step(
            "they are correctly swapped, proving macro definition, unevaluated-operand handling, generated-code evaluation, and gensym all work together for a realistic macro",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(2 1)");
            },
        )
        // --- E7 ---
        .step(
            "all four DEMOs from the behaviour spec run together in one program, in order",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            w.notes.push(eval_ok(
                "b14-e7",
                "(define-macro (swap a b) \
                   (let ((tmp (gensym))) \
                     `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp)))) \
                 (define x 1) (define y 2) \
                 (swap x y) \
                 (write (list x y)) (newline) \
                 (define-macro (double n) `(* ,n 2)) \
                 (define (use-it n) (double n)) \
                 (display (use-it 5)) (newline) \
                 (define-macro (my-when test . body) `(if ,test (begin ,@body) #f)) \
                 (define-macro (my-unless test . body) `(my-when (not ,test) ,@body)) \
                 (my-unless #f (display \"hi\")) (newline) \
                 (display (eq? (gensym) (gensym)))",
            ));
        })
        .step(
            "each produces exactly its prescribed output, and the process exits 0",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(2 1)\n10\nhi\n#f");
            },
        )
}
