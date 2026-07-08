//! Step definitions for features/B11-vectors-and-hash-tables.feature.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::registry::Registry;
use super::world::{eval_ok, run, run_pending, stderr_of, write_source};

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a vector built from a sequence of values, a vector created with a given length and an explicit fill value, and positions inside and outside a vector's bounds",
            |_w, _text, _| {},
        )
        .step(
            "elements are read, replaced, and the length is measured",
            |w, _text, _| {
                let out = eval_ok(
                    "b11-e1.ml",
                    "(display (vector-ref (vector 1 2 3) 1)) \
                     (define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector-ref v 1)) \
                     (display (vector-length (vector 1 2 3))) \
                     (display (make-vector 3 7))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "each returns the correct value, the explicit-fill construction works, and both out-of-bounds reading and out-of-bounds writing are clean runtime errors",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "2993#(7 7 7)");
                let read_oob = write_source(
                    "b11-e1-read-oob.ml",
                    "(display (vector-ref (vector 1 2 3) 3))",
                );
                let output = run(&["eval", read_oob.to_str().unwrap()]);
                assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                assert!(!stderr_of(&output).is_empty());
                let write_oob =
                    write_source("b11-e1-write-oob.ml", "(vector-set! (vector 1 2 3) 3 99)");
                let output = run(&["eval", write_oob.to_str().unwrap()]);
                assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                assert!(!stderr_of(&output).is_empty());
            },
        )
        // --- E2 ---
        .step(
            "a mutated vector converted to a list, a list converted to a vector, an existing vector filled entirely with one value, and a round trip through both conversions",
            |w, _text, _| {
                w.pending = vec![
                    "(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector->list v))"
                        .to_string(),
                    "(display (list->vector (list 1 2)))".to_string(),
                    "(define v (vector 1 2 3)) (vector-fill! v 9) (display v)".to_string(),
                    "(display (vector->list (list->vector (list 1 2 3))))".to_string(),
                ];
            },
        )
        // Shared with E4 below -- both scenarios' Given queues its own
        // MagicLisp snippets into `world.pending` for this generically-
        // worded When to run independently via `run_pending`.
        .step("each operation is applied", |w, _text, _| {
            run_pending(w, "b11-each");
        })
        .step(
            "each produces the correct result, the fill operation changes every position, and the round trip reproduces the original list exactly",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(1 99 3)");
                assert_eq!(w.notes[1], "#(1 2)");
                assert_eq!(w.notes[2], "#(9 9 9)");
                assert_eq!(w.notes[3], "(1 2 3)");
            },
        )
        // --- E3 ---
        .step(
            "a vector literal written directly in source",
            |_w, _text, _| {},
        )
        .step(
            "it is displayed, checked with vector?, and indexed into",
            |w, _text, _| {
                let out = eval_ok(
                    "b11-e3.ml",
                    "(display #(1 2 3)) (display (vector? #(1 2 3))) \
                     (display (vector-ref #(1 2 3) 2))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "it displays correctly as a whole, is recognized as a genuine vector (not merely text), and its elements are individually accessible",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#(1 2 3)#t3");
            },
        )
        // --- E4 ---
        .step(
            "an empty hash table with entries stored, retrieved, and removed by key, a compound key built separately but structurally identical to a stored one, and a missing key looked up with and without a fallback",
            |w, _text, _| {
                w.pending = vec![
                    "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
                     (display (hash-count h))"
                        .to_string(),
                    "(display (hash-ref (make-hash) (quote c) \"nope\"))".to_string(),
                    "(define h (make-hash)) (hash-set! h (quote a) 1) \
                     (display (hash-has-key? h (quote a)))"
                        .to_string(),
                    "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-remove! h (quote a)) \
                     (display (hash-has-key? h (quote a)))"
                        .to_string(),
                    "(define h (make-hash)) (hash-set! h (list 1 2) 42) \
                     (display (hash-ref h (list 1 2)))"
                        .to_string(),
                ];
            },
        )
        .step(
            "count and presence are reported correctly, a structurally-identical but separately-built compound key still retrieves its value (equal?-based, not identity-based), a missing key with a fallback returns the fallback, and a missing key without one is a clean, distinct runtime error",
            |w, _text, _| {
                assert_eq!(w.notes[0], "2");
                assert_eq!(w.notes[1], "nope");
                assert_eq!(w.notes[2], "#t");
                assert_eq!(w.notes[3], "#f");
                assert_eq!(w.notes[4], "42");
                let missing = write_source(
                    "b11-e4-missing-no-fallback.ml",
                    "(hash-ref (make-hash) (quote c))",
                );
                let output = run(&["eval", missing.to_str().unwrap()]);
                assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                assert!(!stderr_of(&output).is_empty());
            },
        )
        // --- E5 ---
        .step(
            "a two-entry table and a table with three insertions, a removal, and a re-insertion of the removed key",
            |_w, _text, _| {},
        )
        .step("the key list is retrieved", |w, _text, _| {
            let out = eval_ok(
                "b11-e5.ml",
                "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
                 (display (hash-keys h)) \
                 (define h2 (make-hash)) \
                 (hash-set! h2 (quote a) 1) (hash-set! h2 (quote b) 2) (hash-set! h2 (quote c) 3) \
                 (hash-remove! h2 (quote a)) (hash-set! h2 (quote a) 99) \
                 (display (hash-keys h2))",
            );
            w.notes.push(out);
        })
        .step(
            "keys come back in first-insertion order, and a removed-then-re-inserted key lands at the end, not its original position",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "(a b)(b c a)");
            },
        )
        // --- E6 ---
        .step(
            "all twelve DEMO expressions/sequences from the behaviour spec run together in one program",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let out = eval_ok(
                "b11-e6.ml",
                "(define v (vector 1 2 3)) (display (vector-ref v 1)) (newline) \
                 (vector-set! v 1 99) (display (vector-ref v 1)) (newline) \
                 (display (vector-length v)) (newline) \
                 (display (vector->list v)) (newline) \
                 (display (make-vector 3 0)) (newline) \
                 (display (list->vector (cons 1 (cons 2 (quote ()))))) (newline) \
                 (display #(1 2 3)) (newline) \
                 (define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
                 (display (hash-count h)) (newline) \
                 (display (hash-keys h)) (newline) \
                 (display (hash-ref h (quote c) \"nope\")) (newline) \
                 (display (hash-has-key? h (quote a))) (newline) \
                 (hash-remove! h (quote a)) (display (hash-has-key? h (quote a))) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "each line of output matches its prescribed value exactly, and the process exits 0",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "2\n99\n3\n(1 99 3)\n#(0 0 0)\n#(1 2)\n#(1 2 3)\n2\n(a b)\nnope\n#t\n#f\n"
                );
            },
        )
}
