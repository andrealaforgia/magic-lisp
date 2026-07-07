//! Interactive read-eval-print loop.

use std::io::{BufRead, Write};

use crate::{compiler, exitcode, reader, vm};

pub fn run(input: impl BufRead, out: &mut impl Write, err: &mut impl Write) -> i32 {
    for line in input.lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(message) = eval_line(trimmed, out) {
            let _ = writeln!(err, "{message}");
        }
    }
    exitcode::SUCCESS
}

fn eval_line(line: &str, out: &mut impl Write) -> Result<(), String> {
    let forms = reader::read_program(line).map_err(|e| e.to_string())?;
    let chunk = compiler::compile_program(&forms).map_err(|e| e.to_string())?;
    vm::run(&chunk, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn run_lines(src: &str) -> (String, String, i32) {
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
    fn evaluates_each_line_and_returns_success_at_eof() {
        let (out, err, code) = run_lines("(display (+ 1 2)) (newline)\n");
        assert_eq!(out, "3\n");
        assert!(err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn evaluates_multiple_lines_in_order() {
        let (out, _, _) = run_lines("(display 1)\n(display 2)\n");
        assert_eq!(out, "12");
    }

    #[test]
    fn reports_a_bad_line_on_stderr_and_keeps_going() {
        let (out, err, code) = run_lines("(display 1)\n)))\n(display 2)\n");
        assert_eq!(out, "12");
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn blank_input_produces_no_output_and_succeeds() {
        let (out, err, code) = run_lines("");
        assert!(out.is_empty());
        assert!(err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }
}
