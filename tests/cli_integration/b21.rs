//! B21: non-functional floors -- performance and memory.
//!
//! SPEC.md 10.1/10.2's floors are explicitly stated for an OPTIMISED
//! RELEASE build, so the timing/memory-shape assertions below only apply
//! when `!cfg!(debug_assertions)` — confirmed empirically that an ordinary
//! unoptimized debug build blows well past them for reasons unrelated to
//! any real regression (the E1 loop alone: ~2.5s released vs ~28s debug).
//! Correctness (the displayed value) is still checked unconditionally in
//! both profiles.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::helpers::{run, stdout_of, temp_path, write_source};

const SELF_TAIL_LOOP: &str =
    "(define (loop i limit) (if (= i limit) i (loop (+ i 1) limit))) (display (loop 0 10000000))";
const NAIVE_FIB_27: &str =
    "(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (display (fib 27))";
const COUNTER_FACTORY_SOAK: &str = "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
     (define (soak i) ((counter)) (if (= i 0) 0 (soak (- i 1)))) \
     (soak 999999999999)";

/// SPEC.md 10.1's generous performance floors, meant only to catch a
/// pathologically slow implementation, not to reward raw speed.
const TEN_MILLION_LOOP_CEILING: Duration = Duration::from_secs(10);
const FIB_27_CEILING: Duration = Duration::from_secs(20);
const TWO_THOUSAND_LINE_COMPILE_CEILING: Duration = Duration::from_secs(5);

fn assert_within_release_ceiling(elapsed: Duration, ceiling: Duration, label: &str) {
    if cfg!(debug_assertions) {
        return;
    }
    assert!(
        elapsed <= ceiling,
        "{label} took {elapsed:?}, exceeding the {ceiling:?} release-build ceiling"
    );
}

fn two_thousand_line_source() -> String {
    let mut src = String::new();
    for i in 0..2000 {
        src.push_str(&format!("(define (f{i} x) (+ x {i}))\n"));
    }
    src.push_str("(display (f1999 1))");
    src
}

#[test]
fn e1_a_ten_million_iteration_tail_loop_completes_within_the_release_ceiling() {
    let file = write_source("b21-e1.ml", SELF_TAIL_LOOP);
    let start = Instant::now();
    let out = run(&["eval", file.to_str().unwrap()]);
    let elapsed = start.elapsed();
    assert_eq!(stdout_of(&out), "10000000");
    assert_within_release_ceiling(
        elapsed,
        TEN_MILLION_LOOP_CEILING,
        "the ten-million-iteration tail loop",
    );
}

#[test]
fn e2_naive_fibonacci_of_27_is_correct_and_completes_within_the_release_ceiling() {
    let file = write_source("b21-e2.ml", NAIVE_FIB_27);
    let start = Instant::now();
    let out = run(&["eval", file.to_str().unwrap()]);
    let elapsed = start.elapsed();
    assert_eq!(stdout_of(&out), "196418");
    assert_within_release_ceiling(elapsed, FIB_27_CEILING, "the naive fib(27) computation");
}

#[test]
fn e3_compiling_a_genuine_two_thousand_line_source_file_completes_within_the_release_ceiling() {
    let file = write_source("b21-e3.ml", &two_thousand_line_source());
    let artifact = temp_path("b21-e3.mlbc");
    let start = Instant::now();
    let out = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    let elapsed = start.elapsed();
    assert!(out.status.success());
    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(stdout_of(&run_out), "2000");
    assert_within_release_ceiling(
        elapsed,
        TWO_THOUSAND_LINE_COMPILE_CEILING,
        "compiling the ~2000-line source file",
    );
}

#[test]
fn e4_the_ten_million_iteration_tail_loop_uses_constant_call_frame_memory_on_the_release_build() {
    if cfg!(debug_assertions) {
        return;
    }
    let small = write_source(
        "b21-e4-small.ml",
        "(define (loop i limit) (if (= i limit) i (loop (+ i 1) limit))) (display (loop 0 1000))",
    );
    let large = write_source("b21-e4-large.ml", SELF_TAIL_LOOP);

    let (small_out, small_rss) =
        super::helpers::run_with_peak_rss(&["eval", small.to_str().unwrap()]);
    let (large_out, large_rss) =
        super::helpers::run_with_peak_rss(&["eval", large.to_str().unwrap()]);

    assert_eq!(stdout_of(&small_out), "1000");
    assert_eq!(stdout_of(&large_out), "10000000");

    // Ten thousand times more iterations must not translate into
    // meaningfully more peak memory -- a generous absolute allowance (a
    // real per-iteration frame leak at ten million iterations would dwarf
    // this many times over), not a tight bound tuned to today's exact
    // numbers.
    let growth = large_rss.saturating_sub(small_rss);
    assert!(
        growth < 10 * 1024 * 1024,
        "peak RSS grew by {growth} bytes between 1,000 and 10,000,000 iterations \
         (small={small_rss}, large={large_rss}) -- call-frame memory should be constant"
    );
}

