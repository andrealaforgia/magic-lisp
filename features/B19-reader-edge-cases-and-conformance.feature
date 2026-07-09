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
    # Evidence: a leading "; comment" line followed by (display 1) -> 1, exit 0

  Scenario: E2 — block comments, including one fully nested inside another, are skipped correctly
    Given a single block comment preceding code, and a block comment containing another complete block comment nested inside it
    When each is run
    Then both are skipped entirely and the following code runs normally — the inner nested comment does not end the outer one early
    # Evidence: $ printf '#| a block comment |# (display 1) (newline)' | magiclisp eval /dev/stdin -> 1, exit 0
    #   $ printf '#| outer #| nested |# still outer |# (display 2) (newline)' | magiclisp eval /dev/stdin -> 2, exit 0
    #   (if the inner comment wrongly ended the outer one, " still outer |#" would leak out as
    #   unparseable source text and fail — it doesn't)
    #   Independently re-verified against the release binary.

  Scenario: E3 — the skip-next-datum marker removes exactly one complete datum
    Given a stray value between two operands of a sum, immediately preceded by the skip marker, and a skipped datum that is itself a whole compound list
    When each is run
    Then the marked datum is skipped entirely regardless of whether it's a single token or a multi-token compound structure
    # Evidence: $ printf '(display (+ 1 #;99 2)) (newline)' | magiclisp eval /dev/stdin -> 3, exit 0
    #   $ printf '(display (+ 1 #;(a b c) 2)) (newline)' | magiclisp eval /dev/stdin -> 3, exit 0
    #   (the skipped datum is itself a whole list, not just one token)
    #   Independently re-verified against the release binary.

  Scenario: E4 — dotted-pair literals and alternate-radix integer literals still read correctly
    Given a quoted dotted-pair literal and integer literals in hex, binary, and octal
    When each is displayed
    Then the dotted pair shows its written structure and each radix reads to the correct decimal value
    # Evidence: (display (quote (1 . 2))) (newline) -> (1 . 2), exit 0
    #   (display #x1A) (newline) (display #b101) (newline) (display #o17) (newline) -> 26/5/15, exit 0

  Scenario: E5 — oversized integer literals still read-error, and arithmetic overflow still wraps
    Given an integer literal exceeding the range, and an addition that overflows the maximum representable integer
    When each is run
    Then the oversized literal is a read error and the overflow wraps rather than erroring or growing arbitrarily
    # Evidence: (display 99999999999999999999999999999) -> read error, exit 65
    #   (display (+ 9223372036854775807 1)) -> -9223372036854775808, exit 0 (wraps)

  Scenario: E6 — all ten officially published sample programs reproduce their specified results exactly
    Given each of the ten SPEC.md 9.5 sample programs (factorial/redefinition, tail-call/loop, closures, shared captured variable, macros, quasiquotation, division, error signalling, reading input, hash tables)
    When each is run through the real compile-then-run pipeline
    Then each produces exactly its officially specified stdout, exit code, and (for the error sample) error-stream prefix
    # Evidence (spec-quoted expected vs actual, all MATCH):
    #   010-factorial: fact(10) via stub-then-real redefinition -> "3628800\n", exit 0
    #   020-tco: self-tail loop to 10,000,000 -> "10000000\n", exit 0
    #   030-closures: two independent counters, interleaved -> "1\n2\n1\n", exit 0
    #   040-shared-upvalue: getter/setter sharing one variable -> "10\n", exit 0
    #   050-macro: swap! via gensym -> "(2 1)\n", exit 0
    #   060-quasi: `(1 ,@mid 5) with mid=(2 3 4) -> "(1 2 3 4 5)\n", exit 0
    #   070-division: 6/3, 7/2, 6/3.0, 1.0 -> "2\n3.5\n2.0\n1.0\n", exit 0
    #   080-error: (error "boom" 42) -> stdout empty, exit 70, stderr "Error: boom 42"
    #   090-read: stdin "(+ 1 2)", read+write then compute -> "(+ 1 2)\n3\n", exit 0
    #   100-hash: create/set/count/keys/fallback/has-key/remove -> "2\n(a b)\nnope\n#t\n#f\n", exit 0
    #   Independently re-verified samples 010-factorial and 100-hash against the release binary.

  Scenario: E7 — integration: all four verbatim lexical demos produce exactly the prescribed output
    Given the four DEMOs from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output with a trailing newline and exit code 0
    # Evidence: 1. skip-marker sum (stray 99 skipped) -> 3
    #   2. single block comment then display 1 -> 1
    #   3. nested block comment then display 2 -> 2
    #   4. dotted pair display -> (1 . 2)
