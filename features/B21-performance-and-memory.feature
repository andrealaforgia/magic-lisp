Feature: B21 — Non-functional floors: performance and memory
  As a developer running MagicLisp programs on an optimised release build
  I want generous minimum performance floors and memory-stability under sustained closure use
  So that the system is confirmed fast and memory-stable enough for real use, on top of B1-B20

  # Builds on B1-B20. Measured on an optimised release build. The exact heap-management/
  # cycle-safety strategy used to achieve memory stability is not observable and not part
  # of this behaviour's pass/fail criteria.

  Scenario: E1 — a ten-million-iteration tail-recursive loop completes within the time ceiling
    Given a self-tail-call loop counting to ten million
    When it is run on the release build
    Then it displays 10000000 well within the spec's ceiling of 10 seconds

  Scenario: E2 — a naive doubly-recursive Fibonacci computation completes within the time ceiling
    Given a naive, non-memoized recursive Fibonacci computation of fib(27)
    When it is run on the release build
    Then it displays the mathematically correct result (196418) well within the spec's ceiling of 20 seconds

  Scenario: E3 — compiling a genuine ~2000-line source file completes within the time ceiling
    Given an actually-generated 2000-line source file
    When it is compiled on the release build
    Then compilation completes well within the spec's ceiling of 5 seconds and the resulting artifact runs correctly

  Scenario: E4 — the tail-call loop uses constant, non-growing call-frame memory on the release build
    Given the same tail-call loop run at a small iteration count and at ten million iterations
    When peak resident memory is measured for each
    Then it stays flat across a 10,000x increase in iteration count, reconfirming B6's guarantee under release-build conditions

  Scenario: E5 — sustained closure creation with a shared captured variable does not leak memory over ~60 seconds
    Given the B5 counter-factory closure pattern exercised continuously for roughly 60 seconds, with memory sampled at multiple points across the run (not just a single before/after pair)
    When the sampled resident-memory trend is examined
    Then memory settles quickly and stays flat for the remainder of the run, rather than growing without bound — this design has no host garbage collector, so this demonstrates the exercised pattern (a returned closure that never references itself, only its own private counter cell) has no reference cycle needing a cycle collector

  Scenario: E6 — integration: all four checks hold together on the release build
    Given the ten-million-iteration tail loop, the naive Fibonacci computation, the ~2000-line compile, and the sustained closure-creation soak
    When each is run together in one review pass
    Then each holds: 10000000 within its ceiling, 196418 within its ceiling, the large-file compile within its ceiling, and the same flat memory trend across another full ~60-second sampled run
