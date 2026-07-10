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
    # Evidence: real 2.703s (comfortable margin under 10s)
    #   Independently re-verified: 2.628s total against the release binary.

  Scenario: E2 — a naive doubly-recursive Fibonacci computation completes within the time ceiling
    Given a naive, non-memoized recursive Fibonacci computation of fib(27)
    When it is run on the release build
    Then it displays the mathematically correct result (196418) well within the spec's ceiling of 20 seconds
    # Evidence: real 0.196s (comfortable margin under 20s), 196418 matches the known Fibonacci sequence
    #   Independently re-verified: 0.213s total, same correct value, against the release binary.

  Scenario: E3 — compiling a genuine ~2000-line source file completes within the time ceiling
    Given an actually-generated 2000-line source file
    When it is compiled on the release build
    Then compilation completes well within the spec's ceiling of 5 seconds and the resulting artifact runs correctly
    # Evidence: wc -l confirms 2000 lines; compile real 0.006s (comfortable margin under 5s); running the
    #   compiled artifact produces the expected output.

  Scenario: E4 — the tail-call loop uses constant, non-growing call-frame memory on the release build
    Given the same tail-call loop run at a small iteration count and at ten million iterations
    When peak resident memory is measured for each
    Then it stays flat across a 10,000x increase in iteration count, reconfirming B6's guarantee under release-build conditions
    # Evidence: 1,000 iterations -> 2,293,760 bytes peak RSS; 10,000,000 iterations -> 2,359,296 bytes
    #   (65,536-byte difference across a 10,000x iteration increase — flat)
    #   Independently re-verified against the release binary: 2,277,376 vs 2,359,296 bytes (81,920-byte
    #   difference across the same 10,000x increase — also flat).

  Scenario: E5 — sustained closure creation with a shared captured variable does not leak memory over ~60 seconds
    Given the B5 counter-factory closure pattern exercised continuously for roughly 60 seconds, with memory sampled at multiple points across the run (not just a single before/after pair)
    When the sampled resident-memory trend is examined
    Then memory settles quickly and stays flat for the remainder of the run, rather than growing without bound — this design has no host garbage collector, so this demonstrates the exercised pattern (a returned closure that never references itself, only its own private counter cell) has no reference cycle needing a cycle collector
    # Evidence: RSS sampled every ~15s over 60s: 2320 KB (t=1s), then 1664 KB flat at t=15/30/45/60s —
    #   settles after the first sample and stays completely flat, no growth trend.
    #   Strategy note (for documentation): the closure never holds a reference back to itself, so each
    #   generation is freed by ordinary reference counting the instant the caller replaces it — no cycle
    #   in this access pattern, so no separate cycle collector was needed.
    #   Independently re-verified with a separately-constructed ~60-second soak (a different, longer-running
    #   program sampled at the same cadence): RSS stayed flat at ~1664-1648 KB across all four samples.

  Scenario: E6 — integration: all four checks hold together on the release build
    Given the ten-million-iteration tail loop, the naive Fibonacci computation, the ~2000-line compile, and the sustained closure-creation soak
    When each is run together in one review pass
    Then each holds: 10000000 within its ceiling, 196418 within its ceiling, the large-file compile within its ceiling, and the same flat memory trend across another full ~60-second sampled run
    # Evidence: all four re-confirmed together, matching E1-E5's individual results.
