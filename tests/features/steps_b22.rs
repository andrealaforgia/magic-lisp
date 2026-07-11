//! Step definitions for features/B22-cycle-safe-memory.feature.
//!
//! Reuses B21's spawn/sample/kill soak harness (`sample_any_soak_rss_over_
//! a_minute`, `assert_plateaus`) against two genuinely CYCLIC closure
//! patterns instead of B21's acyclic one -- see that module's doc comment
//! for why the timing/memory-shape assertions only apply under an
//! optimised release build. E1/E2/E4's sustained ~60-second soaks are
//! skipped entirely under debug, matching B21's own precedent (the
//! Examiner's explicit instruction not to shorten the sampling duration).

use super::registry::Registry;
use super::steps_b21::{assert_plateaus, sample_any_soak_rss_over_a_minute};

/// E1: a captured cell `set!` to hold the very closure that captured it --
/// every generation is a genuine closure -> upvalue -> closure self-cycle.
const SELF_REFERENCE_SOAK: &str = "(define (make-self-ref) \
     (let ((cell #f)) (set! cell (lambda () cell)) cell)) \
     (define (soak i) (make-self-ref) (if (= i 0) 0 (soak (- i 1)))) \
     (soak 999999999999)";

/// E2: two closures whose captured cells each hold the other closure --
/// every generation is a genuine two-closure reference ring.
const MUTUAL_REFERENCE_SOAK: &str = "(define (make-mutual-pair) \
     (let ((a #f) (b #f)) \
       (set! a (lambda () b)) \
       (set! b (lambda () a)) \
       (cons a b))) \
     (define (soak i) (make-mutual-pair) (if (= i 0) 0 (soak (- i 1)))) \
     (soak 999999999999)";

/// E4: the self-reference (E1) and mutual-reference (E2) patterns AND
/// B21/E5's acyclic counter-factory pattern, interleaved every iteration
/// of one soak rather than run as three separate processes -- qa
/// test-design review msg #350 flagged the prior sequential version as
/// duplicating E1+E2+B21/E5 without exercising anything they didn't
/// already cover independently. Interleaving the three actually is a
/// materially different check: cyclic and acyclic garbage accumulating
/// together, in the same run, at the same time.
const INTERLEAVED_SOAK: &str = "(define (self-ref-once) \
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
     (soak 999999999999)";

fn readme_text() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md should exist at the project root")
}

/// Slices out the body of a `## <heading>` markdown section (up to the
/// next `##` heading or end of file) -- E3/E4 check the actual cycle-
/// safety section, not whatever else the README happens to say elsewhere.
fn readme_section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(heading)
        .unwrap_or_else(|| panic!("README should have a {heading:?} section"));
    let after_heading = &text[start + heading.len()..];
    let body_start = after_heading.find('\n').map_or(0, |i| i + 1);
    let body = &after_heading[body_start..];
    let end = body.find("\n## ").unwrap_or(body.len());
    &body[..end]
}

