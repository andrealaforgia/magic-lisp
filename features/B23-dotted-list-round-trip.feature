Feature: B23 — Dotted-list literals round-trip past the const-nesting cap
  As a developer compiling programs containing long dotted-list literals
  I want the bytecode round-trip to succeed regardless of how many elements the
  literal's cdr spine has
  So that a legitimately long dotted list isn't rejected by a cap meant to bound
  real recursive nesting, not flat list length

  # A dotted-list literal `(1 2 3 ... N . tail)` is one flat form -- its cdr chain
  # is program data, not recursive nesting -- but the bytecode decoder used to
  # spend one unit of its MAX_CONST_NESTING_DEPTH budget per cdr hop, the same as
  # genuine nesting (a List/Vector/car-chain of Pairs). A literal past that cap
  # (512) encoded fine but failed to decode its own freshly-written bytes. The cap
  # itself must still protect against genuine car-side/List/Vector nesting -- only
  # the cdr spine's hop count is now exempt.

  Scenario: E1 — compiling a program with a dotted-list literal well past the nesting cap succeeds
    Given a program containing a dotted-list literal several thousand elements long, well past the const-nesting cap
    When it is compiled
    Then compilation succeeds and produces an MLBC artifact, with no spurious rejection for the literal's length

  Scenario: E2 — the freshly-written artifact decodes back byte-for-byte
    Given the MLBC artifact compiled from the long dotted-list literal
    When it is decoded by running it through the real CLI
    Then decoding succeeds with no truncation error, and every element along the chain plus the final tail match what was written

  Scenario: E3 — running the compiled program produces the correct long dotted-list value
    Given the same compiled artifact
    When it is run and the value is displayed
    Then every element and the final tail are exactly as authored, proving the round-tripped value is actually usable at runtime

  Scenario: E4 — genuine car-side nesting past the cap is still rejected
    Given a hand-crafted MLBC artifact with a List nested deeper than the const-nesting cap
    When it is run or disassembled through the real CLI
    Then it is rejected with exit code 66, exactly as before this fix

  Scenario: E5 — integration: the long dotted list round-trips and pathological nesting is still rejected, together
    Given the long dotted-list literal's full compile-run-display path and the hand-crafted pathologically-nested artifact
    When both are exercised together in one review pass
    Then the long dotted list runs correctly end to end and the pathologically-nested artifact is still rejected with exit code 66 -- the fix restores the round-trip guarantee without opening a hole in the malformed-input safety net
