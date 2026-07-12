//! Shared process-spawning and assertion helpers used across every feature
//! slice's tests below. Kept in one place so each slice module only carries
//! its own test bodies.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

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

/// The wall-clock ceiling for [`run_with_stdin`]'s child process. Every
/// real scenario finishes in well under a second; this only exists to turn
/// a genuine hang-class regression into a fast, clear test failure instead
/// of blocking the whole suite indefinitely (qa test-design review msg
/// #213) -- generous on purpose, mirroring this project's own established
/// "the pre-fix bug never finished quickly, so any reasonable ceiling
/// distinguishes fixed from broken" reasoning.
const CHILD_TIMEOUT: Duration = Duration::from_secs(30);

/// Spawns `magiclisp`, feeds it `stdin_data`, and returns its `Output`.
///
/// Writes stdin and drains stdout/stderr concurrently on separate threads
/// rather than writing synchronously before waiting: a child that produces
/// enough output to fill the OS pipe buffer (~64KB) while still blocked
/// waiting for more stdin would otherwise deadlock the harness itself --
/// `std::process`'s own docs warn against exactly this pattern. Also
/// enforces `CHILD_TIMEOUT` via `try_wait` polling, since production code
/// (`vm::run_with_stdin`) goes to real lengths to structurally rule out
/// hangs and this harness testing it deserves the equivalent discipline,
/// especially given this project's specific hang-class-bug history.
pub(crate) fn run_with_stdin(args: &[&str], stdin_data: &[u8]) -> Output {
    let mut child = magiclisp()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdin_data = stdin_data.to_vec();

    let stdin_writer = std::thread::spawn(move || {
        // A closed pipe (the child exited before consuming all of stdin,
        // e.g. it never called read/read-line) is an expected outcome for
        // some scenarios, not a harness bug.
        let _ = stdin.write_all(&stdin_data);
    });
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout
            .read_to_end(&mut buf)
            .expect("reading child stdout should not fail");
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stderr
            .read_to_end(&mut buf)
            .expect("reading child stderr should not fail");
        buf
    });

    let status = wait_with_timeout(&mut child, CHILD_TIMEOUT);
    let stdout = stdout_reader
        .join()
        .expect("stdout reader thread should not panic");
    let stderr = stderr_reader
        .join()
        .expect("stderr reader thread should not panic");
    stdin_writer
        .join()
        .expect("stdin writer thread should not panic");

    Output {
        status,
        stdout,
        stderr,
    }
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .expect("polling child status should not fail")
        {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "magiclisp process did not exit within {timeout:?} -- likely a hang, not a slow test"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
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

/// Runs the real binary under `/usr/bin/time -l` (macOS/BSD), returning its
/// own `Output` (stdout/stderr/exit code all pass through untouched) plus
/// the peak resident-set-size `time` reports, in bytes — real measured
/// memory, not an inference from "it didn't crash". Used by B21's flat-
/// memory checks, which need to prove O(1) space, not just completion.
pub(crate) fn run_with_peak_rss(args: &[&str]) -> (Output, u64) {
    let output = Command::new("/usr/bin/time")
        .arg("-l")
        .arg(env!("CARGO_BIN_EXE_magiclisp"))
        .args(args)
        .output()
        .expect("/usr/bin/time should run");
    let time_report = String::from_utf8_lossy(&output.stderr);
    let rss = time_report
        .lines()
        .find(|line| line.contains("maximum resident set size"))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or_else(|| {
            panic!("could not find peak RSS in `/usr/bin/time -l` output: {time_report}")
        });
    (output, rss)
}

/// Asserts `elapsed` is within `ceiling`, but only on an optimised release
/// build (`!cfg!(debug_assertions)`) — an ordinary unoptimized debug build
/// blows well past any of these ceilings for reasons unrelated to any real
/// regression (qa test-design review msg #71: confirmed empirically, e.g.
/// B21's E1 loop alone runs ~2.5s released vs ~28s debug), so a ceiling
/// checked unconditionally would be a routine flake under `cargo test`
/// without `--release`, not a meaningful regression guard. Correctness
/// should still be asserted unconditionally by the caller, in both
/// profiles — this only ever skips the *timing* check.
pub(crate) fn assert_within_release_ceiling(elapsed: Duration, ceiling: Duration, label: &str) {
    if cfg!(debug_assertions) {
        return;
    }
    assert!(
        elapsed <= ceiling,
        "{label} took {elapsed:?}, exceeding the {ceiling:?} release-build ceiling"
    );
}
