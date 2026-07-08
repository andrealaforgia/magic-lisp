//! Step definitions for features/B6-tail-and-deep-recursion.feature.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::registry::Registry;
use super::world::{
    eval_ok, run, run_pending, run_with_peak_rss, stderr_of, stdout_of, write_source,
};

/// One level short of / exactly at the implementation's non-tail call-depth
/// limit — used by E4's boundary check. Mirrors src/vm.rs's MAX_CALL_DEPTH;
/// if that constant ever moves, this test (not just the unit tests) will
/// catch the drift since the "one-short" case would start failing too.
const CALL_DEPTH_LIMIT: i64 = 150_000;

fn self_tail_loop_src(limit: u64) -> String {
    format!(
        "(define (loop n limit) (if (= n limit) n (loop (+ n 1) limit))) \
         (display (loop 0 {limit})) (newline)"
    )
}

fn mutual_tail_src(depth: u64) -> String {
    format!(
        "(define (even? n) (if (= n 0) #t (odd? (- n 1)))) \
         (define (odd? n) (if (= n 0) #f (even? (- n 1)))) \
         (display (even? {depth})) (newline)"
    )
}

const NON_TAIL_SUM_DEFINE: &str = "(define (sum n) (if (= n 0) 0 (+ n (sum (- n 1)))))";

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- shared: E1/E2 both queue [small-run source, full-run source]
        // and both use this exact When wording; E3 queues a single source
        // under the same wording. The step tells the two cases apart by
        // how many variants were queued.
        .step("it is run to completion", |w, _text, _| {
            let variants = std::mem::take(&mut w.pending);
            match variants.len() {
                1 => {
                    let out = eval_ok("b6-run-to-completion.ml", &variants[0]);
                    w.notes.push(out.trim_end_matches('\n').to_string());
                }
                2 => {
                    let small_file = write_source("b6-small.ml", &variants[0]);
                    let (small_out, small_rss) =
                        run_with_peak_rss(&["eval", small_file.to_str().unwrap()]);
                    assert!(
                        small_out.status.success(),
                        "small run should succeed, stderr: {}",
                        stderr_of(&small_out)
                    );

                    let full_file = write_source("b6-full.ml", &variants[1]);
                    let (full_out, full_rss) =
                        run_with_peak_rss(&["eval", full_file.to_str().unwrap()]);
                    assert!(
                        full_out.status.success(),
                        "full run should succeed, stderr: {}",
                        stderr_of(&full_out)
                    );

                    w.notes
                        .push(stdout_of(&full_out).trim_end_matches('\n').to_string());
                    w.rss_pairs.push((small_rss, full_rss));
                }
                n => panic!("unexpected number of queued source variants: {n}"),
            }
        })
        // --- E1 ---
        .step(
            "a self-recursive function that calls itself as its very last action, counting from 0 to ten million",
            |w, _text, _| {
                w.pending = vec![self_tail_loop_src(10_000), self_tail_loop_src(10_000_000)];
            },
        )
        .step("it displays \"10000000\"", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "10000000");
        })
        .step(
            "peak memory usage does not scale with iteration count — it stays flat between a small-iteration-count run and the full ten-million-iteration run",
            |w, _text, _| {
                let (small_rss, full_rss) = w
                    .rss_pairs
                    .last()
                    .copied()
                    .expect("a flat-memory When step should have recorded an RSS pair");
                assert!(
                    full_rss < small_rss * 10,
                    "expected flat memory (full within 10x of small), got small={small_rss} full={full_rss} \
                     -- a 1000x iteration-count increase producing anywhere near a 1000x memory increase \
                     would mean tail calls are NOT running in O(1) space"
                );
            },
        )
        // --- E2 ---
        .step(
            "two functions (even?/odd?) that call each other back and forth, each call being the last action, driven to a depth of ten million",
            |w, _text, _| {
                w.pending = vec![mutual_tail_src(10_000), mutual_tail_src(10_000_000)];
            },
        )
        .step("it displays \"#t\"", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "#t");
        })
        .step(
            "peak memory usage stays flat between a small-depth run and the full ten-million-depth run, the same as E1",
            |w, _text, _| {
                let (small_rss, full_rss) = w
                    .rss_pairs
                    .last()
                    .copied()
                    .expect("a flat-memory When step should have recorded an RSS pair");
                assert!(
                    full_rss < small_rss * 10,
                    "expected flat memory (full within 10x of small), got small={small_rss} full={full_rss}"
                );
            },
        )
        // --- E3 ---
        .step(
            "a non-tail recursive sum (each call still has an addition pending after the recursive call returns) from 1 to 100,000",
            |w, _text, _| {
                w.pending = vec![format!(
                    "{NON_TAIL_SUM_DEFINE} (display (sum 100000)) (newline)"
                )];
            },
        )
        .step("it displays \"5000050000\"", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "5000050000");
        })
        // --- E4 ---
        .step(
            "the same non-tail sum driven far past the depth genuine recursion can support",
            |w, _text, _| {
                let file = write_source(
                    "b6-e4-too-deep.ml",
                    &format!("{NON_TAIL_SUM_DEFINE} (display (sum 10000000)) (newline)"),
                );
                w.files.push(file);
            },
        )
        .step("it is run", |w, _text, _| {
            let file = w.last_file().clone();
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step(
            "it fails with a clean, reported runtime error and a distinct exit code, not a crash or hang",
            |w, _text, _| {
                assert_eq!(w.last_output().status.code(), Some(RUNTIME_ERROR));
                assert!(!stderr_of(w.last_output()).is_empty());
            },
        )
        .step(
            "the boundary is exact: one level short of the limit succeeds, the limit itself fails",
            |_w, _text, _| {
                let succeeds = eval_ok(
                    "b6-e4-boundary-succeeds.ml",
                    &format!(
                        "{NON_TAIL_SUM_DEFINE} (display (sum {}))",
                        CALL_DEPTH_LIMIT - 1
                    ),
                );
                assert_eq!(succeeds, "11249925000");

                let file = write_source(
                    "b6-e4-boundary-fails.ml",
                    &format!("{NON_TAIL_SUM_DEFINE} (display (sum {CALL_DEPTH_LIMIT}))"),
                );
                let output = run(&["eval", file.to_str().unwrap()]);
                assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
            },
        )
        // --- E5 ---
        .step(
            "each of the three DEMO programs from the behaviour spec",
            |w, _text, _| {
                w.pending = vec![
                    self_tail_loop_src(10_000_000),
                    mutual_tail_src(10_000_000),
                    format!("{NON_TAIL_SUM_DEFINE} (display (sum 100000)) (newline)"),
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending(w, "b6-e5");
        })
        .step(
            "each produces exactly its prescribed output followed by a trailing newline, and exits 0",
            |w, _text, _| {
                assert_eq!(w.notes[0], "10000000\n");
                assert_eq!(w.notes[1], "#t\n");
                assert_eq!(w.notes[2], "5000050000\n");
            },
        )
}
