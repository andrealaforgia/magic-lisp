//! Step definitions for features/B20-self-test-and-quality-gates.feature.
//!
//! Unlike every earlier B-slice, this one's subject is the project's own
//! tooling (`cargo test`, `cargo fmt`, `cargo clippy`, `cargo build`), not
//! the `magiclisp` binary, so these steps shell out to `cargo`/`rustc`
//! directly rather than through `world::run`.

use magiclisp::exitcode::SUCCESS;

use super::registry::Registry;
use super::world::{run, temp_path, write_source};

/// The eleven specific tests E1/E6 require to exist and pass, one per
/// required coverage category (reader comment handling, reader dotted-pair
/// reading, a real file-backed bytecode round trip, closures sharing a
/// captured variable, tail-call recursion at real depth, and one test per
/// established exit code) -- confirmed present in the source tree, not
/// merely asserted to exist.
const NAMED_TESTS: &[&str] = &[
    "reader::tests::skips_a_single_block_comment",
    "reader::tests::a_block_comment_fully_containing_another_is_consumed_as_one_outer_comment",
    "reader::tests::reads_a_dotted_pair_with_a_single_fixed_head_item",
    "b1::e2_compile_then_run_reproduces_eval_output_across_process_boundaries",
    "b5::b5_e2_mutating_a_captured_variable_through_one_closure_is_visible_through_another",
    "b6::b6_e1_self_tail_call_loop_counts_to_ten_million",
    "b1::e8_success_exit_code_for_a_valid_program",
    "b1::e8_usage_error_exit_code_for_a_missing_required_argument",
    "b1::e8_source_error_exit_code_for_unreadable_source",
    "b1::e8_bad_artifact_exit_code_for_a_corrupt_artifact",
    "b1::e8_runtime_error_exit_code_for_an_undefined_global",
];

/// A target directory dedicated to the `cargo build`/`test`/`clippy`
/// child processes this file spawns, distinct from the target directory
/// the outer `cargo test` run (and every sibling test binary running
/// concurrently with this one, e.g. via `CARGO_BIN_EXE_magiclisp`) is
/// using. Without this, a nested build/rebuild here can transiently
/// replace or relink files under the shared target directory while a
/// sibling test elsewhere in the same run is trying to spawn the already-
/// built `magiclisp` binary out of it, causing spurious unrelated
/// failures from a filesystem race, not a real bug.
fn isolated_target_dir() -> &'static std::path::Path {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        std::env::temp_dir().join(format!("magiclisp-b20-target-{}", std::process::id()))
    })
}

fn run_bin(bin: &str, args: &[&str]) -> std::process::Output {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{bin} {args:?} should run: {e}"))
}

/// Removes the isolated target directory after a nested cargo invocation
/// is done with it (qa test-design warning: left uncleaned, every run
/// leaked a fresh multi-hundred-MB directory that was never reclaimed).
/// Cargo recreates it from scratch on the next call if one is needed --
/// a negligible rebuild cost for this dependency-free crate compared to
/// the test execution time these invocations are dominated by.
fn cleanup_isolated_target_dir() {
    let _ = std::fs::remove_dir_all(isolated_target_dir());
}

fn run_cargo(args: &[&str]) -> std::process::Output {
    let output = std::process::Command::new("cargo")
        .args(args)
        .env("CARGO_TARGET_DIR", isolated_target_dir())
        .output()
        .unwrap_or_else(|e| panic!("cargo {args:?} should run: {e}"));
    cleanup_isolated_target_dir();
    output
}

/// This test binary is itself one of the things the documented test
/// command (`cargo test --all`) runs, but this scenario itself is
/// `#[ignore]`d (see `tests/features.rs`) and the child invocation below
/// is always a plain `cargo test --all` with no `--ignored`/
/// `--include-ignored` flag, so the child selects only non-ignored
/// tests -- it never reaches this same scenario again, regardless of how
/// the outer invocation that reached this point was itself launched. No
/// separate recursion guard is needed on top of that.
fn run_documented_test_command() -> std::process::Output {
    let output = std::process::Command::new("cargo")
        .args(["test", "--all"])
        .env("CARGO_TARGET_DIR", isolated_target_dir())
        .output()
        .expect("cargo test --all should run");
    cleanup_isolated_target_dir();
    output
}

