//! B1: walking-skeleton CLI (E1-E11).

use std::path::Path;

use magiclisp::exitcode::{BAD_ARTIFACT, RUNTIME_ERROR, SOURCE_ERROR, SUCCESS, USAGE_ERROR};

use super::helpers::{
    assert_rejected_as_bad_artifact, compile_good_artifact, run, run_with_stdin, stderr_of,
    stdout_of, temp_path, write_source,
};

// E1: `magiclisp eval <file>` on `(display (+ 1 2)) (newline)` prints "3\n" and exits 0.
#[test]
fn e1_eval_prints_computed_sum_and_exits_success() {
    let file = write_source("e1.ml", "(display (+ 1 2)) (newline)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(stdout_of(&output), "3\n");
    assert_eq!(output.status.code(), Some(SUCCESS));
}

// E2: compile then run reproduces eval's output byte-for-byte, across two process invocations.
#[test]
fn e2_compile_then_run_reproduces_eval_output_across_process_boundaries() {
    let file = write_source("e2.ml", "(display (+ 1 2)) (newline)");
    let artifact = temp_path("e2.mlbc");

    let eval_output = run(&["eval", file.to_str().unwrap()]);

    let compile_output = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(compile_output.status.code(), Some(SUCCESS));

    let run_output = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(stdout_of(&run_output), stdout_of(&eval_output));
    assert_eq!(stdout_of(&run_output), "3\n");
    assert_eq!(run_output.status.code(), Some(SUCCESS));
}

// E3: disasm of that artifact prints a legible instruction listing and exits 0.
#[test]
fn e3_disasm_prints_a_legible_instruction_listing() {
    let file = write_source("e3.ml", "(display (+ 1 2)) (newline)");
    let artifact = temp_path("e3.mlbc");
    run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);

    let output = run(&["disasm", artifact.to_str().unwrap()]);
    let text = stdout_of(&output);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert!(
        text.is_ascii(),
        "disasm output should be legible text: {text}"
    );
    assert!(text.contains("CALL"));
    assert!(text.contains("HALT"));
    // not raw bytes: every line should be printable text, not binary garbage
    assert!(text.lines().count() >= 3);
}

// E4: all five verbs route distinctly; none silently ignored, confused, or unrouted.
// One test per verb so a failure names exactly which verb broke.

