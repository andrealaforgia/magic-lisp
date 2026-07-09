//! Regression tests for internal robustness properties that aren't tied to
//! any single behaviour slice (B1-B13) -- these exercise the compiled
//! binary's resilience to constrained environments, not a Scheme-visible
//! feature.

use std::os::unix::process::ExitStatusExt as _;
use std::process::Command;

#[test]
fn compiling_a_hand_built_deeply_nested_quasiquote_list_does_not_crash_on_a_severely_constrained_calling_thread(
) {
    // Regression test for qa test-design WARNING msg #245: the prior unit
    // test for this fix (`compile_program_does_not_depend_on_the_calling_
    // threads_own_stack_size` in src/compiler.rs) passed identically
    // whether the fix was present or reverted under `cargo test --release`
    // -- the profile this whole review process actually tests in -- and
    // even where a difference existed (debug builds only), a real stack
    // overflow aborts the process outright, which `.join()` on a spawned
    // thread structurally cannot observe as a clean, targeted assertion
    // failure (Rust aborts rather than unwinds for a stack overflow).
    //
    // This test sidesteps both problems by observing the fix's effect
    // from OUTSIDE the process: `quasiquote_list_stack_probe` (a small
    // test-support binary, src/bin/quasiquote_list_stack_probe.rs) builds
    // a hand-built AST -- deeply nested plain lists inside one
    // `quasiquote`, bypassing the reader's own depth cap entirely, the
    // same shape `nested_quasiquoted_list` builds in the compiler's own
    // tests -- and calls the real, compiled `compile_program` directly.
    // Run under `sh -c "ulimit -s ... && exec ..."` (an OS-level rlimit on
    // the process's own main-thread stack, no `unsafe` needed), a crash
    // shows up as the process dying by signal, observable cleanly from
    // this driving test without taking down the test harness itself --
    // exactly the pattern this codebase already uses for stack/stall
    // regressions in `tests/cli_integration/b12.rs`.
    //
    // 192 KiB and a nesting depth of 20,000 were determined empirically
    // (hand-verified, not guessed): reverting the dedicated-stack fix and
    // sweeping both the calling-thread stack size and the hand-built
    // tree's depth found this exact combination reliably crashes the
    // reverted code by signal in `--release` -- while comfortably above
    // the compiler's own `MAX_NESTING_DEPTH` guard (512), confirming the
    // crash is a genuine native stack overflow during the bounded
    // recursion up to that guard, not the guard failing to fire at all.
    let probe = env!("CARGO_BIN_EXE_quasiquote_list_stack_probe");
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("ulimit -s 192 && exec '{probe}' 20000"))
        .output()
        .expect("sh should run");

    assert_eq!(
        output.status.signal(),
        None,
        "the probe process was killed by signal {:?} (stderr: {}) -- compile_program \
         inherited the severely constrained calling thread's stack instead of running \
         on its own dedicated, generously-sized one",
        output.status.signal(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "expected a clean exit, got: {:?} (stderr: {})",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    // Exit status alone can't distinguish "compile_program genuinely ran
    // to completion" from "the probe never called it at all" -- both look
    // like a clean, unsignaled exit. The probe prints its outcome
    // specifically so this test can tell the two apart; a depth of 20,000
    // is comfortably past `MAX_NESTING_DEPTH` (512), so a completed run
    // must report the nesting-depth error, not a bare success.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("compiled err") && stdout.contains("nesting"),
        "expected the probe to report compile_program's own nesting-depth \
         error, got stdout: {stdout}"
    );
}
