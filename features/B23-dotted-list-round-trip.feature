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
    # Evidence: `cargo test --release --test cli_integration b23` (commit edc160b), all 4 green.
    #   Examiner's own independent re-check with different parameters than the Builder's fixtures
    #   (8,000 elements, symbolic tail `hello-tail` rather than an int): `compile` exits 0.

  Scenario: E2 — the freshly-written artifact decodes back byte-for-byte
    Given the MLBC artifact compiled from the long dotted-list literal
    When it is decoded by running it through the real CLI
    Then decoding succeeds with no truncation error, and every element along the chain plus the final tail match what was written
    # Evidence: same independent 8,000-element/symbolic-tail run: `run` against the real .mlbc
    #   file on disk exits 0 (no truncation/format rejection at 8,000 elements, well past the
    #   512-element cap).

  Scenario: E3 — running the compiled program produces the correct long dotted-list value
    Given the same compiled artifact
    When it is run and the value is displayed
    Then every element and the final tail are exactly as authored, proving the round-tripped value is actually usable at runtime
    # Evidence: the 8,000-element run's displayed stdout compared character-for-character against
    #   the expected `(1 2 3 ... 8000 . hello-tail)` string -- exact match (38,907 bytes both
    #   sides). Also verified via the unit-level boundary tests: cdr chains at the cap, one past
    #   it, and well past it (`round_trips_a_cdr_chained_constant_pair_{to_exactly,one_deeper,
    #   well_past}_the_configured_maximum`) all pass, while a separately-nested *tail* past the
    #   cap is still rejected (`rejects_a_cdr_chained_pairs_final_tail_when_the_tail_itself_is_
    #   nested_past_the_maximum`) -- confirming only the chain's hop count is exempt, not nesting
    #   in general.

  Scenario: E4 — genuine car-side nesting past the cap is still rejected
    Given a hand-crafted MLBC artifact with a List nested deeper than the const-nesting cap
    When it is run or disassembled through the real CLI
    Then it is rejected with exit code 66, exactly as before this fix
    # Evidence: `cli_integration::b23::e4_a_hand_crafted_artifact_with_car_side_list_nesting_
    #   past_the_cap_is_still_rejected` — green. Examiner's own independent check: truncating a
    #   real compiled artifact to 20 bytes and running it produces `error: MLBC file is truncated
    #   or corrupted`, exit 66 — confirming the malformed-file safety net (SPEC §8.2) generally,
    #   not just this one hand-crafted shape.

  Scenario: E5 — integration: the long dotted list round-trips and pathological nesting is still rejected, together
    Given the long dotted-list literal's full compile-run-display path and the hand-crafted pathologically-nested artifact
    When both are exercised together in one review pass
    Then the long dotted list runs correctly end to end and the pathologically-nested artifact is still rejected with exit code 66 -- the fix restores the round-trip guarantee without opening a hole in the malformed-input safety net
    # Evidence: `cli_integration::b23::e5_the_long_dotted_list_and_the_rejected_pathological_
    #   nesting_both_hold_together` — green, alongside the full `features` b23 BDD scenario
    #   (`b23_dotted_list_round_trip ... ok`, 0.33s) and the full `bytecode::` unit module (34
    #   passed/0 failed). Independently re-run by the Examiner against commit edc160b with
    #   fixture parameters distinct from the Builder's own (8,000 elements, symbolic tail,
    #   truncated-file rejection check) — all consistent with the scenarios above.
