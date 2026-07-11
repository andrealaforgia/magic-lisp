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
    # Evidence: filled in once the soak has been run against the release binary.

  Scenario: E2 — sustained mutual-reference closure creation does not leak memory over ~60 seconds
    Given a closure pattern exercised continuously for roughly 60 seconds where two closures' captured cells each hold the other closure, with memory sampled at multiple points across the run (not just a single before/after pair)
    When the sampled resident-memory trend is examined
    Then memory settles quickly and stays flat for the remainder of the run, rather than growing without bound, even though every generation is a genuine two-closure reference ring that ordinary reference counting alone could never reclaim
    # Evidence: filled in once the soak has been run against the release binary.

  Scenario: E3 — the cycle-safe mechanism is documented in plain language in the README
    Given the README's description of how closure/upvalue reference cycles are reclaimed
    When it is read on its own, without consulting the source
    Then a reader can understand the strategy well enough to know why E1 and E2 stay memory-bounded despite no host garbage collector existing
    # Evidence: filled in once the README section has been written.

  Scenario: E4 — integration: the self-reference soak, the mutual-reference soak, and B21's acyclic soak all hold together
    Given the self-referential soak (E1), the mutual-reference soak (E2), and the B21/E5 acyclic counter-factory soak, each run for its own full ~60-second sampled window in one review pass
    When each sampled resident-memory trend is examined together, alongside the README's mechanism description
    Then all three stay memory-bounded across their full runs, and what the README describes matches what is actually observed running -- demonstrating sustained memory-boundedness under genuine cyclic reference patterns on top of B21's acyclic guarantee, not just isolated pieces passing alone
    # Evidence: filled in once all three soaks have been re-confirmed together against the release binary.
