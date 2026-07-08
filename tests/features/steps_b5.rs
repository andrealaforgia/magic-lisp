//! Step definitions for features/B5-closures.feature.

use super::registry::Registry;
use super::world::eval_ok;

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a factory function that returns a closure capturing one of its parameters",
            |_w, _text, _| {},
        )
        .step(
            "the factory call fully returns, and the returned closure is called afterward",
            |w, _text, _| {
                let out = eval_ok(
                    "b5-e1.ml",
                    "(define (make-adder n) (lambda (x) (+ x n))) (display ((make-adder 3) 4))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "it correctly uses the value captured at creation time, not a default or garbage value",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "7");
            },
        )
        // --- E2 ---
        .step(
            "a factory returning a getter closure and a setter closure over one shared local",
            |_w, _text, _| {},
        )
        .step(
            "the setter is called with a value and then the getter is called",
            |w, _text, _| {
                let out = eval_ok(
                    "b5-e2.ml",
                    "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v))))) \
                     (define p (pairf)) \
                     ((cdr p) 10) \
                     (display ((car p)))",
                );
                w.notes.push(out);
            },
        )
        .step("the getter observes the value the setter wrote", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "10");
        })
        // --- E3 ---
        .step(
            "two independent closures created from two separate calls to the same counter-factory function",
            |_w, _text, _| {},
        )
        .step(
            "calls to the two closures are interleaved (first counter twice, then second counter once)",
            |w, _text, _| {
                let out = eval_ok(
                    "b5-e3.ml",
                    "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
                     (define a (counter)) (define b (counter)) \
                     (display (a)) (newline) (display (a)) (newline) (display (b)) (newline)",
                );
                w.notes.push(out);
            },
        )
        .step("each counter's state is independent of the other's", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "1\n2\n1\n");
        })
        // --- E4 ---
        .step("a pair constructed from two distinguishable values", |w, _text, _| {
            w.notes.push(eval_ok("b5-e4.ml", "(display (cons \"a\" \"b\"))"));
        })
        .step("each half is retrieved", |w, _text, _| {
            let out = eval_ok(
                "b5-e4b.ml",
                "(display (car (cons 1 2))) (display (cdr (cons 1 2)))",
            );
            w.notes.push(out);
        })
        .step(
            "each half matches its original position, not swapped or merged",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(a . b)");
                assert_eq!(w.notes[1], "12");
            },
        )
        // --- E5 ---
        .step(
            "the counter-factory DEMO and the pair-factory DEMO from the behaviour spec",
            |w, _text, _| {
                w.pending = vec![
                    "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
                     (define a (counter)) (define b (counter)) \
                     (display (a)) (newline) (display (a)) (newline) (display (b)) (newline)"
                        .to_string(),
                    "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v))))) \
                     (define p (pairf)) \
                     ((cdr p) 10) \
                     (display ((car p))) (newline)"
                        .to_string(),
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            super::world::run_pending(w, "b5-e5");
        })
        .step(
            "each produces exactly its prescribed output followed by a trailing newline, and exits 0",
            |w, _text, _| {
                assert_eq!(w.notes[0], "1\n2\n1\n");
                assert_eq!(w.notes[1], "10\n");
            },
        )
}
