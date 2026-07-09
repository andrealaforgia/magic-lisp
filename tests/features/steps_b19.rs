//! Step definitions for features/B19-reader-edge-cases-and-conformance.feature.

use super::registry::Registry;
use super::world::{World, run, run_with_stdin, stderr_of, stdout_of, write_source};
use magiclisp::exitcode::{RUNTIME_ERROR, SOURCE_ERROR, SUCCESS};

/// Runs every queued full CLI invocation in `world.pending_commands`,
/// appending each real process `Output` to `world.outputs` -- the shared
/// implementation behind the "each is run" When step, reused verbatim by
/// E2/E3/E5/E7 (all four share this exact wording in the feature file).
fn run_pending_commands(world: &mut World) {
    let commands = std::mem::take(&mut world.pending_commands);
    for args in commands {
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        world.outputs.push(run(&arg_refs));
    }
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a source file with a leading line comment before a display call",
            |w, _text, _| {
                let file = write_source("b19-e1.ml", "; a leading comment\n(display 1)");
                w.pending_commands = vec![vec!["eval".to_string(), file.to_str().unwrap().to_string()]];
            },
        )
        .step("it is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "the comment is skipped and the display call runs normally",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 1);
                assert_eq!(stdout_of(&w.outputs[0]), "1");
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
            },
        )
        // --- E2 ---
        .step(
            "a single block comment preceding code, and a block comment containing another complete block comment nested inside it",
            |w, _text, _| {
                let demo2 = write_source(
                    "b19-e2-demo2.ml",
                    "#| a block comment |# (display 1) (newline)",
                );
                let demo3 = write_source(
                    "b19-e2-demo3.ml",
                    "#| outer #| nested |# still outer |# (display 2) (newline)",
                );
                w.pending_commands = vec![
                    vec!["eval".to_string(), demo2.to_str().unwrap().to_string()],
                    vec!["eval".to_string(), demo3.to_str().unwrap().to_string()],
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "both are skipped entirely and the following code runs normally — the inner nested comment does not end the outer one early",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 2);
                assert_eq!(stdout_of(&w.outputs[0]), "1\n");
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[1]), "2\n");
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
            },
        )
        // --- E3 ---
        .step(
            "a stray value between two operands of a sum, immediately preceded by the skip marker, and a skipped datum that is itself a whole compound list",
            |w, _text, _| {
                let demo1 = write_source("b19-e3-demo1.ml", "(display (+ 1 #;99 2)) (newline)");
                let compound = write_source(
                    "b19-e3-compound.ml",
                    "(display (+ 1 #;(a b c) 2)) (newline)",
                );
                w.pending_commands = vec![
                    vec!["eval".to_string(), demo1.to_str().unwrap().to_string()],
                    vec!["eval".to_string(), compound.to_str().unwrap().to_string()],
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "the marked datum is skipped entirely regardless of whether it's a single token or a multi-token compound structure",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 2);
                assert_eq!(stdout_of(&w.outputs[0]), "3\n");
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[1]), "3\n");
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
            },
        )
        // --- E4 ---
        .step(
            "a quoted dotted-pair literal and integer literals in hex, binary, and octal",
            |w, _text, _| {
                let dotted = write_source(
                    "b19-e4-dotted.ml",
                    "(display (quote (1 . 2))) (newline)",
                );
                let radix = write_source(
                    "b19-e4-radix.ml",
                    "(display #x1A) (newline) (display #b101) (newline) (display #o17) (newline)",
                );
                w.pending_commands = vec![
                    vec!["eval".to_string(), dotted.to_str().unwrap().to_string()],
                    vec!["eval".to_string(), radix.to_str().unwrap().to_string()],
                ];
            },
        )
        .step("each is displayed", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "the dotted pair shows its written structure and each radix reads to the correct decimal value",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 2);
                assert_eq!(stdout_of(&w.outputs[0]), "(1 . 2)\n");
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[1]), "26\n5\n15\n");
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
            },
        )
        // --- E5 ---
        .step(
            "an integer literal exceeding the range, and an addition that overflows the maximum representable integer",
            |w, _text, _| {
                let oversized = write_source(
                    "b19-e5-oversized.ml",
                    "(display 99999999999999999999999999999)",
                );
                let overflow = write_source(
                    "b19-e5-overflow.ml",
                    "(display (+ 9223372036854775807 1))",
                );
                w.pending_commands = vec![
                    vec!["eval".to_string(), oversized.to_str().unwrap().to_string()],
                    vec!["eval".to_string(), overflow.to_str().unwrap().to_string()],
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "the oversized literal is a read error and the overflow wraps rather than erroring or growing arbitrarily",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 2);
                assert_eq!(w.outputs[0].status.code(), Some(SOURCE_ERROR));
                assert!(!stderr_of(&w.outputs[0]).is_empty());
                assert_eq!(stdout_of(&w.outputs[1]), "-9223372036854775808");
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
            },
        )
        // --- E6 ---
        .step(
            "each of the ten SPEC.md 9.5 sample programs (factorial/redefinition, tail-call/loop, closures, shared captured variable, macros, quasiquotation, division, error signalling, reading input, hash tables)",
            |_w, _text, _| {},
        )
        .step("each is run through the real compile-then-run pipeline", |w, _text, _| {
            let samples: [(&str, &str, Option<&str>); 10] = [
                (
                    "010",
                    "(define (fact n) (error \"should have been redefined\"))\n\
                     (define (fact n) (if (< n 2) 1 (* n (fact (- n 1)))))\n\
                     (display (fact 10)) (newline)",
                    None,
                ),
                (
                    "020",
                    "(define (loop i) (if (= i 10000000) i (loop (+ i 1)))) (display (loop 0)) (newline)",
                    None,
                ),
                (
                    "030",
                    "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n)))\n\
                     (define a (counter)) (define b (counter))\n\
                     (display (a)) (newline) (display (a)) (newline) (display (b)) (newline)",
                    None,
                ),
                (
                    "040",
                    "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v)))))\n\
                     (define p (pairf))\n\
                     ((cdr p) 10)\n\
                     (display ((car p))) (newline)",
                    None,
                ),
                (
                    "050",
                    "(define-macro (swap! a b) (let ((tmp (gensym))) `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp))))\n\
                     (define x 1) (define y 2) (swap! x y) (write (list x y)) (newline)",
                    None,
                ),
                (
                    "060",
                    "(define mid (quote (2 3 4))) (write `(1 ,@mid 5)) (newline)",
                    None,
                ),
                (
                    "070",
                    "(display (/ 6 3)) (newline)\n\
                     (display (/ 7 2)) (newline)\n\
                     (display (/ 6 3.0)) (newline)\n\
                     (display 1.0) (newline)",
                    None,
                ),
                ("080", "(error \"boom\" 42)", None),
                (
                    "090",
                    "(define d (read)) (write d) (newline) (display (+ 1 2)) (newline)",
                    Some("(+ 1 2)\n"),
                ),
                (
                    "100",
                    "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2)\n\
                     (display (hash-count h)) (newline)\n\
                     (write (hash-keys h)) (newline)\n\
                     (display (hash-ref h (quote c) \"nope\")) (newline)\n\
                     (display (hash-has-key? h (quote a))) (newline)\n\
                     (hash-remove! h (quote a)) (display (hash-has-key? h (quote a))) (newline)",
                    None,
                ),
            ];
            for (tag, src, stdin) in samples {
                let file = write_source(&format!("b19-e6-{tag}.ml"), src);
                let path = file.to_str().unwrap();
                let output = match stdin {
                    Some(data) => run_with_stdin(&["eval", path], data.as_bytes()),
                    None => run(&["eval", path]),
                };
                w.outputs.push(output);
            }
        })
        .step(
            "each produces exactly its officially specified stdout, exit code, and (for the error sample) error-stream prefix",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 10);
                assert_eq!(stdout_of(&w.outputs[0]), "3628800\n");
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[1]), "10000000\n");
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[2]), "1\n2\n1\n");
                assert_eq!(w.outputs[2].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[3]), "10\n");
                assert_eq!(w.outputs[3].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[4]), "(2 1)\n");
                assert_eq!(w.outputs[4].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[5]), "(1 2 3 4 5)\n");
                assert_eq!(w.outputs[5].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[6]), "2\n3.5\n2.0\n1.0\n");
                assert_eq!(w.outputs[6].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[7]), "");
                assert_eq!(w.outputs[7].status.code(), Some(RUNTIME_ERROR));
                assert_eq!(stderr_of(&w.outputs[7]).lines().next().unwrap(), "Error: boom 42");

                assert_eq!(stdout_of(&w.outputs[8]), "(+ 1 2)\n3\n");
                assert_eq!(w.outputs[8].status.code(), Some(SUCCESS));

                assert_eq!(stdout_of(&w.outputs[9]), "2\n(a b)\nnope\n#t\n#f\n");
                assert_eq!(w.outputs[9].status.code(), Some(SUCCESS));
            },
        )
        // --- E7 ---
        .step("the four DEMOs from the behaviour spec", |w, _text, _| {
            let demo1 = write_source("b19-e7-demo1.ml", "(display (+ 1 #;99 2)) (newline)");
            let demo2 = write_source(
                "b19-e7-demo2.ml",
                "#| a block comment |# (display 1) (newline)",
            );
            let demo3 = write_source(
                "b19-e7-demo3.ml",
                "#| outer #| nested |# still outer |# (display 2) (newline)",
            );
            let demo4 = write_source("b19-e7-demo4.ml", "(display (quote (1 . 2))) (newline)");
            w.pending_commands = vec![
                vec!["eval".to_string(), demo1.to_str().unwrap().to_string()],
                vec!["eval".to_string(), demo2.to_str().unwrap().to_string()],
                vec!["eval".to_string(), demo3.to_str().unwrap().to_string()],
                vec!["eval".to_string(), demo4.to_str().unwrap().to_string()],
            ];
        })
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "each produces exactly its prescribed output with a trailing newline and exit code 0",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 4);
                let expected = ["3\n", "1\n", "2\n", "(1 . 2)\n"];
                for (output, exp) in w.outputs.iter().zip(expected) {
                    assert_eq!(stdout_of(output), exp);
                    assert_eq!(output.status.code(), Some(SUCCESS));
                }
            },
        )
}
