//! B17: the interactive REPL.

use super::helpers::{run_with_stdin, stderr_of, stdout_of};
use magiclisp::exitcode::SUCCESS;

#[test]
fn b17_e1_the_exact_prompt_bytes_appear_once_per_entry_plus_a_final_one_before_eof() {
    let output = run_with_stdin(&["repl"], b"1\n2\n");
    let stdout = stdout_of(&output);
    // Byte-level: exactly `> ` (0x3E 0x20), no trailing newline of its
    // own, appearing once per entry (2 entries here) plus one final time
    // right before end-of-input closes the session -- 3 total.
    let bytes = stdout.as_bytes();
    assert_eq!(bytes.windows(2).filter(|w| w == b"> ").count(), 3, "{stdout:?}");
    assert!(stdout.ends_with("> "), "{stdout:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn b17_e2_a_result_is_auto_printed_write_style_except_the_unspecified_value_of_a_define() {
    let numeric = run_with_stdin(&["repl"], b"(+ 1 2)\n");
    assert_eq!(stdout_of(&numeric), "> 3\n> ");

    let string_valued = run_with_stdin(&["repl"], b"\"hi\"\n");
    assert_eq!(
        stdout_of(&string_valued),
        "> \"hi\"\n> ",
        "a string result must print quoted (write style), not raw"
    );

    let define_entry = run_with_stdin(&["repl"], b"(define x 10)\n");
    assert_eq!(
        stdout_of(&define_entry),
        "> > ",
        "no stray output for a define's own unspecified result"
    );
}

#[test]
fn b17_e3_a_definition_persists_and_the_latest_redefinition_wins() {
    let output = run_with_stdin(
        &["repl"],
        b"(define x 10)\nx\n(define x 20)\nx\n",
    );
    assert_eq!(stdout_of(&output), "> > 10\n> > 20\n> ");
    assert!(stderr_of(&output).is_empty());
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn b17_e4_a_runtime_error_reports_exactly_one_error_line_and_leaves_bindings_intact() {
    let output = run_with_stdin(&["repl"], b"(define x 10)\n(car 5)\nx\n");
    assert_eq!(
        stdout_of(&output),
        "> > > 10\n> ",
        "nothing printed to stdout for the failing entry itself"
    );
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
    assert!(stderr.lines().next().unwrap().starts_with("Error: "), "{stderr:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn b17_e5_end_of_input_exits_with_success_even_when_no_entry_errored() {
    let output = run_with_stdin(&["repl"], b"1\n2\n3\n");
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn b17_e6_running_with_no_arguments_at_all_starts_the_identical_session() {
    let stdin_data = b"(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n";
    let bare = run_with_stdin(&[], stdin_data);
    let explicit = run_with_stdin(&["repl"], stdin_data);
    assert_eq!(stdout_of(&bare), stdout_of(&explicit));
    assert_eq!(stderr_of(&bare), stderr_of(&explicit));
    assert_eq!(bare.status.code(), explicit.status.code());
}

#[test]
fn b17_e7_the_full_demo_sequence_produces_exactly_the_prescribed_transcript() {
    let output = run_with_stdin(
        &["repl"],
        b"(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n",
    );
    assert_eq!(stdout_of(&output), "> 3\n> > 10\n> > 10\n> ");
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
    assert!(stderr.lines().next().unwrap().starts_with("Error: "), "{stderr:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

// --- qa test-design review msg #342: the critical cross-entry
// closure/function-index aliasing fix (warden msgs #327/#332, examiner
// verdict msg #331) had regression coverage only at the in-process
// `src/repl.rs` unit-test level, not paired with this project's usual
// subprocess-level CLI-integration coverage for a fix this consequential.
// These mirror those same unit tests, but drive the real compiled binary.

#[test]
fn a_single_function_defined_in_one_entry_is_called_correctly_with_an_argument_from_a_later_entry()
 {
    let output = run_with_stdin(&["repl"], b"(define (inc n) (+ n 1))\n(inc 5)\n");
    assert_eq!(stdout_of(&output), "> > 6\n> ");
    assert!(stderr_of(&output).is_empty());
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn two_functions_each_defined_in_their_own_entry_and_then_the_first_is_called_correctly() {
    let output = run_with_stdin(
        &["repl"],
        b"(define g (lambda (n) (+ n 1)))\n(define h (lambda (x) (* x 100)))\n(g 3)\n",
    );
    assert_eq!(
        stdout_of(&output),
        "> > > 4\n> ",
        "must call g's own body (4), not h's (which would print 300)"
    );
    assert!(stderr_of(&output).is_empty());
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn a_zero_argument_function_defined_in_one_entry_terminates_when_called_from_a_later_entry() {
    // `run_with_stdin`'s own CHILD_TIMEOUT (30s) is this test's hang
    // guard: a reintroduced regression here fails cleanly and quickly
    // instead of hanging the whole suite.
    let output = run_with_stdin(&["repl"], b"(define (f) 42)\n(f)\n");
    assert_eq!(stdout_of(&output), "> > 42\n> ");
    assert!(stderr_of(&output).is_empty());
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn ordinary_non_tail_recursion_well_within_the_call_depth_limit_does_not_crash_the_session() {
    let output = run_with_stdin(
        &["repl"],
        b"(begin (define (f n) (if (= n 0) 0 (+ 1 (f (- n 1))))) (display (f 100000)))\n",
    );
    assert_eq!(stdout_of(&output), "> 100000> ");
    assert!(stderr_of(&output).is_empty());
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn a_same_entry_definition_survives_a_later_failure_in_that_same_entry() {
    let output = run_with_stdin(&["repl"], b"(begin (define y 5) (car 5))\ny\n");
    assert_eq!(stdout_of(&output), "> > 5\n> ");
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
    assert!(stderr.lines().next().unwrap().starts_with("Error: "), "{stderr:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn a_macro_defined_in_one_entry_does_not_persist_to_a_later_entry() {
    let output = run_with_stdin(
        &["repl"],
        b"(define-macro (twice x) (list (quote begin) x x))\n(twice 1)\n",
    );
    assert_eq!(stdout_of(&output), "> > > ");
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
    assert!(stderr.lines().next().unwrap().starts_with("Error: "), "{stderr:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn a_read_error_entry_reports_one_error_line_and_returns_to_the_prompt() {
    let output = run_with_stdin(&["repl"], b"(display (+ 1\n");
    assert_eq!(stdout_of(&output), "> > ");
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
    assert!(stderr.lines().next().unwrap().starts_with("Error: "), "{stderr:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

#[test]
fn a_compile_error_entry_reports_one_error_line_and_returns_to_the_prompt() {
    let output = run_with_stdin(&["repl"], b"(lambda)\n");
    assert_eq!(stdout_of(&output), "> > ");
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
    assert!(stderr.lines().next().unwrap().starts_with("Error: "), "{stderr:?}");
    assert_eq!(output.status.code(), Some(SUCCESS));
}
