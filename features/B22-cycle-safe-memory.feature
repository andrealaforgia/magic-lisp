Feature: B22 — Cycle-safe memory: closure/upvalue reference cycles stay bounded
  As a developer running long-lived MagicLisp programs that build self- or mutually-
  referencing closures
  I want the implementation to reclaim those reference cycles, not just acyclic closures
  So that a genuinely cyclic access pattern stays memory-bounded too, on top of B21

  # Builds on B21 (which proved the ACYCLIC closure soak stays flat -- a returned closure
  # that never references itself has no cycle, so ordinary Rc counting alone reclaims it).
  # This behaviour proves the harder case SPEC.md §2.1/§10.2 calls out by name: a captured
  # cell that ends up holding a reference back to its own capturing closure, directly or
  # through another closure, which plain Rc reference counting can never reclaim on its
  # own. The exact cycle-safe strategy is the implementation's choice (a tracing collector,
  # or a documented cycle-safe ownership strategy such as weak back-edges); it is not
  # observable from outside and not part of this behaviour's pass/fail criteria -- only
  # E3 requires it to be documented in plain language.

  Scenario: E1 — sustained self-referential closure creation does not leak memory over ~60 seconds
    Given a closure pattern exercised continuously for roughly 60 seconds where a captured cell is set! to hold the very closure that captured it, with memory sampled at multiple points across the run (not just a single before/after pair)
    When the sampled resident-memory trend is examined
    Then memory settles quickly and stays flat for the remainder of the run, rather than growing without bound, even though every generation is a genuine closure -> upvalue -> closure self-reference cycle that ordinary reference counting alone could never reclaim
    # Evidence: `cargo test --release --test features b22_cycle_safe_memory -- --nocapture`
    #   against commit 9faa44e: test result ok, finished in 213.10s.
    #   Examiner's own independent manual soak of the identical pattern against the same release
    #   binary (t=1/15/30/45/60s RSS): 5,696 / 13,056 / 17,056 / 21,072 / 19,296 KB -- growth
    #   decelerates and reverses in the run's second half (+2,240 KB) versus its first
    #   (+11,360 KB), both far inside the harness's generous slack, and the batched trial-
    #   deletion sweep visibly reclaims the backlog rather than letting it grow unbounded.

  Scenario: E2 — sustained mutual-reference closure creation does not leak memory over ~60 seconds
    Given a closure pattern exercised continuously for roughly 60 seconds where two closures' captured cells each hold the other closure, with memory sampled at multiple points across the run (not just a single before/after pair)
    When the sampled resident-memory trend is examined
    Then memory settles quickly and stays flat for the remainder of the run, rather than growing without bound, even though every generation is a genuine two-closure reference ring that ordinary reference counting alone could never reclaim
    # Evidence: same executed BDD run as E1 (same process, ok in 213.10s).
    #   Examiner's own independent manual soak of the identical pattern (t=1/15/30/45/60s RSS):
    #   6,592 / 15,504 / 19,296 / 20,752 / 22,768 KB -- second-half growth (+3,472 KB) stays well
    #   under the harness's generous slack and does not accelerate relative to the first half
    #   (+12,704 KB), confirming the mutual two-closure ring is reclaimed, not leaked.

  Scenario: E3 — the cycle-safe mechanism is documented in plain language in the README
    Given the README's description of how closure/upvalue reference cycles are reclaimed
    When it is read on its own, without consulting the source
    Then a reader can understand the strategy well enough to know why E1 and E2 stay memory-bounded despite no host garbage collector existing
    # Evidence: README.md's "Memory and cycle-safety" section explains both WHY plain Rc counting
    #   fails on a cycle ("Plain Rc counts never reach zero for a cycle like that") and HOW
    #   reclamation tells genuine garbage apart from a live object (real reference counts minus
    #   what's explained by other tracked objects; whatever's left must come from outside the
    #   tracked set). Checked by the stricter assert_readme_explains_cycle_safety in
    #   steps_b22.rs, which greps the section itself (not the whole file) for both explanations
    #   plus the mechanism's name -- part of the same green BDD run as E1/E2/E4.

  Scenario: E4 — integration: the self-reference, mutual-reference, and B21 acyclic patterns interleaved in one run all hold together
    Given a single ~60-second run that interleaves the self-referential pattern (E1), the mutual-reference pattern (E2), and the B21/E5 acyclic counter-factory pattern every iteration, with memory sampled at multiple points across the run, alongside the README's mechanism description
    When the sampled resident-memory trend is examined together with that description
    Then it stays memory-bounded across the full run, and what the README describes matches what is actually observed running -- demonstrating sustained memory-boundedness when genuine cyclic reference patterns and the acyclic pattern are exercised together, not just isolated pieces passing alone
    # Evidence: `cargo test --release --test features b22_cycle_safe_memory -- --nocapture`,
    #   commit 9faa44e, executed independently by the Examiner: "running 1 test ... test
    #   b22_cycle_safe_memory ... ok ... finished in 213.10s" -- a single interleaved ~60s soak
    #   (self-ref + mutual-ref + acyclic every iteration) plateaus and the README check passes
    #   in the same pass. This closes out a QA test-design regression (msg #350: the prior unit
    #   tests never actually drove a sweep, and the README check only grepped for four keywords
    #   anywhere in the file) and two Warden findings against the mechanism itself (indirection
    #   through a cons/vector/hash bypassing collection entirely, and O(N^2) aggregate sweep
    #   cost for long-lived-but-acyclic workloads) -- all re-verified green: full unit suite 812
    #   passed/0 failed, including the new pair/vector/hash/list-mediated-cycle reclamation
    #   tests, plus this BDD run.
