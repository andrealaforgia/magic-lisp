//! Interactive read-eval-print loop (B17, spec 9.1).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::mpsc;

use crate::value::{Value, write_repr};
use crate::{compiler, exitcode, reader, vm};

/// The exact prompt text (spec 9.1): a greater-than sign and one space,
/// with no trailing newline -- the whole point is that the next thing
/// written to `out` (either the user's own echoed input on a real
/// terminal, or an entry's result) appears to continue the same line.
const PROMPT: &str = "> ";

/// A request the session thread (see `run_session`) sends back to the
/// thread that owns the caller's actual `input`/`out`/`err` (see `run`).
/// Neither payload variant carries anything `Rc`-based -- `Vec<u8>` and,
/// via `line_rx`, `Option<String>` -- so this crosses the thread boundary
/// with no `Send` concerns at all, unlike `globals`/`ReplState`, which
/// never leave the session thread for the whole session's lifetime.
enum Msg {
    Write(Vec<u8>),
    WriteErr(Vec<u8>),
    ReadLine,
}

/// Forwards every byte a REPL entry displays (`display`/`newline`, and
/// this module's own auto-printed results) to the relay loop in `run`
/// rather than writing directly -- see `run_session`'s own doc comment for
/// why the whole session runs on a separate, dedicated-stack thread that
/// can't touch the caller's own (possibly non-`Send`) `out` directly.
struct ChannelOut<'a> {
    tx: &'a mpsc::Sender<Msg>,
}

