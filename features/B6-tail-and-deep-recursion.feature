Feature: B6 — Constant-space tail recursion and deep recursion
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want tail calls (self and mutual) to run in constant memory and genuine deep recursion to fail cleanly when exhausted
  So that recursion used as a loop behaves like a loop, completing this iteration's language core on top of B1-B5

  # Builds on B1-B5. Exact performance/timing floors are not part of this behaviour (a
  # later behaviour covers that) — here the requirement is that the constant-space cases
  # complete correctly at all, and that going too deep in genuine recursion fails cleanly
  # rather than crashing. How tail calls or deep recursion are implemented internally
  # (call representation, frame management) is not observable and not part of this
  # behaviour.

  Scenario: E1 — a self-tail-call loop runs an enormous number of iterations in flat memory
    Given a self-recursive function that calls itself as its very last action, counting from 0 to ten million
    When it is run to completion
    Then it displays "10000000"
    And peak memory usage does not scale with iteration count — it stays flat between a small-iteration-count run and the full ten-million-iteration run

  Scenario: E2 — mutual tail calls run an enormous number of round trips in flat memory
    Given two functions (even?/odd?) that call each other back and forth, each call being the last action, driven to a depth of ten million
    When it is run to completion
    Then it displays "#t"
    And peak memory usage stays flat between a small-depth run and the full ten-million-depth run, the same as E1

  Scenario: E3 — genuine non-tail recursion nests on the order of 100,000 levels and completes correctly
    Given a non-tail recursive sum (each call still has an addition pending after the recursive call returns) from 1 to 100,000
    When it is run to completion
    Then it displays "5000050000"

  Scenario: E4 — non-tail recursion nested too deep fails cleanly with a distinct exit code
    Given the same non-tail sum driven far past the depth genuine recursion can support
    When it is run
    Then it fails with a clean, reported runtime error and a distinct exit code, not a crash or hang
    And the boundary is exact: one level short of the limit succeeds, the limit itself fails

  Scenario: E5 — integration: all three verbatim demo programs produce exactly the prescribed output
    Given each of the three DEMO programs from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output followed by a trailing newline, and exits 0
