//! B22: cycle-safe memory under genuine closure/upvalue reference cycles.
//!
//! E1-E4's own soak/README/interleaved scenarios are proven through the BDD
//! acceptance layer (features/B22-cycle-safe-memory.feature,
//! tests/features/steps_b22.rs) using `eval` directly. This module adds the
//! two checks that layer doesn't cover (Examiner msg #424):
//!
//! - E5: both cyclic shapes still compute the CORRECT final result under
//!   real, repeated automatic collection pressure (finite iteration counts,
//!   well past the sweep threshold), not just "memory stays flat" -- a
//!   collector that clears a still-live cell would corrupt the answer while
//!   leaving memory looking perfectly healthy.
//! - E6: the same guarantee holds through the actual `compile` + `run`
//!   path (a real `.mlbc` artifact executed by the VM), not only through
//!   `eval` -- "a real program run through the normal CLI (compile/run,
//!   not a test-only path)".

use super::helpers::{
    assert_plateaus, run, sample_rss_over_a_minute, stderr_of, stdout_of, temp_path, write_source,
};

/// Well past `EnvGc`'s sweep threshold (512 -- see `src/vm.rs`), so a
/// finite run of this size forces several real automatic sweeps while the
/// cycle it just built is still a live, in-scope local -- not just once at
/// the very end.
const FINITE_ITERATIONS: u64 = 5_000;

fn self_reference_soak(iterations: u64) -> String {
    format!(
        "(define (make-self-ref) (let ((cell #f)) (set! cell (lambda () cell)) 0)) \
         (define (soak i) (make-self-ref) (if (= i 0) 0 (soak (- i 1)))) \
         (display (soak {iterations}))"
    )
}

fn mutual_reference_soak(iterations: u64) -> String {
    format!(
        "(define (make-mutual-pair) \
           (let ((a #f) (b #f)) \
             (set! a (lambda () b)) \
             (set! b (lambda () a)) \
             0)) \
         (define (soak i) (make-mutual-pair) (if (= i 0) 0 (soak (- i 1)))) \
         (display (soak {iterations}))"
    )
}

/// The self-reference and mutual-reference patterns interleaved with B21's
/// acyclic counter-factory pattern every iteration -- the same shape
/// features/B22-cycle-safe-memory.feature's E4 exercises for memory, reused
/// here for both a finite correctness check and a real compile+run soak.
fn interleaved_soak(iterations: u64) -> String {
    format!(
        "(define (self-ref-once) \
           (let ((cell #f)) (set! cell (lambda () cell)) 0)) \
         (define (mutual-ref-once) \
           (let ((a #f) (b #f)) \
             (set! a (lambda () b)) \
             (set! b (lambda () a)) \
             0)) \
         (define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
         (define (acyclic-once) ((counter)) 0) \
         (define (soak i) \
           (self-ref-once) \
           (mutual-ref-once) \
           (acyclic-once) \
           (if (= i 0) 0 (soak (- i 1)))) \
         (display (soak {iterations}))"
    )
}

fn eval_program(label: &str, src: &str) -> std::process::Output {
    let file = write_source(label, src);
    run(&["eval", file.to_str().unwrap()])
}

fn compile_and_run(label: &str, src: &str) -> std::process::Output {
    let src_file = write_source(&format!("{label}.ml"), src);
    let artifact = temp_path(&format!("{label}.mlbc"));
    let compile_out = run(&[
        "compile",
        src_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert!(
        compile_out.status.success(),
        "compile stderr: {}",
        stderr_of(&compile_out)
    );
    run(&["run", artifact.to_str().unwrap()])
}

// --- E5: correctness under repeated real collection, not just flat memory ---

#[test]
fn e5_a_finite_self_referential_soak_still_computes_the_correct_result_under_real_collection() {
    let out = eval_program("b22-e5-self.ml", &self_reference_soak(FINITE_ITERATIONS));
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert_eq!(stdout_of(&out), "0");
}

#[test]
fn e5_a_finite_mutual_reference_soak_still_computes_the_correct_result_under_real_collection() {
    let out = eval_program(
        "b22-e5-mutual.ml",
        &mutual_reference_soak(FINITE_ITERATIONS),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert_eq!(stdout_of(&out), "0");
}

// --- E6: through the real compile+run path, not eval ---

#[test]
fn e6_a_compiled_artifact_run_through_the_normal_cli_computes_the_correct_result_for_both_cyclic_shapes_together()
 {
    let out = compile_and_run("b22-e6-finite", &interleaved_soak(FINITE_ITERATIONS));
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert_eq!(stdout_of(&out), "0");
}

#[test]
#[ignore = "unconditionally costs ~60s+ on the release build, for the same reason as B21's own \
            e5/e6 (qa test-design warning) -- redundant with the BDD acceptance suite's always-on \
            B22/E4 scenario, which exercises the same interleaved churn via `eval`. This test's \
            distinct value is proving the SAME guarantee holds through the real compile+run path \
            instead. Invoke explicitly (`cargo test --release --test cli_integration -- --ignored \
            b22::e6`) for a standalone re-check."]
fn e6_a_compiled_artifact_run_through_the_normal_cli_stays_memory_bounded_over_a_sustained_minute()
{
    if cfg!(debug_assertions) {
        return;
    }
    let src_file = write_source("b22-e6-soak.ml", &interleaved_soak(999_999_999_999));
    let artifact = temp_path("b22-e6-soak.mlbc");
    let compile_out = run(&[
        "compile",
        src_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert!(compile_out.status.success());

    let samples = sample_rss_over_a_minute(&["run", artifact.to_str().unwrap()]);
    assert_plateaus(&samples);
}
