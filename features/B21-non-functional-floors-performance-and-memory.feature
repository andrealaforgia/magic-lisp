Feature: B21 — Non-functional floors: performance and memory
  As a developer running MagicLisp programs on an optimised release build
  I want the system to meet generous performance floors and stay memory-stable under sustained use
  So that it is fast and reliable enough for real use, not merely correct

  # Builds on B1-B20. All floors below are measured on an OPTIMISED RELEASE build
  # (SPEC.md 10.1/10.2) -- an ordinary unoptimized debug build is not held to these
  # ceilings, since interpreter overhead alone can exceed them without indicating any
  # real regression. The exact heap-management/cycle-safety strategy is not observable
  # from outside the CLI and is not part of this behaviour's pass/fail criteria.

  Scenario: E1 — a ten-million-iteration tail-recursive loop completes within the time ceiling
    Given a tail-recursive loop counting from 0 to ten million
    When it is run on the release build
    Then it displays "10000000" and completes within the spec's ceiling of 10 seconds
    # Evidence: SPEC.md 10.1 ceiling: <= 10s. Measured: ~2.5s (comfortable margin).

  Scenario: E2 — a naive doubly-recursive Fibonacci computation completes within the time ceiling
    Given a naive, non-memoized recursive computation of fib(27)
    When it is run on the release build
    Then it displays "196418" and completes within the spec's ceiling of 20 seconds
    # Evidence: SPEC.md 10.1 ceiling: <= 20s. Measured: ~0.2s (comfortable margin);
    #   196418 independently confirmed against the known Fibonacci sequence.

  Scenario: E3 — compiling a genuine ~2000-line source file completes within the time ceiling
    Given an actually-generated source file of roughly 2000 lines
    When it is compiled on the release build
    Then compilation completes within the spec's ceiling of 5 seconds and the compiled artifact runs correctly
    # Evidence: SPEC.md 10.1 ceiling: <= 5s. Measured: ~0.01s (comfortable margin).

  Scenario: E4 — the tail-call loop uses constant, non-growing call-frame memory on the release build
    Given the same tail-recursive loop run first at a small iteration count and then at ten million
    When peak resident memory is measured for each run on the release build
    Then peak memory at ten million iterations is not meaningfully larger than at the small count
    # Evidence: peak RSS at 1,000 iterations vs. 10,000,000 iterations differs by
    #   well under 10 MB (a real per-iteration leak at that scale would dwarf this).

  Scenario: E5 — repeatedly creating closures that capture a shared variable stays memory-bounded over a sustained run
    Given a program that repeatedly creates and calls fresh closures capturing a shared counter variable
    When it runs continuously for about 60 seconds on the release build while memory is sampled at several points
    Then the memory measurements plateau rather than growing without bound
    # Evidence: RSS sampled at ~1s/15s/30s/45s/60s stays flat after the initial
    #   startup sample (no growth trend across the run's second half).

  Scenario: E6 — integration: all four performance and memory demos hold together on the release build
    Given the ten-million-iteration tail loop, the naive fib(27) computation, the ~2000-line compile, and the sustained closure-creation soak
    When each is run on the release build
    Then the tail loop displays "10000000" within its ceiling, fib(27) displays "196418" within its ceiling, the compile finishes within its ceiling, and the soak's memory measurements stay bounded
    # Evidence: as demonstrated in E1-E5 above, all confirmed together in one review pass.
