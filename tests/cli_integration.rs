//! Process-level acceptance tests: they invoke the real `magiclisp` binary as a
//! separate OS process, exactly the way a user's shell would, and check its
//! actual stdout/stderr/exit code. This is what proves the CLI as a whole
//! (not just the library functions behind it) satisfies B1's expectations.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SUCCESS: i32 = 0;
const USAGE_ERROR: i32 = 64;
const SOURCE_ERROR: i32 = 65;
const BAD_ARTIFACT: i32 = 66;
const RUNTIME_ERROR: i32 = 70;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "magiclisp-integration-{}-{n}-{label}",
        std::process::id()
    ))
}

fn write_source(label: &str, content: &str) -> PathBuf {
    let path = temp_path(label);
    std::fs::write(&path, content).unwrap();
    path
}

fn magiclisp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_magiclisp"))
}

fn run(args: &[&str]) -> Output {
    magiclisp().args(args).output().expect("binary should run")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

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
#[test]
fn e4_all_five_verbs_are_routed_to_distinct_handling() {
    let ok_file = write_source("e4-ok.ml", "(display 1)");
    let artifact = temp_path("e4.mlbc");
    run(&[
        "compile",
        ok_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);

    let eval_out = run(&["eval", ok_file.to_str().unwrap()]);
    assert_eq!(stdout_of(&eval_out), "1");

    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(stdout_of(&run_out), "1");

    let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
    assert!(stdout_of(&disasm_out).contains("HALT"));
    // disasm's job is to describe, not execute — its stdout must not be program output
    assert!(!stdout_of(&disasm_out).trim_start().starts_with('1'));

    let compile_out = run(&[
        "compile",
        ok_file.to_str().unwrap(),
        "-o",
        temp_path("e4-out.mlbc").to_str().unwrap(),
    ]);
    assert_eq!(compile_out.status.code(), Some(SUCCESS));

    // repl fed a single line via stdin is distinct from all the above verbs
    let mut child = magiclisp()
        .arg("repl")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write as _;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"(display 1)\n")
        .unwrap();
    let repl_out = child.wait_with_output().unwrap();
    assert_eq!(stdout_of(&repl_out), "1");
    assert_eq!(repl_out.status.code(), Some(SUCCESS));

    // no-verb default also reaches the repl, not some other unrouted path
    let mut default_child = magiclisp()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    default_child
        .stdin
        .take()
        .unwrap()
        .write_all(b"(display 1)\n")
        .unwrap();
    let default_out = default_child.wait_with_output().unwrap();
    assert_eq!(stdout_of(&default_out), "1");
    assert_eq!(default_out.status.code(), Some(SUCCESS));

    // an unknown verb fails cleanly and distinctly rather than hanging or crashing
    let unknown = run(&["frobnicate", ok_file.to_str().unwrap()]);
    assert_eq!(unknown.status.code(), Some(USAGE_ERROR));
}

// E5: reader accepts numbers, symbols, escaped strings, booleans, nested lists, comments, whitespace.
#[test]
fn e5_reads_a_source_file_exercising_every_supported_construct_together() {
    let src = r#"
        ; a leading comment
        (display "line one\nline two\ttabbed\r\"quoted\"\\backslash") (newline)
        (display (+ 42 (+ 1 2))) (newline)
        (display true) (newline)
        (display false) (newline)
    "#;
    let file = write_source("e5.ml", src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(
        stdout_of(&output),
        "line one\nline two\ttabbed\r\"quoted\"\\backslash\n45\ntrue\nfalse\n"
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

// E7: run/disasm reject bad magic, bad version, truncated content, and bad internal pointers.
#[test]
fn e7_run_and_disasm_reject_every_invalid_artifact_shape() {
    let good_file = write_source("e7-good.ml", "(display 1)");
    let good_artifact = temp_path("e7-good.mlbc");
    run(&[
        "compile",
        good_file.to_str().unwrap(),
        "-o",
        good_artifact.to_str().unwrap(),
    ]);
    let good_bytes = std::fs::read(&good_artifact).unwrap();

    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("bad-magic", {
            let mut b = good_bytes.clone();
            b[0..4].copy_from_slice(b"NOPE");
            b
        }),
        ("bad-version", {
            let mut b = good_bytes.clone();
            b[4] = 200;
            b
        }),
        ("truncated", good_bytes[..good_bytes.len() - 4].to_vec()),
        ("bad-pointer", {
            let mut b = good_bytes.clone();
            // entry_index is the 4 LE bytes right after magic(4)+major(1)+minor(1)+flags(2)
            b[8..12].copy_from_slice(&999u32.to_le_bytes());
            b
        }),
    ];

    for (label, bytes) in cases {
        let artifact = temp_path(&format!("e7-{label}.mlbc"));
        std::fs::write(&artifact, &bytes).unwrap();

        let run_output = run(&["run", artifact.to_str().unwrap()]);
        assert_eq!(
            run_output.status.code(),
            Some(BAD_ARTIFACT),
            "case {label}: run should reject, stderr: {}",
            stderr_of(&run_output)
        );
        assert!(
            !stderr_of(&run_output).is_empty(),
            "case {label}: run stderr"
        );

        let disasm_output = run(&["disasm", artifact.to_str().unwrap()]);
        assert_eq!(
            disasm_output.status.code(),
            Some(BAD_ARTIFACT),
            "case {label}: disasm should reject, stderr: {}",
            stderr_of(&disasm_output)
        );
        assert!(
            !stderr_of(&disasm_output).is_empty(),
            "case {label}: disasm stderr"
        );
    }
}

// E8: the five failure classes map to five pairwise-distinct exit codes.
#[test]
fn e8_the_five_failure_classes_have_pairwise_distinct_exit_codes() {
    let good_file = write_source("e8-good.ml", "(display (+ 1 2)) (newline)");

    let success = run(&["eval", good_file.to_str().unwrap()]);
    assert_eq!(success.status.code(), Some(SUCCESS));

    let bad_usage = run(&["eval"]); // missing required file argument
    assert_eq!(bad_usage.status.code(), Some(USAGE_ERROR));

    let bad_source = write_source("e8-bad-source.ml", "\"unterminated");
    let source_error = run(&["eval", bad_source.to_str().unwrap()]);
    assert_eq!(source_error.status.code(), Some(SOURCE_ERROR));

    let corrupt_artifact = temp_path("e8-corrupt.mlbc");
    std::fs::write(&corrupt_artifact, b"garbage, not MLBC").unwrap();
    let bad_artifact = run(&["run", corrupt_artifact.to_str().unwrap()]);
    assert_eq!(bad_artifact.status.code(), Some(BAD_ARTIFACT));

    let bad_runtime_file = write_source("e8-bad-runtime.ml", "(this-global-is-undefined)");
    let runtime_error = run(&["eval", bad_runtime_file.to_str().unwrap()]);
    assert_eq!(runtime_error.status.code(), Some(RUNTIME_ERROR));

    let codes = [
        success.status.code().unwrap(),
        bad_usage.status.code().unwrap(),
        source_error.status.code().unwrap(),
        bad_artifact.status.code().unwrap(),
        runtime_error.status.code().unwrap(),
    ];
    let unique: std::collections::HashSet<_> = codes.iter().collect();
    assert_eq!(
        unique.len(),
        codes.len(),
        "exit codes must be pairwise distinct: {codes:?}"
    );
}

// E9: display/newline/+ (0, 1, 2, >2 args) all work; output is ordered and flushed.
#[test]
fn e9_builtins_behave_correctly_and_output_is_ordered_and_flushed() {
    let src = "\
        (display (+)) (newline) \
        (display (+ 5)) (newline) \
        (display (+ 1 2)) (newline) \
        (display (+ 1 2 3 4)) (newline)";
    let file = write_source("e9.ml", src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "0\n5\n3\n10\n");
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
