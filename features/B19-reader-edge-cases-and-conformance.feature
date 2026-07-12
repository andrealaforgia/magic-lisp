Feature: B19 — Reader edge cases and the full conformance-sample pass
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want the remaining lexical forms handled correctly and the whole system to reproduce every officially published sample
  So that this starts the final hardening iteration on top of B1-B18

  # Builds on B1-B18. How comments are skipped or literals parsed internally is not
  # observable and not part of this behaviour — only that the described lexical forms are
  # accepted/rejected correctly and that the ten published samples reproduce their
  # specified results exactly.

  Scenario: E1 — line comments still work
    Given a source file with a leading line comment before a display call
    When it is run
    Then the comment is skipped and the display call runs normally

  Scenario: E2 — block comments, including one fully nested inside another, are skipped correctly
    Given a single block comment preceding code, and a block comment containing another complete block comment nested inside it
    When each is run
    Then both are skipped entirely and the following code runs normally — the inner nested comment does not end the outer one early

  Scenario: E3 — the skip-next-datum marker removes exactly one complete datum
    Given a stray value between two operands of a sum, immediately preceded by the skip marker, and a skipped datum that is itself a whole compound list
    When each is run
    Then the marked datum is skipped entirely regardless of whether it's a single token or a multi-token compound structure

  Scenario: E4 — dotted-pair literals and alternate-radix integer literals still read correctly
    Given a quoted dotted-pair literal and integer literals in hex, binary, and octal
    When each is displayed
    Then the dotted pair shows its written structure and each radix reads to the correct decimal value

  Scenario: E5 — oversized integer literals still read-error, and arithmetic overflow still wraps
    Given an integer literal exceeding the range, and an addition that overflows the maximum representable integer
    When each is run
    Then the oversized literal is a read error and the overflow wraps rather than erroring or growing arbitrarily

  Scenario: E6 — all ten officially published sample programs reproduce their specified results exactly
    Given each of the ten SPEC.md 9.5 sample programs (factorial/redefinition, tail-call/loop, closures, shared captured variable, macros, quasiquotation, division, error signalling, reading input, hash tables)
    When each is run through the real compile-then-run pipeline
    Then each produces exactly its officially specified stdout, exit code, and (for the error sample) error-stream prefix

  Scenario: E7 — integration: all four verbatim lexical demos produce exactly the prescribed output
    Given the four DEMOs from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output with a trailing newline and exit code 0
