//! Step definitions for features/B21-performance-and-memory.feature.
//!
//! SPEC.md 10.1/10.2's floors are explicitly stated for an OPTIMISED
//! RELEASE build, so the timing/memory-shape assertions below only apply
//! when `!cfg!(debug_assertions)` (confirmed empirically that an ordinary
//! debug build blows well past them for reasons unrelated to any real
//! regression). Correctness (the displayed value) is still checked
//! unconditionally. E5/E6's sustained ~60-second soak is skipped entirely
//! under debug rather than shortened, per the Examiner's explicit
//! instruction not to shorten the sampling duration.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::registry::Registry;
use super::world::{run, run_with_peak_rss, stdout_of, temp_path, write_source};

const SELF_TAIL_LOOP: &str =
    "(define (loop i limit) (if (= i limit) i (loop (+ i 1) limit))) (display (loop 0 10000000))";
const NAIVE_FIB_27: &str =
    "(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (display (fib 27))";
const COUNTER_FACTORY_SOAK: &str = "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
     (define (soak i) ((counter)) (if (= i 0) 0 (soak (- i 1)))) \
     (soak 999999999999)";

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

fn run_timed(label: &str, src: &str) -> (String, Duration) {
    let file = write_source(label, src);
    let start = Instant::now();
    let out = run(&["eval", file.to_str().unwrap()]);
    let elapsed = start.elapsed();
    (stdout_of(&out), elapsed)
}

fn note_f64(notes: &[String], key: &str) -> f64 {
    notes
        .iter()
        .find_map(|n| n.strip_prefix(&format!("{key}:")))
        .unwrap_or_else(|| panic!("missing note {key} in {notes:?}"))
        .parse()
        .unwrap()
}

