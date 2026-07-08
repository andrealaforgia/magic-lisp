//! Step definitions for features/B3-local-bindings-mutation-conditionals.feature.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::registry::Registry;
use super::world::{eval_ok, first_quoted, run, run_pending, stderr_of, write_source};

/// Shared behind every "\"<code>\" is evaluated"-style step: several
/// scenarios embed the literal snippet directly in this one combined
/// step (rather than a separate Given), each with different code, so one
/// function that self-extracts the quote from whatever text matched
/// covers all of them without a dedicated closure per scenario.
fn eval_quoted_and_store(w: &mut super::world::World, text: &str, _doc: Option<&str>) {
    let code = first_quoted(text).expect("step should embed a quoted expression");
    w.notes.push(eval_ok("b3-quoted.ml", &code));
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- shared generic verbs: several scenarios reuse the exact same
        // bare wording for their When step, with a different Given
        // queueing up what it should run.
        .step("it is evaluated", |w, _text, _| run_pending(w, "b3-it-eval"))
        .step("each is evaluated", |w, _text, _| {
            run_pending(w, "b3-each-eval")
        })
        .step("the function is called", |w, _text, _| {
            run_pending(w, "b3-fn-called")
        })
        .step("each is run", |w, _text, _| run_pending(w, "b3-each-run"))
        // --- E1 ---
        .step(
            "an outer x bound to 1, and a let group binding a sibling x to 2 and y to the outer x",
            |_w, _text, _| {},
        )
        .step(
            "\"(define x 1) (let ((x 2) (y x)) (display y))\" is evaluated",
            eval_quoted_and_store,
        )
        .step(
            "it displays \"1\" — y resolves to the outer x, not the sibling binding",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "1");
            },
        )
        // --- E2 ---
        .step(
            "a let* group where a later binding's expression references an earlier one",
            |_w, _text, _| {},
        )
        .step(
            "\"(let* ((x 1) (y (+ x 1))) (display y))\" is evaluated",
            eval_quoted_and_store,
        )
        .step("it displays \"2\"", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "2");
        })
        // --- E3 ---
        .step("a letrec-bound self-referencing recursive function", |w, _text, _| {
            w.pending = vec![
                "(display (letrec ((fact (lambda (n) (if (< n 2) 1 (* n (fact (- n 1))))))) (fact 5)))"
                    .to_string(),
            ];
        })
        .step(
            "it is called with an argument requiring multiple recursive steps",
            |w, _text, _| run_pending(w, "b3-e3"),
        )
        .step(
            "it terminates and returns the mathematically correct result",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "120");
            },
        )
        // --- E4 ---
        .step("a named-let loop summing from 1 to 100", |w, _text, _| {
            w.pending = vec![
                "(display (let loop ((i 1) (sum 0)) (if (> i 100) sum (loop (+ i 1) (+ sum i)))))"
                    .to_string(),
            ];
        })
        .step("it displays \"5050\"", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "5050");
        })
        // --- E5 ---
        .step(
            "a function body starting with definitions that reference each other out of declaration order",
            |w, _text, _| {
                w.pending = vec![
                    "(define (f) \
                       (define (double x) (* x 2)) \
                       (define (six-times x) (double (triple x))) \
                       (define (triple x) (* x 3)) \
                       (six-times 5)) \
                     (display (f))"
                        .to_string(),
                ];
            },
        )
        .step(
            "it resolves correctly as if all definitions were mutually visible from the start",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "30");
            },
        )
        // --- E6 ---
        .step(
            "a bound variable, and separately a name that was never defined",
            |_w, _text, _| {},
        )
        .step(
            "the bound variable is mutated with set! and then displayed",
            |w, _text, _| {
                let out = eval_ok("b3-e6a.ml", "(define v 0) (set! v 1) (display v)");
                w.notes.push(out);
            },
        )
        .step("it shows the new value", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "1");
        })
        .step("set! is applied to the undefined name", |w, _text, _| {
            let file = write_source("b3-e6b.ml", "(set! never-defined 1)");
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step(
            "the process fails with a distinct, non-zero exit code separate from success/usage/read-compile/file-format errors",
            |w, _text, _| {
                assert_eq!(w.last_output().status.code(), Some(RUNTIME_ERROR));
                assert!(!stderr_of(w.last_output()).is_empty());
            },
        )
        // --- E7 ---
        .step("a cond with several falsy tests and a trailing else", |w, _text, _| {
            w.pending = vec!["(display (cond (#f 1) (#f 2) (else 3)))".to_string()];
        })
        .step("the else branch's value is returned", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "3");
        })
        .step(
            "a cond clause using the \"=>\" variant with a truthy test value",
            |w, _text, _| {
                w.pending =
                    vec!["(display (cond (5 => (lambda (x) (* x 2)))))".to_string()];
            },
        )
        .step("the function is applied to the test's own value", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "10");
        })
        // --- E8 ---
        .step(
            "a case expression with a key matching one candidate group, and separately a key matching none",
            |w, _text, _| {
                w.pending = vec![
                    "(display (case 2 ((1 2 3) \"hi\") (else \"bye\")))".to_string(),
                    "(display (case 99 ((1 2 3) \"hi\") (else \"bye\")))".to_string(),
                ];
            },
        )
        .step(
            "the matching group's body runs, and the non-matching key falls through to else",
            |w, _text, _| {
                assert_eq!(w.notes[0], "hi");
                assert_eq!(w.notes[1], "bye");
            },
        )
        // --- E9 ---
        .step("an and expression where every argument is truthy", |w, _text, _| {
            w.pending = vec!["(display (and 1 2 3))".to_string()];
        })
        .step("it returns the last value", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "3");
        })
        .step(
            "an and expression where an early argument is falsy and a later argument has a side effect",
            |w, _text, _| {
                w.pending = vec![
                    "(define fired #f) (and #f (begin (set! fired #t) 1)) (display fired)"
                        .to_string(),
                ];
            },
        )
        .step(
            "the falsy value is returned and the later side effect never runs",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#f");
            },
        )
        // --- E10 ---
        .step(
            "an or expression where an early argument is truthy and a later argument has a side effect",
            |w, _text, _| {
                w.pending = vec![
                    "(define fired2 #f) (or 1 (begin (set! fired2 #t) 2)) (display fired2)"
                        .to_string(),
                ];
            },
        )
        .step(
            "the truthy value is returned and the later side effect never runs",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#f");
            },
        )
        .step("an or expression where every argument is falsy", |w, _text, _| {
            w.pending = vec!["(display (or #f #f #f))".to_string()];
        })
        .step(
            "it returns the last (falsy) value, proving the \"no truthy value found\" branch actually runs",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#f");
            },
        )
        // --- E11 ---
        .step(
            "all four combinations of when/unless with a truthy or falsy condition",
            |w, _text, _| {
                w.pending = vec![
                    "(display (when #t 1))".to_string(),
                    "(define ran #f) (when #f (set! ran #t)) (display ran)".to_string(),
                    "(display (unless #f 1))".to_string(),
                    "(define ran2 #f) (unless #t (set! ran2 #t)) (display ran2)".to_string(),
                ];
            },
        )
        .step(
            "when-true runs its body, when-false doesn't, unless-false runs its body, unless-true doesn't",
            |w, _text, _| {
                assert_eq!(w.notes[0], "1");
                assert_eq!(w.notes[1], "#f");
                assert_eq!(w.notes[2], "1");
                assert_eq!(w.notes[3], "#f");
            },
        )
        // --- E12 ---
        .step("each of the eight DEMO programs from the behaviour spec", |w, _text, _| {
            w.pending = vec![
                "(display (let loop ((i 1) (sum 0)) (if (> i 100) sum (loop (+ i 1) (+ sum i))))) (newline)".to_string(),
                "(display (let* ((x 2) (y (* x 3))) y)) (newline)".to_string(),
                "(display (cond (5 => (lambda (x) (* x 2))))) (newline)".to_string(),
                "(display (case 2 ((1 2 3) \"hi\") (else \"bye\"))) (newline)".to_string(),
                "(display (and 1 2 3)) (newline)".to_string(),
                "(display (or #f 'x 'y)) (newline)".to_string(),
                "(define v 0) (set! v 1) (display v) (newline)".to_string(),
                "(define (f) (define (double x) (* x 2)) (define (six-times x) (double (triple x))) \
                 (define (triple x) (* x 3)) (six-times 5)) (display (f)) (newline)"
                    .to_string(),
            ];
        })
        .step(
            "each produces exactly its prescribed output followed by a trailing newline, and exits 0",
            |w, _text, _| {
                assert_eq!(w.notes[0], "5050\n");
                assert_eq!(w.notes[1], "6\n");
                assert_eq!(w.notes[2], "10\n");
                assert_eq!(w.notes[3], "hi\n");
                assert_eq!(w.notes[4], "3\n");
                assert_eq!(w.notes[5], "x\n");
                assert_eq!(w.notes[6], "1\n");
                assert_eq!(w.notes[7], "30\n");
            },
        )
        // --- E13 ---
        .step(
            "a function body containing two independent, non-nested let blocks one after another",
            |w, _text, _| {
                w.pending = vec![
                    "(define (f) (let ((a 1)) (display a)) (newline) (let ((b 2)) (display b))) (f)"
                        .to_string(),
                ];
            },
        )
        .step(
            "each let's binding is evaluated correctly, with neither overwritten by or aliased to the other's runtime slot",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "1\n2");
            },
        )
        // --- E14 ---
        .step(
            "a let binding x to 1, containing a nested let that rebinds x to 2",
            |w, _text, _| {
                w.pending = vec![
                    "(display (let ((x 1)) (let ((x 2)) (display x)) (newline) x))".to_string(),
                ];
            },
        )
        .step(
            "the inner scope sees 2 while it is active, and the outer scope sees its own 1 again afterward",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "2\n1");
            },
        )
        // --- E15 ---
        .step(
            "a let binding x to 1, containing a nested let that mutates x with set!",
            |w, _text, _| {
                w.pending = vec![
                    "(display (let ((x 1)) (let ((y 2)) (set! x 99)) x))".to_string(),
                ];
            },
        )
        .step(
            "the outer x reflects the mutation once the inner scope closes",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "99");
            },
        )
        // --- E16 ---
        .step(
            "a letrec group where one binding's own initializer reads another binding that has not run yet",
            |_w, _text, _| {},
        )
        .step("the letrec expression is evaluated", |w, _text, _| {
            let file = write_source("b3-e16.ml", "(display (letrec ((a b) (b 1)) a))");
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        // --- E17 ---
        .step(
            "a let binding x, containing a lambda with no parameters of its own that references x",
            |w, _text, _| {
                w.pending = vec!["(display (let ((x 5)) ((lambda () x))))".to_string()];
            },
        )
        .step("it correctly resolves x from the enclosing scope", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "5");
        })
}
