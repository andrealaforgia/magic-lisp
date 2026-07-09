//! Per-scenario state and process-spawning helpers, mirroring
//! `tests/cli_integration/helpers.rs`'s process-level rigor (spawn the
//! real compiled binary, assert on its real stdout/stderr/exit code) but
//! kept in this separate test binary since `tests/features.rs` and
//! `tests/cli_integration.rs` are independent crates.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_path(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "magiclisp-features-{}-{n}-{label}",
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

/// Runs the real binary under `/usr/bin/time -l` (macOS/BSD), returning its
/// own `Output` (stdout/stderr/exit code all pass through untouched) plus
/// the peak resident-set-size `time` reports, in bytes — real measured
/// memory, not an inference from "it didn't crash". Used by B6's flat-
/// memory scenarios, which need to prove O(1) space, not just completion.
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

pub(crate) fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

pub(crate) fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// Runs `src` via `eval` and returns its stdout, panicking with the
/// process's stderr if it didn't exit successfully — for the many steps
/// that only care about a program's displayed output.
pub(crate) fn eval_ok(label: &str, src: &str) -> String {
    let file = write_source(label, src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "expected {label} to succeed, stderr: {}",
        stderr_of(&output)
    );
    stdout_of(&output)
}

/// Wraps `src` in `(display ...)` unless it already contains a `display`
/// call somewhere (i.e. it's already a complete program that prints its
/// own result, typically because it also needs to prove a side effect
/// happened or didn't). Several scenarios queue a bare expression via a
/// shared, generically-worded When step ("it is evaluated", "each is
/// evaluated") that doesn't know in advance which shape it'll get.
pub(crate) fn maybe_wrap_display(src: &str) -> String {
    if src.contains("(display") {
        src.to_string()
    } else {
        format!("(display {src})")
    }
}

/// Runs every pending MagicLisp snippet queued in `world.pending` (via
/// [`maybe_wrap_display`]), appending each result to `world.notes` — the
/// shared implementation behind the several generically-worded When steps
/// ("it is evaluated", "each is evaluated", "the function is called",
/// "each is run") that different scenarios reuse verbatim with different
/// Givens.
pub(crate) fn run_pending(world: &mut World, label_prefix: &str) {
    let pending = std::mem::take(&mut world.pending);
    for (i, src) in pending.iter().enumerate() {
        let program = maybe_wrap_display(src);
        world
            .notes
            .push(eval_ok(&format!("{label_prefix}-{i}.ml"), &program));
    }
}

/// Per-scenario state, reset fresh for each scenario the runner executes.
/// Steps stash whatever the *next* step needs here — e.g. a Given writes a
/// source file and records its path, the following When runs it and
/// records the Output, the following Then reads that Output back out.
#[derive(Default)]
pub(crate) struct World {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) artifacts: Vec<PathBuf>,
    pub(crate) outputs: Vec<Output>,
    /// Named outputs, for scenarios that need to keep several distinctly
    /// labeled results straight (e.g. one per CLI verb) rather than just a
    /// "most recent" stack.
    pub(crate) labeled: Vec<(String, Output)>,
    /// Scratch string storage for steps that need to hand a value (e.g. an
    /// expected-output string parsed out of a Given) to a later step.
    pub(crate) notes: Vec<String>,
    /// MagicLisp source snippets a Given step wants a shared, generically-
    /// worded When step (e.g. "each is evaluated") to run — several
    /// scenarios reuse the exact same When wording with different Givens,
    /// so the When step can't hardcode what to run; it runs whatever's
    /// queued here instead.
    pub(crate) pending: Vec<String>,
    /// (small-run peak RSS, full-run peak RSS) pairs recorded by a flat-
    /// memory When step (B6 E1/E2), read back by the following "peak
    /// memory usage stays flat" Then/And step.
    pub(crate) rss_pairs: Vec<(u64, u64)>,
    /// Full CLI invocations (e.g. `["eval", "/tmp/x.ml"]`, `["run",
    /// "/tmp/x.mlbc"]`, `["frobnicate"]`) a Given step queues for a
    /// shared, generically-worded When step (B18's "each is run", reused
    /// verbatim across several scenarios whose Givens span both malformed
    /// SOURCE files and corrupted ARTIFACT files) — unlike `pending`
    /// (always a bare MagicLisp snippet to `display`-wrap and `eval`),
    /// each entry here is a complete, self-contained argv, since a single
    /// shared step can't otherwise tell a `run`/`disasm` case from an
    /// `eval` one.
    pub(crate) pending_commands: Vec<Vec<String>>,
}

impl World {
    pub(crate) fn last_output(&self) -> &Output {
        self.outputs
            .last()
            .expect("a When step should have run something before this assertion")
    }

    pub(crate) fn last_file(&self) -> &PathBuf {
        self.files
            .last()
            .expect("a Given step should have created a source file before this")
    }

    pub(crate) fn last_artifact(&self) -> &PathBuf {
        self.artifacts
            .last()
            .expect("a prior step should have produced a compiled artifact before this")
    }

    pub(crate) fn labeled(&self, name: &str) -> &Output {
        self.labeled
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| o)
            .unwrap_or_else(|| panic!("no output was recorded under the label {name:?}"))
    }
}

/// Un-escapes the small set of backslash escapes the feature files use
/// inside inline double-quoted step arguments (mirroring the reader's own
/// string-escape support), since Gherkin step text itself carries no
/// escaping convention of its own.
pub(crate) fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Extracts the content of the first `"..."`-quoted segment in `text`,
/// unescaping it the same way the reader would. Used by steps whose
/// wording embeds a literal MagicLisp snippet or expected-output string.
pub(crate) fn first_quoted(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let rest = &text[start + 1..];
    let mut end = None;
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
            continue;
        }
        if c == '"' {
            end = Some(i);
            break;
        }
    }
    let end = end?;
    Some(unescape(&rest[..end]))
}

/// Like [`first_quoted`], but returns every `"..."`-quoted segment in
/// order — for steps that embed more than one literal (e.g. two source
/// snippets in the same Given).
pub(crate) fn all_quoted(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(q) = first_quoted(rest) {
        out.push(q.clone());
        let Some(start) = rest.find('"') else { break };
        let after_open = &rest[start + 1..];
        let Some(relative_end) = find_unescaped_quote(after_open) else {
            break;
        };
        rest = &after_open[relative_end + 1..];
    }
    out
}

fn find_unescaped_quote(s: &str) -> Option<usize> {
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
            continue;
        }
        if c == '"' {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_handles_the_readers_escape_set() {
        assert_eq!(unescape(r#"a\nb\tc\rd\"e\\f"#), "a\nb\tc\rd\"e\\f");
    }

    #[test]
    fn first_quoted_extracts_a_single_literal() {
        assert_eq!(
            first_quoted(r#"Given "(display 1)" is evaluated"#),
            Some("(display 1)".to_string())
        );
    }

    #[test]
    fn all_quoted_extracts_every_literal_in_order() {
        assert_eq!(
            all_quoted(r#"the expressions "(a)" and "(b)""#),
            vec!["(a)".to_string(), "(b)".to_string()]
        );
    }
}