fn assert_test_command_ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "cargo test --all did not succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for name in NAMED_TESTS {
        assert!(stdout.contains(name), "missing test {name} in: {stdout}");
    }
}

fn assert_fmt_ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "cargo fmt --check reported differences: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(out.stdout.is_empty(), "expected no diff output");
}

fn assert_clippy_ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "cargo clippy did not succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("warning:"),
        "unexpected warning: {combined}"
    );
}

fn assert_determinism_ok(notes: &[String]) {
    assert!(
        notes.iter().any(|n| n == "determinism_macro_free:true"),
        "macro-free compile was not deterministic: {notes:?}"
    );
    assert!(
        notes.iter().any(|n| n == "determinism_macro_using:true"),
        "macro-using compile was not deterministic: {notes:?}"
    );
}

/// Compiles a macro-free program and a macro-using program (using `gensym`
/// during expansion) twice each, under `label_prefix` to keep temp paths
/// from different scenarios apart, and reports whether each pair of
/// resulting artifacts is byte-for-byte identical.
fn double_compile_is_deterministic(label_prefix: &str) -> (bool, bool) {
    let macro_free_src = "(display (+ 1 2)) (newline)";
    let macro_using_src = "(define-macro (swap! a b) (let ((tmp (gensym))) `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp))))\n\
                           (define x 1) (define y 2) (swap! x y) (write (list x y)) (newline)";
    let free_file = write_source(&format!("{label_prefix}-free.ml"), macro_free_src);
    let using_file = write_source(&format!("{label_prefix}-using.ml"), macro_using_src);

    let mut artifacts = Vec::new();
    for (label, file) in [("free", &free_file), ("using", &using_file)] {
        for n in 1..=2 {
            let artifact = temp_path(&format!("{label_prefix}-{label}-{n}.mlbc"));
            let out = run(&[
                "compile",
                file.to_str().unwrap(),
                "-o",
                artifact.to_str().unwrap(),
            ]);
            assert_eq!(out.status.code(), Some(SUCCESS), "compile should succeed");
            artifacts.push(artifact);
        }
    }
    let free_identical =
        std::fs::read(&artifacts[0]).unwrap() == std::fs::read(&artifacts[1]).unwrap();
    let using_identical =
        std::fs::read(&artifacts[2]).unwrap() == std::fs::read(&artifacts[3]).unwrap();
    (free_identical, using_identical)
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1/E2/E3/E6 share a "queue what to check, then run it" shape. ---
        .step("the project's documented test command", |w, _text, _| {
            w.notes.push("test".to_string());
        })
        .step("the project's formatting check", |w, _text, _| {
            w.notes.push("fmt".to_string());
        })
        .step(
            "the project's linter run across all targets and features",
            |w, _text, _| {
                w.notes.push("clippy".to_string());
            },
        )
        .step(
            "the single documented test command, the formatting check, the linter, and the double-compile determinism check",
            |w, _text, _| {
                w.notes.extend([
                    "test".to_string(),
                    "fmt".to_string(),
                    "clippy".to_string(),
                    "determinism".to_string(),
                ]);
            },
        )
        .step("it is run", |w, _text, _| {
            let kinds = std::mem::take(&mut w.notes);
            for kind in &kinds {
                run_queued_check(w, kind);
            }
        })
        .step("each is run", |w, _text, _| {
            let kinds = std::mem::take(&mut w.notes);
            for kind in &kinds {
                run_queued_check(w, kind);
            }
        })
        .step(
            "it completes successfully, and specific named tests exist covering the reader (comment handling, dotted-pair reading), a real bytecode round trip through a file written to and read back from disk, closures sharing a captured variable, tail-call recursion reaching real depth without growing memory, and one example of each of the five established exit-code outcomes",
            |w, _text, _| {
                assert_test_command_ok(w.labeled("test"));
            },
        )
        .step("it passes with no differences", |w, _text, _| {
            assert_fmt_ok(w.labeled("fmt"));
        })
        .step("it reports no warnings", |w, _text, _| {
            assert_clippy_ok(w.labeled("clippy"));
        })
        .step(
            "the test command completes successfully with all five required categories present, the formatting check passes with no differences, the linter reports no warnings, and a sample program compiled twice yields byte-identical output",
            |w, _text, _| {
                assert_test_command_ok(w.labeled("test"));
                assert_fmt_ok(w.labeled("fmt"));
                assert_clippy_ok(w.labeled("clippy"));
                assert_determinism_ok(&w.notes);
            },
        )
        // --- E4: toolchain, dependency manifest, and unsafe-code inspection. ---
        .step(
            "the stable Rust toolchain, the project's dependency manifest, and the source tree",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("each is inspected", |w, _text, _| {
            let rustc_out = run_bin("rustc", &["--version"]);
            let rustc_stable = !String::from_utf8_lossy(&rustc_out.stdout).contains("nightly");
            w.notes.push(format!("rustc_stable:{rustc_stable}"));

            let build_out = run_cargo(&["build", "--release"]);
            w.notes
                .push(format!("build_ok:{}", build_out.status.success()));

            let tree_out = run_cargo(&["tree"]);
            let tree_stdout = String::from_utf8_lossy(&tree_out.stdout);
            let no_deps = tree_out.status.success() && tree_stdout.lines().count() <= 1;
            w.notes.push(format!("no_deps:{no_deps}"));

            let lib_src = std::fs::read_to_string("src/lib.rs").expect("src/lib.rs should exist");
            let main_src =
                std::fs::read_to_string("src/main.rs").expect("src/main.rs should exist");
            let unsafe_forbidden = lib_src.contains("#![forbid(unsafe_code)]")
                && main_src.contains("#![forbid(unsafe_code)]");
            w.notes.push(format!("unsafe_forbidden:{unsafe_forbidden}"));
        })
        .step(
            "the build succeeds on stable, no runtime dependencies beyond the standard library are declared, and unsafe code is forbidden at compile time in both crate roots",
            |w, _text, _| {
                assert!(
                    w.notes.iter().any(|n| n == "rustc_stable:true"),
                    "not a stable toolchain: {:?}",
                    w.notes
                );
                assert!(
                    w.notes.iter().any(|n| n == "build_ok:true"),
                    "release build failed: {:?}",
                    w.notes
                );
                assert!(
                    w.notes.iter().any(|n| n == "no_deps:true"),
                    "found runtime dependencies beyond the standard library: {:?}",
                    w.notes
                );
                assert!(
                    w.notes.iter().any(|n| n == "unsafe_forbidden:true"),
                    "unsafe code is not forbidden in both crate roots: {:?}",
                    w.notes
                );
            },
        )
        // --- E5: double-compile determinism, macro-free and macro-using. ---
        .step(
            "a macro-free source file and a macro-using source file (using gensym during expansion), each compiled twice",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("the two resulting artifacts are compared", |w, _text, _| {
            let (free_identical, using_identical) = double_compile_is_deterministic("b20-e5");
            w.notes
                .push(format!("determinism_macro_free:{free_identical}"));
            w.notes
                .push(format!("determinism_macro_using:{using_identical}"));
        })
        .step("both pairs are byte-for-byte identical", |w, _text, _| {
            assert_determinism_ok(&w.notes);
        })
}

fn run_queued_check(w: &mut super::world::World, kind: &str) {
    match kind {
        "test" => {
            let out = run_documented_test_command();
            w.labeled.push(("test".to_string(), out));
        }
        "fmt" => {
            let out = run_cargo(&["fmt", "--check"]);
            w.labeled.push(("fmt".to_string(), out));
        }
        "clippy" => {
            let out = run_cargo(&["clippy", "--all-targets", "--all-features"]);
            w.labeled.push(("clippy".to_string(), out));
        }
        "determinism" => {
            let (free_identical, using_identical) = double_compile_is_deterministic("b20-e6");
            w.notes
                .push(format!("determinism_macro_free:{free_identical}"));
            w.notes
                .push(format!("determinism_macro_using:{using_identical}"));
        }
        other => panic!("unknown B20 check kind queued: {other}"),
    }
}