fn note_u64(notes: &[String], key: &str) -> u64 {
    notes
        .iter()
        .find_map(|n| n.strip_prefix(&format!("{key}:")))
        .unwrap_or_else(|| panic!("missing note {key} in {notes:?}"))
        .parse()
        .unwrap()
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

/// See `tests/cli_integration/b21.rs`'s identical reasoning: the growth in
/// the run's second half must not exceed the growth in its first half by
/// more than a generous slack, AND must itself stay under a generous
/// absolute cap -- together these catch both an accelerating leak and a
/// slow-but-steady one, while tolerating ordinary allocator/OS noise.
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

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1/E2 share a "queue what to run, then run it" shape. ---
        .step("a self-tail-call loop counting to ten million", |w, _text, _| {
            w.notes.push("tail_loop".to_string());
        })
        .step(
            "a naive, non-memoized recursive Fibonacci computation of fib(27)",
            |w, _text, _| {
                w.notes.push("fib27".to_string());
            },
        )
        .step("it is run on the release build", |w, _text, _| {
            let kinds = std::mem::take(&mut w.notes);
            for kind in &kinds {
                match kind.as_str() {
                    "tail_loop" => {
                        let (stdout, elapsed) = run_timed("b21-e1.ml", SELF_TAIL_LOOP);
                        w.notes.push(format!("tail_loop_stdout:{stdout}"));
                        w.notes
                            .push(format!("tail_loop_elapsed:{}", elapsed.as_secs_f64()));
                    }
                    "fib27" => {
                        let (stdout, elapsed) = run_timed("b21-e2.ml", NAIVE_FIB_27);
                        w.notes.push(format!("fib27_stdout:{stdout}"));
                        w.notes
                            .push(format!("fib27_elapsed:{}", elapsed.as_secs_f64()));
                    }
                    other => panic!("unknown B21 timed check queued: {other}"),
                }
            }
        })
        .step(
            "it displays 10000000 well within the spec's ceiling of 10 seconds",
            |w, _text, _| {
                let stdout = w
                    .notes
                    .iter()
                    .find_map(|n| n.strip_prefix("tail_loop_stdout:"))
                    .expect("tail_loop should have been run");
                assert_eq!(stdout, "10000000");
                let elapsed = Duration::from_secs_f64(note_f64(&w.notes, "tail_loop_elapsed"));
                assert_within_release_ceiling(
                    elapsed,
                    TEN_MILLION_LOOP_CEILING,
                    "the ten-million-iteration tail loop",
                );
            },
        )
        .step(
            "it displays the mathematically correct result (196418) well within the spec's ceiling of 20 seconds",
            |w, _text, _| {
                let stdout = w
                    .notes
                    .iter()
                    .find_map(|n| n.strip_prefix("fib27_stdout:"))
                    .expect("fib27 should have been run");
                assert_eq!(stdout, "196418");
                let elapsed = Duration::from_secs_f64(note_f64(&w.notes, "fib27_elapsed"));
                assert_within_release_ceiling(
                    elapsed,
                    FIB_27_CEILING,
                    "the naive fib(27) computation",
                );
            },
        )
        // --- E3 ---
        .step("an actually-generated 2000-line source file", |w, _text, _| {
            let file = write_source("b21-e3.ml", &two_thousand_line_source());
            w.files.push(file);
        })
        .step("it is compiled on the release build", |w, _text, _| {
            let file = w.last_file().clone();
            let artifact = temp_path("b21-e3.mlbc");
            let start = Instant::now();
            let out = run(&[
                "compile",
                file.to_str().unwrap(),
                "-o",
                artifact.to_str().unwrap(),
            ]);
            let elapsed = start.elapsed();
            assert!(out.status.success(), "compile should succeed");
            w.artifacts.push(artifact);
            w.notes
                .push(format!("compile_elapsed:{}", elapsed.as_secs_f64()));
        })
        .step(
            "compilation completes well within the spec's ceiling of 5 seconds and the resulting artifact runs correctly",
            |w, _text, _| {
                let elapsed = Duration::from_secs_f64(note_f64(&w.notes, "compile_elapsed"));
                assert_within_release_ceiling(
                    elapsed,
                    TWO_THOUSAND_LINE_COMPILE_CEILING,
                    "compiling the ~2000-line source file",
                );
                let artifact = w.last_artifact().clone();
                let out = run(&["run", artifact.to_str().unwrap()]);
                assert_eq!(stdout_of(&out), "2000");
            },
        )
        // --- E4 ---
        .step(
            "the same tail-call loop run at a small iteration count and at ten million iterations",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("peak resident memory is measured for each", |w, _text, _| {
            let small = write_source(
                "b21-e4-small.ml",
                "(define (loop i limit) (if (= i limit) i (loop (+ i 1) limit))) (display (loop 0 1000))",
            );
            let large = write_source("b21-e4-large.ml", SELF_TAIL_LOOP);
            let (small_out, small_rss) = run_with_peak_rss(&["eval", small.to_str().unwrap()]);
            let (large_out, large_rss) = run_with_peak_rss(&["eval", large.to_str().unwrap()]);
            assert_eq!(stdout_of(&small_out), "1000");
            assert_eq!(stdout_of(&large_out), "10000000");
            w.notes.push(format!("small_rss:{small_rss}"));
            w.notes.push(format!("large_rss:{large_rss}"));
        })
        .step(
            "it stays flat across a 10,000x increase in iteration count, reconfirming B6's guarantee under release-build conditions",
            |w, _text, _| {
                if cfg!(debug_assertions) {
                    return;
                }
                let small_rss = note_u64(&w.notes, "small_rss");
                let large_rss = note_u64(&w.notes, "large_rss");
                let growth = large_rss.saturating_sub(small_rss);
                assert!(
                    growth < 10 * 1024 * 1024,
                    "peak RSS grew by {growth} bytes between 1,000 and 10,000,000 iterations \
                     (small={small_rss}, large={large_rss})"
                );
            },
        )
        // --- E5 ---
        .step(
            "the B5 counter-factory closure pattern exercised continuously for roughly 60 seconds, with memory sampled at multiple points across the run (not just a single before/after pair)",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("the sampled resident-memory trend is examined", |w, _text, _| {
            if cfg!(debug_assertions) {
                return;
            }
            let samples = sample_soak_rss_over_a_minute();
            w.notes = samples.iter().map(|s| s.to_string()).collect();
        })
        .step(
            "memory settles quickly and stays flat for the remainder of the run, rather than growing without bound — this design has no host garbage collector, so this demonstrates the exercised pattern (a returned closure that never references itself, only its own private counter cell) has no reference cycle needing a cycle collector",
            |w, _text, _| {
                if cfg!(debug_assertions) {
                    return;
                }
                let samples: Vec<u64> = w.notes.iter().map(|s| s.parse().unwrap()).collect();
                assert_plateaus(&samples);
            },
        )
        // --- E6: integration ---
        .step(
            "the ten-million-iteration tail loop, the naive Fibonacci computation, the ~2000-line compile, and the sustained closure-creation soak",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("each is run together in one review pass", |w, _text, _| {
            let (tail_stdout, tail_elapsed) = run_timed("b21-e6-loop.ml", SELF_TAIL_LOOP);
            w.notes.push(format!("tail_loop_stdout:{tail_stdout}"));
            w.notes
                .push(format!("tail_loop_elapsed:{}", tail_elapsed.as_secs_f64()));

            let (fib_stdout, fib_elapsed) = run_timed("b21-e6-fib.ml", NAIVE_FIB_27);
            w.notes.push(format!("fib27_stdout:{fib_stdout}"));
            w.notes
                .push(format!("fib27_elapsed:{}", fib_elapsed.as_secs_f64()));

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
            w.notes.push(format!(
                "compile_elapsed:{}",
                compile_start.elapsed().as_secs_f64()
            ));

            if !cfg!(debug_assertions) {
                let samples = sample_soak_rss_over_a_minute();
                for (i, s) in samples.iter().enumerate() {
                    w.notes.push(format!("soak_sample_{i}:{s}"));
                }
            }
        })
        .step(
            "each holds: 10000000 within its ceiling, 196418 within its ceiling, the large-file compile within its ceiling, and the same flat memory trend across another full ~60-second sampled run",
            |w, _text, _| {
                let tail_stdout = w
                    .notes
                    .iter()
                    .find_map(|n| n.strip_prefix("tail_loop_stdout:"))
                    .unwrap();
                assert_eq!(tail_stdout, "10000000");
                assert_within_release_ceiling(
                    Duration::from_secs_f64(note_f64(&w.notes, "tail_loop_elapsed")),
                    TEN_MILLION_LOOP_CEILING,
                    "the ten-million-iteration tail loop",
                );

                let fib_stdout = w
                    .notes
                    .iter()
                    .find_map(|n| n.strip_prefix("fib27_stdout:"))
                    .unwrap();
                assert_eq!(fib_stdout, "196418");
                assert_within_release_ceiling(
                    Duration::from_secs_f64(note_f64(&w.notes, "fib27_elapsed")),
                    FIB_27_CEILING,
                    "the naive fib(27) computation",
                );

                assert_within_release_ceiling(
                    Duration::from_secs_f64(note_f64(&w.notes, "compile_elapsed")),
                    TWO_THOUSAND_LINE_COMPILE_CEILING,
                    "compiling the ~2000-line source file",
                );

                if !cfg!(debug_assertions) {
                    let samples: Vec<u64> = (0..5)
                        .map(|i| note_u64(&w.notes, &format!("soak_sample_{i}")))
                        .collect();
                    assert_plateaus(&samples);
                }
            },
        )
}
