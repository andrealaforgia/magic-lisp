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