/// Spawns the counter-factory soak program, samples its resident memory at
/// roughly 1s (post-startup warmup), 15s, 30s, 45s, and 60s, then kills it
/// -- the program's own iteration count is astronomically large so it
/// never finishes on its own; only the live sampling window matters.
fn sample_soak_rss_over_a_minute() -> Vec<u64> {
    let file = write_source("b21-soak.ml", COUNTER_FACTORY_SOAK);
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .args(["eval", file.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("binary should spawn");
    let pid = child.id();

    std::thread::sleep(Duration::from_secs(1));
    let mut samples = Vec::new();
    for _ in 0..5 {
        samples.push(sample_rss_kb(pid));
        std::thread::sleep(Duration::from_secs(14));
    }

    let _ = child.kill();
    let _ = child.wait();
    samples
}

fn sample_rss_kb(pid: u32) -> u64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("ps should run");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("could not parse ps rss output: {:?}", out.stdout))
}

/// Asserts a series of RSS samples plateaus rather than growing without
/// bound: the growth in the run's second half must not exceed the growth
/// in its first half by more than a generous slack, AND must itself stay
/// under a generous absolute cap -- together these catch both an
/// accelerating leak and a slow-but-steady one, while tolerating the
/// allocator/OS noise a single-digit-KB comparison would not.
fn assert_plateaus(samples: &[u64]) {
    assert_eq!(samples.len(), 5, "expected 5 RSS samples, got {samples:?}");
    let first_half_growth = samples[2].saturating_sub(samples[0]) as i64;
    let second_half_growth = samples[4].saturating_sub(samples[2]) as i64;
    const GENEROUS_SLACK_KB: i64 = 20_000;
    assert!(
        second_half_growth <= first_half_growth + GENEROUS_SLACK_KB,
        "memory grew faster in the second half of the run ({second_half_growth} KB) than the \
         first ({first_half_growth} KB) -- samples: {samples:?}"
    );
    assert!(
        second_half_growth <= GENEROUS_SLACK_KB,
        "memory grew by {second_half_growth} KB in the run's second half -- samples: {samples:?}"
    );
}

#[test]
#[ignore = "unconditionally costs ~60s+ on the release build (qa test-design warning: the \
            same unconditional-slow-test lesson B20 just taught, one commit later), and is \
            redundant with the BDD acceptance suite's own always-on B21 scenario (steps_b21.rs), \
            which the Examiner explicitly asked NOT to shorten there. Invoke explicitly \
            (`cargo test --release --test cli_integration -- --ignored b21::e5`) for a \
            standalone CLI-integration-level re-check."]
fn e5_repeatedly_creating_closures_over_a_captured_variable_over_a_sustained_minute_stays_bounded()
{
    if cfg!(debug_assertions) {
        return;
    }
    let samples = sample_soak_rss_over_a_minute();
    assert_plateaus(&samples);
}

#[test]
#[ignore = "unconditionally costs ~60s+ on the release build for the same reason as e5 above \
            (qa test-design warning), and is redundant with the BDD acceptance suite's own \
            always-on B21 integration scenario. Invoke explicitly (`cargo test --release \
            --test cli_integration -- --ignored b21::e6`) for a standalone re-check."]
fn e6_all_four_performance_and_memory_demos_hold_together_on_the_release_build() {
    if cfg!(debug_assertions) {
        return;
    }

    let loop_file = write_source("b21-e6-loop.ml", SELF_TAIL_LOOP);
    let loop_start = Instant::now();
    let loop_out = run(&["eval", loop_file.to_str().unwrap()]);
    assert_eq!(stdout_of(&loop_out), "10000000");
    assert_within_release_ceiling(
        loop_start.elapsed(),
        TEN_MILLION_LOOP_CEILING,
        "the ten-million-iteration tail loop",
    );

    let fib_file = write_source("b21-e6-fib.ml", NAIVE_FIB_27);
    let fib_start = Instant::now();
    let fib_out = run(&["eval", fib_file.to_str().unwrap()]);
    assert_eq!(stdout_of(&fib_out), "196418");
    assert_within_release_ceiling(
        fib_start.elapsed(),
        FIB_27_CEILING,
        "the naive fib(27) computation",
    );

    let compile_file = write_source("b21-e6-compile.ml", &two_thousand_line_source());
    let artifact = temp_path("b21-e6-compile.mlbc");
    let compile_start = Instant::now();
    let compile_out = run(&[
        "compile",
        compile_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert!(compile_out.status.success());
    assert_within_release_ceiling(
        compile_start.elapsed(),
        TWO_THOUSAND_LINE_COMPILE_CEILING,
        "compiling the ~2000-line source file",
    );

    let samples = sample_soak_rss_over_a_minute();
    assert_plateaus(&samples);
}
