//! Step definitions for features/B17-repl.feature.

use super::registry::Registry;
use super::world::{run_with_stdin, stderr_of, stdout_of};
use magiclisp::exitcode::SUCCESS;

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step("two entries piped into the REPL", |w, _text, _| {
            w.notes.push("1\n2\n".to_string());
        })
        .step("the raw stdout bytes are inspected", |w, _text, _| {
            let stdin = w.notes.last().unwrap().clone();
            w.outputs.push(run_with_stdin(&["repl"], stdin.as_bytes()));
        })
        .step(
            "the prompt is exactly \">\" followed by a single space with no trailing newline of its own, appearing once before each entry plus once more right before the session closes",
            |w, _text, _| {
                let output = w.last_output();
                assert_eq!(stdout_of(output), "> 1\n> 2\n> ");
                assert_eq!(output.status.code(), Some(SUCCESS));
            },
        )
        // --- E2 ---
        .step(
            "an entry evaluating to a number, an entry evaluating to a string, and a define entry (unspecified value)",
            |w, _text, _| {
                w.pending = vec![
                    "(+ 1 2)".to_string(),
                    "\"hi\"".to_string(),
                    "(define x 10)".to_string(),
                ];
            },
        )
        .step("each is entered", |w, _text, _| {
            let pending = std::mem::take(&mut w.pending);
            for src in pending {
                w.outputs
                    .push(run_with_stdin(&["repl"], format!("{src}\n").as_bytes()));
            }
        })
        .step(
            "the number and string print in write form (the string quoted, not raw) followed by a newline, and the define entry produces no output between its surrounding prompts",
            |w, _text, _| {
                assert_eq!(stdout_of(&w.outputs[0]), "> 3\n> ");
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[1]), "> \"hi\"\n> ");
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[2]), "> > ");
                assert_eq!(w.outputs[2].status.code(), Some(SUCCESS));
            },
        )
        // --- E3 ---
        .step(
            "x defined, then referenced, then redefined, then referenced again",
            |w, _text, _| {
                w.notes
                    .push("(define x 10)\nx\n(define x 20)\nx\n".to_string());
            },
        )
        .step("each entry is evaluated in sequence", |w, _text, _| {
            let stdin = w.notes.last().unwrap().clone();
            w.outputs.push(run_with_stdin(&["repl"], stdin.as_bytes()));
        })
        .step(
            "the first reference sees the original value and the second reference sees the redefined value",
            |w, _text, _| {
                let output = w.last_output();
                assert_eq!(stdout_of(output), "> > 10\n> > 20\n> ");
                assert_eq!(output.status.code(), Some(SUCCESS));
            },
        )
        // --- E4 ---
        .step(
            "a definition, then an entry that misuses a built-in and errors, then a reference to the earlier definition",
            |w, _text, _| {
                w.notes.push("(define x 10)\n(car 5)\nx\n".to_string());
            },
        )
        .step("each is evaluated in sequence", |w, _text, _| {
            let stdin = w.notes.last().unwrap().clone();
            w.outputs.push(run_with_stdin(&["repl"], stdin.as_bytes()));
        })
        .step(
            "the failing entry produces no stdout output and exactly one \"Error: \"-prefixed stderr line, the session continues to the next prompt, and the earlier definition is still correctly bound afterward",
            |w, _text, _| {
                let output = w.last_output();
                assert_eq!(stdout_of(output), "> > > 10\n> ");
                assert_eq!(stderr_of(output).lines().count(), 1);
                assert_eq!(
                    stderr_of(output).trim_end(),
                    "Error: car expects a pair, found 5"
                );
                assert_eq!(output.status.code(), Some(SUCCESS));
            },
        )
        // --- E5 ---
        .step(
            "a handful of ordinary entries with no errors, followed by end-of-input",
            |w, _text, _| {
                w.notes.push("1\n2\n3\n".to_string());
            },
        )
        .step("the session runs to completion", |w, _text, _| {
            let stdin = w.notes.last().unwrap().clone();
            w.outputs.push(run_with_stdin(&["repl"], stdin.as_bytes()));
        })
        .step(
            "the process exits with code 0 and stderr is empty",
            |w, _text, _| {
                let output = w.last_output();
                assert_eq!(output.status.code(), Some(SUCCESS));
                assert_eq!(stderr_of(output), "");
            },
        )
        // --- E6 ---
        .step(
            "the same sequence of entries piped into `magiclisp` with no arguments and into `magiclisp repl`",
            |w, _text, _| {
                w.notes
                    .push("(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n".to_string());
            },
        )
        .step("both are run", |w, _text, _| {
            let stdin = w.notes.last().unwrap().clone();
            w.labeled
                .push(("bare".to_string(), run_with_stdin(&[], stdin.as_bytes())));
            w.labeled.push((
                "repl".to_string(),
                run_with_stdin(&["repl"], stdin.as_bytes()),
            ));
        })
        .step("their stdout and stderr are byte-identical", |w, _text, _| {
            let bare = w.labeled("bare");
            let repl = w.labeled("repl");
            assert_eq!(bare.stdout, repl.stdout);
            assert_eq!(bare.stderr, repl.stderr);
        })
        // --- E7 ---
        .step(
            "the DEMO sequence of five entries — (+ 1 2), (define x 10), x, (car 5), x — followed by end-of-input",
            |w, _text, _| {
                w.notes
                    .push("(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n".to_string());
            },
        )
        .step("the session runs", |w, _text, _| {
            let stdin = w.notes.last().unwrap().clone();
            w.outputs.push(run_with_stdin(&["repl"], stdin.as_bytes()));
        })
        .step(
            "stdout, stderr, and the exit code exactly match the prescribed transcript",
            |w, _text, _| {
                let output = w.last_output();
                assert_eq!(stdout_of(output), "> 3\n> > 10\n> > 10\n> ");
                assert_eq!(
                    stderr_of(output).trim_end(),
                    "Error: car expects a pair, found 5"
                );
                assert_eq!(output.status.code(), Some(SUCCESS));
            },
        )
}