#[test]
fn e4_eval_verb_evaluates_source_and_prints_the_result() {
    let file = write_source("e4-eval.ml", "(display 1)");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(stdout_of(&out), "1");
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e4_run_verb_executes_a_compiled_artifact() {
    let file = write_source("e4-run.ml", "(display 1)");
    let artifact = temp_path("e4-run.mlbc");
    run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    let out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(stdout_of(&out), "1");
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e4_disasm_verb_describes_the_artifact_without_executing_it() {
    let file = write_source("e4-disasm.ml", "(display 1)");
    let artifact = temp_path("e4-disasm.mlbc");
    run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    let out = run(&["disasm", artifact.to_str().unwrap()]);
    assert!(stdout_of(&out).contains("HALT"));
    // disasm's job is to describe, not execute — its stdout must not be program output
    assert!(!stdout_of(&out).trim_start().starts_with('1'));
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e4_compile_verb_writes_an_artifact_file_to_disk() {
    let file = write_source("e4-compile.ml", "(display 1)");
    let artifact = temp_path("e4-compile.mlbc");
    let out = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(SUCCESS));
    assert!(Path::new(&artifact).exists());
}

#[test]
fn e4_repl_verb_evaluates_lines_from_stdin() {
    // B17 gives the REPL its own real prompt ("> ") before each entry;
    // `display`'s own side-effect output ("1") is interleaved with those
    // prompts, since `display` itself returns the unspecified value (no
    // auto-print of its own).
    let out = run_with_stdin(&["repl"], b"(display 1)\n");
    assert_eq!(stdout_of(&out), "> 1> ");
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e4_no_verb_given_defaults_to_repl() {
    let out = run_with_stdin(&[], b"(display 1)\n");
    assert_eq!(stdout_of(&out), "> 1> ");
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e4_unknown_verb_fails_cleanly_with_a_usage_error_instead_of_hanging_or_crashing() {
    let file = write_source("e4-unknown.ml", "(display 1)");
    let out = run(&["frobnicate", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(USAGE_ERROR));
}

// E5: reader accepts numbers, symbols, escaped strings, booleans, nested lists, comments,
// whitespace. One test per construct, plus one proving they compose in a single file.

#[test]
fn e5_reads_a_leading_comment_line() {
    let file = write_source("e5-comment.ml", "; a leading comment\n(display 1)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(stdout_of(&output), "1");
}

#[test]
fn e5_reads_a_string_literal_with_every_supported_escape() {
    let file = write_source(
        "e5-escapes.ml",
        r#"(display "line one\nline two\ttabbed\r\"quoted\"\\backslash")"#,
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(
        stdout_of(&output),
        "line one\nline two\ttabbed\r\"quoted\"\\backslash"
    );
}

#[test]
fn e5_reads_and_evaluates_a_nested_arithmetic_expression() {
    let file = write_source("e5-nested.ml", "(display (+ 42 (+ 1 2)))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(stdout_of(&output), "45");
}

#[test]
fn e5_reads_boolean_literals() {
    let file = write_source("e5-bools.ml", "(display true) (newline) (display false)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(stdout_of(&output), "#t\n#f");
}

#[test]
fn e5_reads_a_source_file_exercising_every_supported_construct_together() {
    // The tests above prove each construct is read correctly in isolation;
    // this proves they also compose without interference when combined in a
    // single file, per E5's "read together" requirement — so it must check
    // the actual combined output, not just that the process didn't crash.
    let src = r#"
        ; a leading comment
        (display "line one\nline two\ttabbed\r\"quoted\"\\backslash") (newline)
        (display (+ 42 (+ 1 2))) (newline)
        (display true) (newline)
        (display false) (newline)
    "#;
    let file = write_source("e5-combined.ml", src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(
        stdout_of(&output),
        "line one\nline two\ttabbed\r\"quoted\"\\backslash\n45\n#t\n#f\n"
    );
}

// E6: a raw unescaped newline inside a string literal is a read error, exit code = source error.
#[test]
fn e6_raw_newline_inside_string_literal_is_rejected_as_a_read_error() {
    let file = write_source("e6.ml", "(display \"broken\nstring\")");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&output).is_empty());
    assert!(stdout_of(&output).is_empty());
}

// E7: run/disasm reject bad magic, bad version, truncated content, and bad internal
// pointers. One test per corruption case so a failure names exactly which shape broke.

#[test]
fn e7_rejects_bad_magic() {
    let mut bytes = compile_good_artifact("e7-magic");
    bytes[0..4].copy_from_slice(b"NOPE");
    assert_rejected_as_bad_artifact(&bytes, "e7-magic-bad");
}

#[test]
fn e7_rejects_unsupported_version() {
    let mut bytes = compile_good_artifact("e7-version");
    bytes[4] = 200;
    assert_rejected_as_bad_artifact(&bytes, "e7-version-bad");
}

#[test]
fn e7_rejects_truncated_content() {
    let bytes = compile_good_artifact("e7-truncated");
    let truncated = bytes[..bytes.len() - 4].to_vec();
    assert_rejected_as_bad_artifact(&truncated, "e7-truncated-bad");
}

#[test]
fn e7_rejects_an_out_of_range_internal_pointer() {
    let mut bytes = compile_good_artifact("e7-pointer");
    // entry_index is the 4 LE bytes right after magic(4)+major(1)+minor(1)+flags(2)
    bytes[8..12].copy_from_slice(&999u32.to_le_bytes());
    assert_rejected_as_bad_artifact(&bytes, "e7-pointer-bad");
}

// E8: the five failure classes map to five pairwise-distinct exit codes. One test per
// class proves its own mapping; `exitcode::tests::all_five_classes_are_pairwise_distinct`
// (in src/exitcode.rs) proves the five named constants used here can't collide.

#[test]
fn e8_success_exit_code_for_a_valid_program() {
    let file = write_source("e8-success.ml", "(display (+ 1 2)) (newline)");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e8_usage_error_exit_code_for_a_missing_required_argument() {
    let out = run(&["eval"]);
    assert_eq!(out.status.code(), Some(USAGE_ERROR));
}

#[test]
fn e8_source_error_exit_code_for_unreadable_source() {
    let bad_source = write_source("e8-bad-source.ml", "\"unterminated");
    let out = run(&["eval", bad_source.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
}

#[test]
fn e8_bad_artifact_exit_code_for_a_corrupt_artifact() {
    let corrupt_artifact = temp_path("e8-corrupt.mlbc");
    std::fs::write(&corrupt_artifact, b"garbage, not MLBC").unwrap();
    let out = run(&["run", corrupt_artifact.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(BAD_ARTIFACT));
}

#[test]
fn e8_runtime_error_exit_code_for_an_undefined_global() {
    let bad_runtime_file = write_source("e8-bad-runtime.ml", "(this-global-is-undefined)");
    let out = run(&["eval", bad_runtime_file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(RUNTIME_ERROR));
}

// E9: display/newline/+ (0, 1, 2, >2 args) all work; output is ordered and flushed.
// One test per arg-count case, plus one dedicated to ordering/flushing.

#[test]
fn e9_plus_with_zero_arguments_is_zero() {
    let file = write_source("e9-zero.ml", "(display (+))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "0");
}

#[test]
fn e9_plus_with_one_argument_is_that_argument() {
    let file = write_source("e9-one.ml", "(display (+ 5))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "5");
}

#[test]
fn e9_plus_with_two_arguments_sums_them() {
    let file = write_source("e9-two.ml", "(display (+ 1 2))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "3");
}

#[test]
fn e9_plus_with_more_than_two_arguments_sums_them_all() {
    let file = write_source("e9-many.ml", "(display (+ 1 2 3 4))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "10");
}

#[test]
fn e9_multiple_displays_and_newlines_appear_in_order_and_are_fully_flushed() {
    let file = write_source(
        "e9-order.ml",
        "(display 1) (newline) (display 2) (newline) (display 3) (newline)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "1\n2\n3\n");
}

// E10 (integration): source on disk -> compile -> save -> (new process) load -> run,
// and (new process) load -> disasm, all consistent, for a program that displays a sum.
#[test]
fn e10_full_pipeline_round_trips_across_process_boundaries() {
    let source_file = write_source("e10.ml", "(display (+ 19 23)) (newline)");
    let artifact = temp_path("e10.mlbc");

    let compile_output = run(&[
        "compile",
        source_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(compile_output.status.code(), Some(SUCCESS));
    assert!(Path::new(&artifact).exists());

    // separate process invocation: run the saved artifact
    let run_output = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(stdout_of(&run_output), "42\n");
    assert_eq!(run_output.status.code(), Some(SUCCESS));

    // separate process invocation: disassemble the same saved artifact
    let disasm_output = run(&["disasm", artifact.to_str().unwrap()]);
    assert_eq!(disasm_output.status.code(), Some(SUCCESS));
    assert!(stdout_of(&disasm_output).contains("HALT"));

    // and eval'ing the original source directly agrees with the compiled pipeline
    let eval_output = run(&["eval", source_file.to_str().unwrap()]);
    assert_eq!(stdout_of(&eval_output), stdout_of(&run_output));
}

// E11: compiling the same source text twice, in two separate process
// invocations, produces byte-identical artifact files — no incidental
// nondeterminism (timestamps, ids, unordered iteration) leaks into MLBC.
#[test]
fn e11_compiling_the_same_source_twice_produces_byte_identical_artifacts() {
    let source = write_source(
        "e11.ml",
        "(display (+ 1 2)) (newline) (display \"hi\") (display true) (display false)",
    );
    let artifact_a = temp_path("e11-a.mlbc");
    let artifact_b = temp_path("e11-b.mlbc");

    let out_a = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        artifact_a.to_str().unwrap(),
    ]);
    assert_eq!(out_a.status.code(), Some(SUCCESS));

    let out_b = run(&[
        "compile",
        source.to_str().unwrap(),
        "-o",
        artifact_b.to_str().unwrap(),
    ]);
    assert_eq!(out_b.status.code(), Some(SUCCESS));

    let bytes_a = std::fs::read(&artifact_a).unwrap();
    let bytes_b = std::fs::read(&artifact_b).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "two compiles of the same source must be byte-identical"
    );
}
