//! Shared process-spawning and assertion helpers used across every feature
//! slice's tests below. Kept in one place so each slice module only carries
//! its own test bodies.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use magiclisp::exitcode::{BAD_ARTIFACT, SUCCESS};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_path(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "magiclisp-integration-{}-{n}-{label}",
        std::process::id()
    ))
}

pub(crate) fn write_source(label: &str, content: &str) -> PathBuf {
    let path = temp_path(label);
    std::fs::write(&path, content).unwrap();
    path
}

fn magiclisp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_magiclisp"))
}

pub(crate) fn run(args: &[&str]) -> Output {
    magiclisp().args(args).output().expect("binary should run")
}

pub(crate) fn run_with_stdin(args: &[&str], stdin_data: &[u8]) -> Output {
    let mut child = magiclisp()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");
    child.stdin.take().unwrap().write_all(stdin_data).unwrap();
    child.wait_with_output().expect("binary should run")
}

pub(crate) fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

pub(crate) fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

pub(crate) fn compile_good_artifact(label: &str) -> Vec<u8> {
    let file = write_source(&format!("{label}.ml"), "(display 1)");
    let artifact = temp_path(&format!("{label}.mlbc"));
    let out = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(SUCCESS));
    std::fs::read(&artifact).unwrap()
}

pub(crate) fn assert_rejected_as_bad_artifact(bytes: &[u8], label: &str) {
    let artifact = temp_path(&format!("{label}.mlbc"));
    std::fs::write(&artifact, bytes).unwrap();

    let run_output = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(
        run_output.status.code(),
        Some(BAD_ARTIFACT),
        "run should reject, stderr: {}",
        stderr_of(&run_output)
    );
    assert!(!stderr_of(&run_output).is_empty(), "run stderr");

    let disasm_output = run(&["disasm", artifact.to_str().unwrap()]);
    assert_eq!(
        disasm_output.status.code(),
        Some(BAD_ARTIFACT),
        "disasm should reject, stderr: {}",
        stderr_of(&disasm_output)
    );
    assert!(!stderr_of(&disasm_output).is_empty(), "disasm stderr");
}

/// Runs `src`, asserting the process succeeded, and returns its stdout.
/// Used by every B2/B3 test that only cares about the final displayed value.
pub(crate) fn eval_ok(label: &str, src: &str) -> String {
    let file = write_source(label, src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    stdout_of(&output)
}

/// Like `eval_ok`, but appends `(newline)` first — used by the E12 demo
/// programs, whose prescribed output always ends with a trailing newline.
pub(crate) fn run_demo(label: &str, src: &str) -> String {
    let full_src = format!("{src} (newline)");
    let file = write_source(label, &full_src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        stderr_of(&output)
    );
    stdout_of(&output)
}