/// A README section only counts as understandable "without reading the
/// source" (E3) if it explains BOTH why plain reference counting fails on
/// a cycle (not just that "cycle" appears somewhere) AND, at a conceptual
/// level, how reclamation tells genuine garbage apart from something still
/// in use -- qa test-design review msg #350: a prior version of this check
/// only tested for four keywords anywhere in the whole file, which would
/// pass identically for a coherent explanation or four scattered buzzwords.
fn assert_readme_explains_cycle_safety(text: &str) {
    let section = readme_section(text, "## Memory and cycle-safety");
    let lower = section.to_lowercase();

    let explains_why_counting_fails = ["never reach zero", "never free", "never reclaim", "leak"]
        .iter()
        .any(|phrase| lower.contains(phrase));
    assert!(
        explains_why_counting_fails,
        "README's cycle-safety section should explain WHY plain reference counting fails \
         on a cycle (e.g. that counts never reach zero), not just name the word \"cycle\" \
         -- section text:\n{section}"
    );

    let explains_how_reclamation_decides = ["unreachable", "external", "outside"]
        .iter()
        .any(|phrase| lower.contains(phrase));
    assert!(
        explains_how_reclamation_decides,
        "README's cycle-safety section should explain, conceptually, how reclamation tells \
         genuine garbage apart from something still in use -- section text:\n{section}"
    );

    for phrase in ["cycle", "closure", "reference count", "reclaim"] {
        assert!(
            lower.contains(phrase),
            "README's cycle-safety section should mention {phrase:?} to be understandable \
             without reading the source"
        );
    }
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1/E2 share a "queue which cyclic shape, then sample" shape,
        // mirroring B21's own E1/E2 dispatch-by-queued-kind pattern. ---
        .step(
            "a closure pattern exercised continuously for roughly 60 seconds where a captured cell is set! to hold the very closure that captured it, with memory sampled at multiple points across the run (not just a single before/after pair)",
            |w, _text, _| {
                w.notes.push("self_ref".to_string());
            },
        )
        .step(
            "a closure pattern exercised continuously for roughly 60 seconds where two closures' captured cells each hold the other closure, with memory sampled at multiple points across the run (not just a single before/after pair)",
            |w, _text, _| {
                w.notes.push("mutual_ref".to_string());
            },
        )
        .step("the sampled resident-memory trend is examined", |w, _text, _| {
            if cfg!(debug_assertions) {
                return;
            }
            let kind = w
                .notes
                .pop()
                .expect("a Given step should have queued which cyclic soak to run");
            let samples = match kind.as_str() {
                "self_ref" => {
                    sample_any_soak_rss_over_a_minute("b22-e1-self-ref.ml", SELF_REFERENCE_SOAK)
                }
                "mutual_ref" => sample_any_soak_rss_over_a_minute(
                    "b22-e2-mutual-ref.ml",
                    MUTUAL_REFERENCE_SOAK,
                ),
                other => panic!("unknown B22 soak kind queued: {other}"),
            };
            w.notes = samples.iter().map(|s| s.to_string()).collect();
        })
        .step(
            "memory settles quickly and stays flat for the remainder of the run, rather than growing without bound, even though every generation is a genuine closure -> upvalue -> closure self-reference cycle that ordinary reference counting alone could never reclaim",
            |w, _text, _| {
                if cfg!(debug_assertions) {
                    return;
                }
                let samples: Vec<u64> = w.notes.iter().map(|s| s.parse().unwrap()).collect();
                assert_plateaus(&samples);
            },
        )
        .step(
            "memory settles quickly and stays flat for the remainder of the run, rather than growing without bound, even though every generation is a genuine two-closure reference ring that ordinary reference counting alone could never reclaim",
            |w, _text, _| {
                if cfg!(debug_assertions) {
                    return;
                }
                let samples: Vec<u64> = w.notes.iter().map(|s| s.parse().unwrap()).collect();
                assert_plateaus(&samples);
            },
        )
        // --- E3 ---
        .step(
            "the README's description of how closure/upvalue reference cycles are reclaimed",
            |w, _text, _| {
                w.notes.push(readme_text());
            },
        )
        .step(
            "it is read on its own, without consulting the source",
            |_w, _text, _| { /* purely descriptive; the Then step below checks the queued text */
            },
        )
        .step(
            "a reader can understand the strategy well enough to know why E1 and E2 stay memory-bounded despite no host garbage collector existing",
            |w, _text, _| {
                let text = w
                    .notes
                    .last()
                    .expect("the README text should have been queued by the Given step");
                assert_readme_explains_cycle_safety(text);
            },
        )
        // --- E4: integration (interleaved, not sequential -- see
        // INTERLEAVED_SOAK's own doc comment) ---
        .step(
            "a single ~60-second run that interleaves the self-referential pattern (E1), the mutual-reference pattern (E2), and the B21/E5 acyclic counter-factory pattern every iteration, with memory sampled at multiple points across the run, alongside the README's mechanism description",
            |w, _text, _| {
                w.notes.push(readme_text());
            },
        )
        .step(
            "the sampled resident-memory trend is examined together with that description",
            |w, _text, _| {
                if cfg!(debug_assertions) {
                    return;
                }
                let samples = sample_any_soak_rss_over_a_minute("b22-e4-interleaved.ml", INTERLEAVED_SOAK);
                for s in samples {
                    w.notes.push(s.to_string());
                }
            },
        )
        .step(
            "it stays memory-bounded across the full run, and what the README describes matches what is actually observed running -- demonstrating sustained memory-boundedness when genuine cyclic reference patterns and the acyclic pattern are exercised together, not just isolated pieces passing alone",
            |w, _text, _| {
                let readme = w
                    .notes
                    .first()
                    .expect("the README text should have been queued by the Given step");
                assert_readme_explains_cycle_safety(readme);

                if cfg!(debug_assertions) {
                    return;
                }
                let samples: Vec<u64> = w.notes[1..].iter().map(|s| s.parse().unwrap()).collect();
                assert_plateaus(&samples);
            },
        )
}