impl Write for ChannelOut<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = self.tx.send(Msg::Write(buf.to_vec()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Runs the interactive session. `input`/`out`/`err` never leave this
/// (calling) thread -- see `run_session`'s doc comment for why the actual
/// read-eval-print loop instead runs on a separate, dedicated big-stack
/// thread that only ever talks back here over a channel.
pub fn run(mut input: impl BufRead, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let (msg_tx, msg_rx) = mpsc::channel::<Msg>();
    let (line_tx, line_rx) = mpsc::channel::<Option<String>>();

    std::thread::scope(|scope| {
        let handle = match std::thread::Builder::new()
            .stack_size(vm::VM_STACK_SIZE)
            .spawn_scoped(scope, move || run_session(msg_tx, line_rx))
        {
            Ok(handle) => handle,
            Err(e) => {
                let _ = writeln!(err, "Error: failed to start the REPL session: {e}");
                return exitcode::RUNTIME_ERROR;
            }
        };

        while let Ok(msg) = msg_rx.recv() {
            match msg {
                Msg::Write(bytes) => {
                    let _ = out.write_all(&bytes);
                    let _ = out.flush();
                }
                Msg::WriteErr(bytes) => {
                    let _ = err.write_all(&bytes);
                    let _ = err.flush();
                }
                Msg::ReadLine => {
                    let mut line = String::new();
                    // `Ok(0)` is genuine end-of-input; a read error is
                    // treated the same way (end the session) rather than
                    // looping forever on a stream that can't make progress.
                    let reply = match input.read_line(&mut line) {
                        Ok(0) | Err(_) => None,
                        Ok(_) => Some(line),
                    };
                    if line_tx.send(reply).is_err() {
                        break;
                    }
                }
            }
        }
        handle.join().unwrap_or(exitcode::RUNTIME_ERROR)
    })
}

/// The actual read-eval-print loop, run on its own dedicated
/// `VM_STACK_SIZE` thread for the whole session (spawned once by `run`
/// above, not once per entry): warden security review msg #327 (High)
/// found that without a dedicated stack, ordinary non-tail recursion well
/// within `MAX_CALL_DEPTH`'s own documented-safe depth crashed the entire
/// process via native stack overflow, since every earlier verb
/// (`run`/`run_with_stdin`) already gets this same dedicated stack but the
/// REPL previously didn't. A dedicated thread per ENTRY was rejected: it
/// would require moving `globals`/`ReplState` (both containing `Rc`-based
/// values once a session has any `define`d pair/string/closure) across a
/// thread boundary every entry, which isn't `Send`-safe. Instead, both
/// live here for the session's entire lifetime and never cross a thread
/// boundary at all; only plain, `Send`-safe request/response messages
/// (`Msg`, `Option<String>`) cross back to `run`'s own thread, which alone
/// touches the caller's actual `input`/`out`/`err`.
fn run_session(msg_tx: mpsc::Sender<Msg>, line_rx: mpsc::Receiver<Option<String>>) -> i32 {
    let mut globals = vm::default_globals();
    let mut state = compiler::ReplState::new();
    loop {
        if msg_tx.send(Msg::Write(PROMPT.as_bytes().to_vec())).is_err() {
            break;
        }
        if msg_tx.send(Msg::ReadLine).is_err() {
            break;
        }
        let Ok(Some(line)) = line_rx.recv() else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut out = ChannelOut { tx: &msg_tx };
        let (result, updated_globals, updated_state) =
            eval_entry(trimmed, globals, state, &mut out);
        globals = updated_globals;
        state = updated_state;
        match result {
            Ok(value) => {
                // The unspecified value (e.g. a `define`'s own result)
                // prints nothing at all, not even a blank line.
                if !matches!(value, Value::Unspecified) {
                    let _ = msg_tx.send(Msg::Write(
                        format!("{}\n", write_repr(&value)).into_bytes(),
                    ));
                }
            }
            Err(message) => {
                let _ = msg_tx.send(Msg::WriteErr(format!("Error: {message}\n").into_bytes()));
            }
        }
    }
    exitcode::SUCCESS
}

/// Reads, compiles, and evaluates one entry, threading `globals` and
/// `state` in and returning both back out regardless of outcome -- see
/// `vm::eval_repl_entry`'s own doc comment for why `globals` must, and
/// `compiler::ReplState`'s own doc comment for why `state` must too. A
/// read or compile error is reported the same way as a runtime one (both
/// end up as one "Error: ..." line and a return to the prompt): B17 draws
/// no observable distinction between them, unlike the non-REPL CLI paths
/// (B1/B15), which is specific to those verbs running a whole program
/// once, not accumulating state entry by entry.
fn eval_entry(
    src: &str,
    globals: HashMap<String, Value>,
    state: compiler::ReplState,
    out: &mut impl Write,
) -> (Result<Value, String>, HashMap<String, Value>, compiler::ReplState) {
    let forms = match reader::read_program(src) {
        Ok(forms) => forms,
        Err(e) => return (Err(e.to_string()), globals, state),
    };
    let (state, compiled) = compiler::compile_repl_entry(state, &forms);
    let fn_index = match compiled {
        Ok(index) => index,
        Err(e) => return (Err(e.to_string()), globals, state),
    };
    let (result, updated_globals) = vm::eval_repl_entry(state.module(), fn_index, globals, out);
    (result.map_err(|e| e.message), updated_globals, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn run_session(src: &str) -> (String, String, i32) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(Cursor::new(src.as_bytes()), &mut out, &mut err);
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
            code,
        )
    }

    #[test]
    fn prints_the_exact_prompt_before_each_entry_and_once_more_before_eof() {
        let (out, _, _) = run_session("1\n2\n");
        // Two entries -> three prompts total (one per entry, plus the
        // final one right before EOF ends the session).
        assert_eq!(out.matches("> ").count(), 3, "{out:?}");
        assert!(out.ends_with("> "), "{out:?}");
    }

    #[test]
    fn an_ordinary_result_is_auto_printed_in_write_form_with_a_trailing_newline() {
        let (out, _, _) = run_session("(+ 1 2)\n");
        assert_eq!(out, "> 3\n> ");
    }

    #[test]
    fn a_string_result_is_auto_printed_quoted_write_style_not_raw() {
        let (out, _, _) = run_session("\"hi\"\n");
        assert_eq!(out, "> \"hi\"\n> ");
    }

    #[test]
    fn the_unspecified_result_of_a_define_prints_nothing_for_that_entry() {
        let (out, _, _) = run_session("(define x 10)\n");
        assert_eq!(out, "> > ");
    }

    #[test]
    fn a_definition_persists_and_a_later_redefinition_wins() {
        let (out, err, code) = run_session("(define x 10)\nx\n(define x 20)\nx\n");
        assert_eq!(out, "> > 10\n> > 20\n> ");
        assert!(err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn a_runtime_error_reports_one_error_line_and_returns_to_the_prompt_with_bindings_intact() {
        let (out, err, code) = run_session("(define x 10)\n(car 5)\nx\n");
        assert_eq!(out, "> > > 10\n> ");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        assert!(err.lines().next().unwrap().starts_with("Error: "), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn end_of_input_exits_with_success_even_with_no_entries_at_all() {
        let (out, err, code) = run_session("");
        assert_eq!(out, "> ");
        assert!(err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn the_full_demo_sequence_produces_exactly_the_prescribed_transcript() {
        let (out, err, code) =
            run_session("(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n");
        assert_eq!(out, "> 3\n> > 10\n> > 10\n> ");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        assert!(err.lines().next().unwrap().starts_with("Error: "), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    // --- warden security review msg #327 ---

    #[test]
    fn a_closure_defined_in_one_entry_still_calls_its_own_body_from_a_later_entry() {
        // Critical: each entry used to get an entirely fresh function
        // table starting back at index 0, so a closure persisted via
        // `globals` from an earlier entry became a dangling index once a
        // LATER entry's `Vm` ran against a different module -- silently
        // resolving to whatever unrelated function happened to occupy
        // that same index instead. Reproduced independently: `g` and `h`
        // are each the first (index-0) function compiled in their own
        // entry, so before the fix `(g 3)` silently ran `h`'s body
        // instead (`3 * 100`), printing 300 with no error at all.
        let (out, err, code) = run_session(
            "(define g (lambda (n) (+ n 1)))\n\
             (begin (define h (lambda (x) (* x 100))) (display (g 3)))\n",
        );
        assert_eq!(out, "> > 4> ");
        assert!(err.is_empty(), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn ordinary_non_tail_recursion_well_within_the_call_depth_limit_does_not_crash_the_session() {
        // High: a REPL entry used to run on whatever thread called
        // `repl::run`, with no dedicated big stack of its own (unlike
        // every other verb) -- so genuine, non-tail recursion nowhere
        // near `vm::MAX_CALL_DEPTH`'s own documented-safe limit crashed
        // the whole process via native stack overflow. Depth chosen to
        // match warden's own independently confirmed repro exactly.
        let (out, err, code) = run_session(
            "(begin (define (f n) (if (= n 0) 0 (+ 1 (f (- n 1))))) (display (f 100000)))\n",
        );
        assert_eq!(out, "> 100000> ");
        assert!(err.is_empty(), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    // --- qa test-design warning msg #330 / examiner verdict msg #331:
    // the same root cause (a fresh per-entry function table) surfaced in
    // three distinct, independently reproduced ways once function values,
    // not just plain values, were exercised across entries ---

    #[test]
    fn a_single_function_defined_in_one_entry_is_called_correctly_with_an_argument_from_a_later_entry()
     {
        // Before the fix: "Error: expected exactly 0 argument(s), got 1"
        // -- `inc`'s persisted closure index happened to alias the next
        // entry's own zero-arg top-level wrapper chunk instead of `inc`'s
        // real one-argument body.
        let (out, err, code) = run_session("(define (inc n) (+ n 1))\n(inc 5)\n");
        assert_eq!(out, "> > 6\n> ");
        assert!(err.is_empty(), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn two_functions_each_defined_in_their_own_entry_and_then_the_first_is_called_correctly() {
        // Examiner's own re-verification case (msg #331): each `define`
        // in its own entry, then a THIRD entry calls the first function
        // -- distinct from the single-entry `begin`-wrapped repro this
        // module already had, which only proved the bug across an
        // internal alias boundary, not a genuine separate top-level
        // entry-to-entry boundary for BOTH functions.
        let (out, err, code) = run_session(
            "(define g (lambda (n) (+ n 1)))\n(define h (lambda (x) (* x 100)))\n(g 3)\n",
        );
        assert_eq!(out, "> > > 4\n> ");
        assert!(err.is_empty(), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn a_zero_argument_function_defined_in_one_entry_terminates_when_called_from_a_later_entry() {
        // Examiner independently found this hangs indefinitely before the
        // fix (msg #331): the persisted zero-arg closure's index happened
        // to alias the LATER entry's own top-level wrapper chunk (also
        // zero-arg, so no arity error masked it) -- and since that
        // wrapper's own last form is exactly the same call in tail
        // position, it became a self-referential tail loop, trampolining
        // forever with no depth limit rather than erroring or crashing.
        // Run on a detached thread with a bounded wait so a real
        // regression fails this test cleanly and quickly instead of
        // hanging the whole suite.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(run_session("(define (f) 42)\n(f)\n"));
        });
        let (out, err, code) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect(
                "REPL session did not terminate within 10s -- likely a reintroduced \
                 infinite tail-call loop, not just a slow test",
            );
        assert_eq!(out, "> > 42\n> ");
        assert!(err.is_empty(), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn a_same_entry_definition_survives_a_later_failure_in_that_same_entry() {
        // The exact scenario `vm::eval_repl_entry`'s own doc comment cites
        // to justify threading `globals` through even on `Err` -- flagged
        // by qa (msg #330) as asserted but never actually tested.
        let (out, err, code) = run_session("(begin (define y 5) (car 5))\ny\n");
        assert_eq!(out, "> > 5\n> ");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        assert!(err.lines().next().unwrap().starts_with("Error: "), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn a_macro_defined_in_one_entry_does_not_persist_to_a_later_entry() {
        // Documented scope limitation (`ReplState`'s own doc comment):
        // only ordinary `define`d values persist across entries, not
        // `define-macro`. Flagged by qa (msg #330) as asserted but never
        // actually tested -- confirms this degrades cleanly (an ordinary
        // "unbound global" error), not a crash or a hang.
        let (out, err, code) =
            run_session("(define-macro (twice x) (list (quote begin) x x))\n(twice 1)\n");
        assert_eq!(out, "> > > ");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        assert!(err.lines().next().unwrap().starts_with("Error: "), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn a_read_error_entry_reports_one_error_line_and_returns_to_the_prompt() {
        // Flagged by qa (msg #330): only the runtime-error path had
        // coverage; a read error (malformed syntax, before compilation
        // even starts) was untested end to end through the REPL.
        let (out, err, code) = run_session("(display (+ 1\n");
        assert_eq!(out, "> > ");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        assert!(err.lines().next().unwrap().starts_with("Error: "), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn a_compile_error_entry_reports_one_error_line_and_returns_to_the_prompt() {
        // Flagged by qa (msg #330): only the runtime-error path had
        // coverage; a compile error (valid syntax, invalid semantics) was
        // untested end to end through the REPL.
        let (out, err, code) = run_session("(lambda)\n");
        assert_eq!(out, "> > ");
        assert_eq!(err.lines().count(), 1, "{err:?}");
        assert!(err.lines().next().unwrap().starts_with("Error: "), "{err:?}");
        assert_eq!(code, exitcode::SUCCESS);
    }
}
