//! Step definitions for features/B15-errors-and-exit.feature.

use super::registry::Registry;
use super::world::{run, stderr_of, stdout_of, write_source, World};
use magiclisp::exitcode::{RUNTIME_ERROR, SOURCE_ERROR, SUCCESS};

/// Runs every pending MagicLisp source snippet queued in `world.pending`
/// as a real, complete program, appending each real process `Output` to
/// `world.outputs` -- the shared implementation behind the "each is run"
/// When step, reused verbatim by E1/E2/E4/E6 (all four share this exact
/// wording in the feature file) with different Givens queuing different
/// snippets. Distinct from `world::run_pending`: that helper is built on
/// `eval_ok`, which panics on any non-success exit and only captures
/// stdout as a bare string -- both fatal here, since every one of this
/// feature's scenarios is specifically about non-zero exit codes and
/// stderr content (not to mention needing no `display`-wrapping, since
/// every snippet here is already a complete program).
fn run_pending_as_programs(world: &mut World, label_prefix: &str) {
    let pending = std::mem::take(&mut world.pending);
    for (i, src) in pending.iter().enumerate() {
        let file = write_source(&format!("{label_prefix}-{i}"), src);
        world.outputs.push(run(&["eval", file.to_str().unwrap()]));
    }
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "an error raised with a message and one integer irritant, and an error raised with a message and mixed irritants including a string",
            |w, _text, _| {
                w.pending = vec![
                    "(error \"boom\" 42)".to_string(),
                    "(error \"bad value\" 1 \"two\" (quote three))".to_string(),
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_as_programs(w, "b15-run");
        })
        .step(
            "the message appears in human-readable form, each irritant appears in machine-readable form (a string irritant appears quoted, distinguishing it from a bare number/symbol), space-separated, on stderr with no stdout",
            |w, _text, _| {
                assert_eq!(w.outputs[0].status.code(), Some(RUNTIME_ERROR));
                assert_eq!(stdout_of(&w.outputs[0]), "");
                assert_eq!(
                    stderr_of(&w.outputs[0]).lines().next().unwrap(),
                    "Error: boom 42"
                );
                assert_eq!(w.outputs[1].status.code(), Some(RUNTIME_ERROR));
                assert_eq!(
                    stderr_of(&w.outputs[1]).lines().next().unwrap(),
                    "Error: bad value 1 \"two\" three"
                );
            },
        )
        // --- E2 ---
        .step(
            "a program that produces output, then misuses a built-in, then would produce more output, plus four more built-in-misuse categories (division by exact zero, wrong argument count, undefined name, wrong-type operand)",
            |w, _text, _| {
                w.pending = vec![
                    "(display \"before\") (newline) (display (car 5)) (display \"after\")"
                        .to_string(),
                    "(display (/ 1 0))".to_string(),
                    "(define (f a b) (+ a b)) (display (f 1))".to_string(),
                    "(display this-name-does-not-exist)".to_string(),
                    "(display (+ 1 \"a\"))".to_string(),
                ];
            },
        )
        .step(
            "only the output before the failure point appears, each produces exactly one \"Error: \"-prefixed stderr line, and all five cases (plus the deliberate-raise case from E1) share the IDENTICAL exit code",
            |w, _text, _| {
                assert_eq!(w.outputs[0].status.code(), Some(RUNTIME_ERROR));
                assert_eq!(stdout_of(&w.outputs[0]), "before\n");
                for output in &w.outputs {
                    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                    assert!(stderr_of(output).starts_with("Error: "));
                }
            },
        )
        // --- E3 ---
        .step(
            "a source file with an unterminated list (a read error, before the program starts running)",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let file = write_source("b15-e3", "(display (+ 1");
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step(
            "free-form error text is reported on stderr and the process exits with a code distinct from E2's runtime-error code",
            |w, _text, _| {
                let output = w.last_output();
                assert_eq!(output.status.code(), Some(SOURCE_ERROR));
                assert_ne!(SOURCE_ERROR, RUNTIME_ERROR);
                assert!(!stderr_of(output).is_empty());
            },
        )
        // --- E4 ---
        .step(
            "a program that exits with a specific code, one that exits with no code, and one that exits then attempts further output",
            |w, _text, _| {
                w.pending = vec![
                    "(exit 3)".to_string(),
                    "(exit)".to_string(),
                    "(exit 0) (display \"should never appear\")".to_string(),
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_as_programs(w, "b15-run");
        })
        .step(
            "the specific code is used, no code means success, and nothing after the exit call executes",
            |w, _text, _| {
                assert_eq!(w.outputs[0].status.code(), Some(3));
                assert!(stderr_of(&w.outputs[0]).is_empty());
                assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
                assert!(stderr_of(&w.outputs[1]).is_empty());
                assert_eq!(w.outputs[2].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[2]), "");
            },
        )
        // --- E5 ---
        .step(
            "the same zero divisor applied to a float dividend and to an integer dividend",
            |_w, _text, _| {},
        )
        .step("each division is displayed", |w, _text, _| {
            let float_file = write_source("b15-e5-float", "(display (/ 1.0 0.0))");
            w.outputs.push(run(&["eval", float_file.to_str().unwrap()]));
            let int_file = write_source("b15-e5-int", "(display (/ 1 0))");
            w.outputs.push(run(&["eval", int_file.to_str().unwrap()]));
        })
        .step(
            "the float case succeeds with a recognizable infinity value and exit 0, while the integer case fails with the runtime-error exit code",
            |w, _text, _| {
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[0]), "+inf.0");
                assert_eq!(w.outputs[1].status.code(), Some(RUNTIME_ERROR));
            },
        )
        // --- E6 ---
        .step(
            "each of the four DEMO scenarios from the behaviour spec",
            |w, _text, _| {
                w.pending = vec![
                    "(error \"boom\" 42)".to_string(),
                    "(display \"before\") (newline) (display (car 5)) (display \"after\")"
                        .to_string(),
                    "(exit 3)".to_string(),
                    "(display (/ 1.0 0.0)) (newline)".to_string(),
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_as_programs(w, "b15-run");
        })
        .step(
            "each produces exactly its prescribed stdout/stderr content and exit code",
            |w, _text, _| {
                assert_eq!(w.outputs[0].status.code(), Some(RUNTIME_ERROR));
                assert_eq!(stdout_of(&w.outputs[0]), "");
                assert_eq!(
                    stderr_of(&w.outputs[0]).lines().next().unwrap(),
                    "Error: boom 42"
                );

                assert_eq!(w.outputs[1].status.code(), Some(RUNTIME_ERROR));
                assert_eq!(stdout_of(&w.outputs[1]), "before\n");
                assert!(stderr_of(&w.outputs[1]).starts_with("Error: "));

                assert_eq!(w.outputs[2].status.code(), Some(3));
                assert!(stderr_of(&w.outputs[2]).is_empty());

                assert_eq!(w.outputs[3].status.code(), Some(SUCCESS));
                assert_eq!(stdout_of(&w.outputs[3]), "+inf.0\n");
            },
        )
}
