//! B12: input reading and the write/display output distinction (spec 3.2, 4.8).

use super::helpers::{eval_ok, run_with_stdin, stdout_of, write_source};
use magiclisp::exitcode::SUCCESS;

fn eval_ok_with_stdin(label: &str, src: &str, stdin_data: &[u8]) -> String {
    let file = write_source(label, src);
    let output = run_with_stdin(&["eval", file.to_str().unwrap()], stdin_data);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_of(&output)
}

#[test]
fn b12_e1_read_returns_data_unevaluated_advances_and_eof_shown_both_ways() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e1a.ml",
            "(define d (read)) (write d) (newline) (display (+ 1 2))",
            b"(+ 1 2)"
        ),
        "(+ 1 2)\n3"
    );
    assert_eq!(
        eval_ok_with_stdin("b12-e1b.ml", "(display (read)) (display (read))", b"1 2"),
        "12"
    );
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e1c.ml",
            "(display (eof-object? (read))) (display (eof-object? (read)))",
            b"1"
        ),
        "#f#t"
    );
}

#[test]
fn b12_e2_read_line_reads_successive_lines_then_eof_with_terminator_stripped() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e2a.ml",
            "(display (read-line)) (newline) (display (read-line)) (newline) \
             (display (eof-object? (read-line))) (newline)",
            b"hello\nworld\n"
        ),
        "hello\nworld\n#t\n"
    );
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e2b.ml",
            "(display (string-length (read-line)))",
            b"hello\n"
        ),
        "5"
    );
}

#[test]
fn b12_e3_display_prints_raw_strings_and_bare_characters() {
    assert_eq!(eval_ok("b12-e3a.ml", "(display \"a\\nb\")"), "a\nb");
    assert_eq!(eval_ok("b12-e3b.ml", "(display #\\a)"), "a");
}

#[test]
fn b12_e4_write_prints_escaped_strings_named_characters_and_matches_display_for_ordinary_values() {
    assert_eq!(eval_ok("b12-e4a.ml", "(write \"a\\nb\")"), "\"a\\nb\"");
    assert_eq!(eval_ok("b12-e4b.ml", "(write (quote sym))"), "sym");
    assert_eq!(eval_ok("b12-e4c.ml", "(write #\\space)"), "#\\space");
    assert_eq!(eval_ok("b12-e4d.ml", "(display #\\space)"), " ");
    assert_eq!(
        eval_ok("b12-e4e.ml", "(write 42)"),
        eval_ok("b12-e4f.ml", "(display 42)")
    );
    assert_eq!(
        eval_ok("b12-e4g.ml", "(write (list 1 2 3))"),
        eval_ok("b12-e4h.ml", "(display (list 1 2 3))")
    );
}

#[test]
fn b12_e5_all_output_present_and_in_order_with_interleaved_reads_and_writes() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e5.ml",
            "(display \"start\") (newline) (display (read-line)) (newline) (display \"end\")",
            b"middle\n"
        ),
        "start\nmiddle\nend"
    );
}

#[test]
fn b12_e6_all_three_demo_scenarios_produce_exactly_the_prescribed_output() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e6a.ml",
            "(define d (read)) (write d) (newline) (display (+ 1 2)) (newline)",
            b"(+ 1 2)\n"
        ),
        "(+ 1 2)\n3\n"
    );
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e6b.ml",
            "(display (read-line)) (newline) (display (read-line)) (newline) \
             (display (eof-object? (read-line))) (newline)",
            b"hello\nworld\n"
        ),
        "hello\nworld\n#t\n"
    );
    assert_eq!(
        eval_ok(
            "b12-e6c.ml",
            "(write \"a\\nb\") (newline) (display \"a\\nb\") (newline) \
             (write (quote sym)) (newline)"
        ),
        "\"a\\nb\"\na\nb\nsym\n"
    );
}
