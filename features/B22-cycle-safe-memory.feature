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

  Scenario: E2 — sustained mutual-reference closure creation does not leak memory over ~60 seconds
    Given a closure pattern exercised continuously for roughly 60 seconds where two closures' captured cells each hold the other closure, with memory sampled at multiple points across the run (not just a single before/after pair)
    When the sampled resident-memory trend is examined
    Then memory settles quickly and stays flat for the remainder of the run, rather than growing without bound, even though every generation is a genuine two-closure reference ring that ordinary reference counting alone could never reclaim

  Scenario: E3 — the cycle-safe mechanism is documented in plain language in the README
    Given the README's description of how closure/upvalue reference cycles are reclaimed
    When it is read on its own, without consulting the source
    Then a reader can understand the strategy well enough to know why E1 and E2 stay memory-bounded despite no host garbage collector existing

  Scenario: E4 — integration: the self-reference, mutual-reference, and B21 acyclic patterns interleaved in one run all hold together
    Given a single ~60-second run that interleaves the self-referential pattern (E1), the mutual-reference pattern (E2), and the B21/E5 acyclic counter-factory pattern every iteration, with memory sampled at multiple points across the run, alongside the README's mechanism description
    When the sampled resident-memory trend is examined together with that description
    Then it stays memory-bounded across the full run, and what the README describes matches what is actually observed running -- demonstrating sustained memory-boundedness when genuine cyclic reference patterns and the acyclic pattern are exercised together, not just isolated pieces passing alone

  # E5 and E6 are proven as committed, always-run Rust tests in
  # tests/cli_integration/b22.rs rather than duplicated here as Gherkin --
  # they check correctness (does reclaiming a cycle ever corrupt a still-live
  # value?), not the memory-shape claims E1-E4 already cover, and a second
  # Gherkin+step-definition copy of the same assertion would be exactly the
  # kind of redundant test layer this project's own reviews have repeatedly
  # flagged elsewhere (B20, B21).
  #
  # E5 — both cyclic shapes still compute the correct result under real,
  # repeated automatic collection pressure (finite iteration counts, well
  # past the collector's sweep threshold), not just "memory stays flat" --
  # a collector that cleared a still-live cell would corrupt the answer
  # while leaving memory looking perfectly healthy.
  #
  # E6 — integration: the same guarantee holds through the real compile +
  # run path (an actual .mlbc artifact executed by the VM), not only via
  # eval, both at a finite correctness-checked scale and, redundantly with
  # this feature's own E4 scenario, over a sustained ~60-second soak
  # (gated `#[ignore]`, invoke explicitly for a standalone re-check).
