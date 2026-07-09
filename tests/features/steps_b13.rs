//! Step definitions for features/B13-quasiquotation.feature.

use super::registry::Registry;
use super::world::{eval_ok, run_pending};

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a backquoted template containing an expression that would evaluate very differently if it were code",
            |w, _text, _| {
                w.pending = vec!["`(+ 1 2)".to_string()];
            },
        )
        .step("it is displayed", |w, _text, _| {
            run_pending(w, "b13-e1");
        })
        .step(
            "it shows the literal written structure, not the result of evaluating it",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "(+ 1 2)");
            },
        )
        // --- E2 ---
        .step(
            "a template with one unquote marker, a template with two separate unquote markers, and a template unquoting a variable bound to a list",
            |w, _text, _| {
                w.pending = vec![
                    "(define x 10) (display `(a ,x c))".to_string(),
                    "(define x 1) (define y 2) (display `(,x mid ,y))".to_string(),
                    "(define mid (list 2 3 4)) (display `(1 ,mid 5))".to_string(),
                ];
            },
        )
        .step("each is displayed", |w, _text, _| {
            run_pending(w, "b13-each");
        })
        .step(
            "each marked spot is independently evaluated and substituted, and a list-valued unquote is inserted as ONE nested element, not flattened",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(a 10 c)");
                assert_eq!(w.notes[1], "(1 mid 2)");
                assert_eq!(w.notes[2], "(1 (2 3 4) 5)");
            },
        )
        // --- E3 ---
        .step(
            "the same list-valued variable as E2, spliced instead of unquoted, plus an inline list splice, an empty-list splice, and a splice with elements on both sides",
            |w, _text, _| {
                w.pending = vec![
                    "(define mid (list 2 3 4)) (display `(1 ,@mid 5))".to_string(),
                    "(display `(1 ,@(list 2 3) 4))".to_string(),
                    "(display `(1 ,@(list) 2))".to_string(),
                    "(display `(0 1 ,@(list 2 3) 4 5))".to_string(),
                ];
            },
        )
        .step(
            "the list's elements are spliced in directly (contrasting directly with E2's single-element insertion on the same value), an empty splice contributes zero elements, and surrounding elements remain correctly adjacent",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(1 2 3 4 5)");
                assert_eq!(w.notes[1], "(1 2 3 4)");
                assert_eq!(w.notes[2], "(1 2)");
                assert_eq!(w.notes[3], "(0 1 2 3 4 5)");
            },
        )
        // --- E4 ---
        .step(
            "a doubly-nested template where a doubly-marked spot brings the nesting level to zero, and a contrasting template where a single marker only lowers the level partway",
            |w, _text, _| {
                w.pending = vec![
                    "(define y 5) (display `(a `(b ,,y)))".to_string(),
                    "(define y 5) (display `(a `(b ,y)))".to_string(),
                ];
            },
        )
        .step(
            "the doubly-marked spot is evaluated while its surrounding inner quasiquote/unquote survive as literal tagged data, and in the contrasting case the singly-marked variable is NOT substituted at all — the level never reaches zero",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(a (quasiquote (b (unquote 5))))");
                assert_eq!(w.notes[1], "(a (quasiquote (b (unquote y))))");
            },
        )
        // --- E5 ---
        .step(
            "a vector template with an unquote marker and a vector template with an unquote-splicing marker",
            |w, _text, _| {
                w.pending = vec![
                    "(define x 10) (display `#(1 ,x 3))".to_string(),
                    "(display `#(1 ,@(list 2 3) 4))".to_string(),
                ];
            },
        )
        .step(
            "unquote substitutes a single value and unquote-splicing flattens a list's elements, exactly as in list templates",
            |w, _text, _| {
                assert_eq!(w.notes[0], "#(1 10 3)");
                assert_eq!(w.notes[1], "#(1 2 3 4)");
            },
        )
        // --- E6 ---
        .step(
            "all five DEMO expressions from the behaviour spec run together in one program",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let out = eval_ok(
                "b13-e6.ml",
                "(define mid (list 2 3 4)) (write `(1 ,@mid 5)) (newline) \
                 (define x 10) (display `(a ,x c)) (newline) \
                 (display `(1 ,@(list 2 3) 4)) (newline) \
                 (display `#(1 ,x 3)) (newline) \
                 (define y 5) (display `(a `(b ,,y))) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "each line of output matches its prescribed value exactly, and the process exits 0",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "(1 2 3 4 5)\n(a 10 c)\n(1 2 3 4)\n#(1 10 3)\n(a (quasiquote (b (unquote 5))))\n"
                );
            },
        )
}
