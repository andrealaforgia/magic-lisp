//! Interactive read-eval-print loop (B17, spec 9.1).

use std::collections::HashMap;
use std::io::{BufRead, Write};

use crate::value::{Value, write_repr};
use crate::{compiler, exitcode, reader, vm};

/// The exact prompt text (spec 9.1): a greater-than sign and one space,
/// with no trailing newline -- the whole point is that the next thing
/// written to `out` (either the user's own echoed input on a real
/// terminal, or an entry's result) appears to continue the same line.
const PROMPT: &str = "> ";

pub fn run(mut input: impl BufRead, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let mut globals = vm::default_globals();
    loop {
        let _ = write!(out, "{PROMPT}");
        let _ = out.flush();
        let mut line = String::new();
        // `Ok(0)` is genuine end-of-input; a read error is treated the
        // same way (end the session) rather than looping forever on a
        // stream that can't make progress.
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (result, updated_globals) = eval_entry(trimmed, globals, out);
        globals = updated_globals;
        match result {
            Ok(value) => {
                // The unspecified value (e.g. a `define`'s own result)
                // prints nothing at all, not even a blank line.
                if !matches!(value, Value::Unspecified) {
                    let _ = writeln!(out, "{}", write_repr(&value));
                }
            }
            Err(message) => {
                let _ = writeln!(err, "Error: {message}");
            }
        }
    }
    exitcode::SUCCESS
}

/// Reads, compiles, and evaluates one entry, threading `globals` in and
/// returning the updated map back out regardless of outcome -- see
/// `vm::eval_repl_entry`'s own doc comment for why this must hold even on
/// a failing entry. A read or compile error is reported the same way as a
/// runtime one (both end up as one "Error: ..." line and a return to the
/// prompt): B17 draws no observable distinction between them, unlike the
/// non-REPL CLI paths (B1/B15), which is specific to those verbs running
/// a whole program once, not accumulating state entry by entry.
fn eval_entry(
    src: &str,
    globals: HashMap<String, Value>,
    out: &mut impl Write,
) -> (Result<Value, String>, HashMap<String, Value>) {
    let forms = match reader::read_program(src) {
        Ok(forms) => forms,
        Err(e) => return (Err(e.to_string()), globals),
    };
    let (module, fn_index) = match compiler::compile_repl_entry(&forms) {
        Ok(compiled) => compiled,
        Err(e) => return (Err(e.to_string()), globals),
    };
    let (result, updated_globals) = vm::eval_repl_entry(&module, fn_index, globals, out);
    (result.map_err(|e| e.message), updated_globals)
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
}
