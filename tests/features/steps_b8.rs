//! Step definitions for features/B8-type-predicates-and-equality.feature.

use super::registry::Registry;
use super::world::eval_ok;

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "two separately-written same-named symbols, two separately-built pairs with identical contents, the same pair bound to two names, two separately-built strings with identical contents, and the same string bound to two names",
            |_w, _text, _| {},
        )
        .step("eq? is applied to each pair", |w, _text, _| {
            let out = eval_ok(
                "b8-e1.ml",
                "(display (eq? (quote a) (quote a))) (newline) \
                 (display (eq? (cons 1 2) (cons 1 2))) (newline) \
                 (define p (cons 1 2)) (define q p) (display (eq? p q)) (newline) \
                 (display (eq? \"ab\" \"ab\")) (newline) \
                 (define s \"ab\") (define t s) (display (eq? s t)) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "simple values (symbols) compare equal when they're the same value, while separately-built compound values compare unequal and only the literally-same object compares equal",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#t\n#f\n#t\n#f\n#t\n");
            },
        )
        // --- E2 ---
        .step(
            "an integer and a float of the same magnitude, positive and negative zero, two independently-computed equal floats, and two NaN floats",
            |_w, _text, _| {},
        )
        .step("eqv? is applied to each pair", |w, _text, _| {
            let out = eval_ok(
                "b8-e2.ml",
                "(display (eqv? 1 1.0)) (newline) \
                 (display (eqv? 0.0 -0.0)) (newline) \
                 (display (eqv? (+ 0.5 0.5) 1.0)) (newline) \
                 (display (eqv? (/ 0.0 0.0) (/ 0.0 0.0))) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "an integer never compares equal to a float, positive and negative zero compare unequal, two NaNs compare equal to each other, and two independently-computed equal floats compare equal",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#f\n#f\n#t\n#t\n");
            },
        )
        // --- E3 ---
        .step(
            "two separately-built lists with identical contents, two separately-built strings with identical contents, two separately-built nested lists (a list containing a list), an integer vs a float of the same magnitude, and a large non-circular list built two separate times",
            |_w, _text, _| {},
        )
        .step("equal? is applied to each pair", |w, _text, _| {
            let out = eval_ok(
                "b8-e3.ml",
                "(display (equal? (cons 1 (cons 2 (quote ()))) (cons 1 (cons 2 (quote ()))))) (newline) \
                 (display (equal? \"ab\" \"ab\")) (newline) \
                 (display (equal? (cons 1 (cons (cons 2 (quote ())) (quote ()))) \
                                  (cons 1 (cons (cons 2 (quote ())) (quote ()))))) (newline) \
                 (display (equal? 1 1.0)) (newline)",
            );
            w.notes.push(out);
            let deep = eval_ok(
                "b8-e3-deep.ml",
                "(define (build-deep-list n) \
                   (if (= n 0) (quote ()) (cons n (build-deep-list (- n 1))))) \
                 (display (equal? (build-deep-list 5000) (build-deep-list 5000)))",
            );
            w.notes.push(deep);
        })
        .step(
            "structurally identical containers (including nested ones) compare equal, non-container values fall back to eqv? semantics (so an integer still never equals a float), and the large structure completes without hanging",
            |w, _text, _| {
                assert_eq!(w.notes[w.notes.len() - 2], "#t\n#t\n#t\n#f\n");
                assert_eq!(w.notes.last().unwrap(), "#t");
            },
        )
        // --- E4 ---
        .step(
            "false, the truthy whole number 0, and the truthy empty list",
            |_w, _text, _| {},
        )
        .step("not is applied to each", |w, _text, _| {
            let out = eval_ok(
                "b8-e4.ml",
                "(display (not #f)) (newline) \
                 (display (not 0)) (newline) \
                 (display (not (quote ()))) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "only false yields true; every other value, regardless of type, yields false",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#t\n#f\n#f\n");
            },
        )
        // --- E5 ---
        .step(
            "a matching and a non-matching value for each of: empty-list, pair, proper list, symbol, string, character, boolean, procedure, vector, hash table",
            |_w, _text, _| {},
        )
        .step("each predicate is applied to its matching and non-matching value", |w, _text, _| {
            let out = eval_ok(
                "b8-e5.ml",
                "(display (list? (cons 1 (cons 2 (cons 3 (quote ())))))) (newline) \
                 (display (list? (cons 1 2))) (newline) \
                 (display (null? (quote ()))) (newline) \
                 (display (pair? (quote ()))) (newline) \
                 (display (procedure? +)) (newline) \
                 (display (symbol? (quote a))) (newline) \
                 (display (symbol? 5)) (newline) \
                 (display (string? \"x\")) (newline) \
                 (display (string? 5)) (newline) \
                 (display (char? #\\a)) (newline) \
                 (display (char? 5)) (newline) \
                 (display (boolean? #t)) (newline) \
                 (display (boolean? 5)) (newline) \
                 (display (vector? #(1 2))) (newline) \
                 (display (vector? 5)) (newline) \
                 (display (hash? (make-hash))) (newline) \
                 (display (hash? 5)) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "every predicate returns #t on the matching value and #f on the non-matching one, including a proper list returning #f for an improper (dotted) structure",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "#t\n#f\n#t\n#f\n#t\n#t\n#f\n#t\n#f\n#t\n#f\n#t\n#f\n#t\n#f\n#t\n#f\n"
                );
            },
        )
        // --- E6 ---
        .step(
            "all twelve DEMO expressions from the behaviour spec run together in one program",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let out = eval_ok(
                "b8-e6.ml",
                "(display (eq? (quote a) (quote a))) (newline) \
                 (display (eqv? 1 1.0)) (newline) \
                 (display (eqv? 0.0 -0.0)) (newline) \
                 (display (equal? (cons 1 (cons 2 (quote ()))) (cons 1 (cons 2 (quote ()))))) (newline) \
                 (display (equal? \"ab\" \"ab\")) (newline) \
                 (display (not #f)) (newline) \
                 (display (not 0)) (newline) \
                 (display (list? (cons 1 (cons 2 (cons 3 (quote ())))))) (newline) \
                 (display (list? (cons 1 2))) (newline) \
                 (display (null? (quote ()))) (newline) \
                 (display (pair? (quote ()))) (newline) \
                 (display (procedure? +)) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "each line of output matches its prescribed value exactly, and the process exits 0",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "#t\n#f\n#f\n#t\n#t\n#t\n#f\n#t\n#f\n#t\n#f\n#t\n"
                );
            },
        )
}
