Feature: B15 — Error signalling and the exit procedure
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want failures to be reported precisely and uniformly, and a way to raise or exit deliberately
  So that this iteration's errors/disassembler/REPL/robustness work starts on solid ground, on top of B1-B14

  # Builds on B1-B14. The disassembler and REPL parts of this iteration are separate
  # behaviours, not this one. The exact wording of built-in-triggered error messages is not
  # prescribed (only the deliberate-error message text and irritant formatting are
  # normative) — the "Error: " prefix, the error output stream, and the specific
  # per-failure-class exit codes are what must hold.

  Scenario: E1 — deliberately raised errors show the message raw and irritants machine-readable
    Given an error raised with a message and one integer irritant, and an error raised with a message and mixed irritants including a string
    When each is run
    Then the message appears in human-readable form, each irritant appears in machine-readable form (a string irritant appears quoted, distinguishing it from a bare number/symbol), space-separated, on stderr with no stdout
    # Evidence: $ cat e1a.ml: (error "boom" 42)
    #   $ magiclisp eval e1a.ml -> stderr "Error: boom 42", exit 70, no stdout
    #   $ cat e1b.ml: (error "bad value" 1 "two" (quote three))
    #   $ magiclisp eval e1b.ml -> stderr "Error: bad value 1 \"two\" three", exit 70
    #   (the string irritant "two" is quoted — write style — distinguishing it from the bare
    #   integer and symbol irritants alongside it)
    #   Independently re-verified against the release binary.

  Scenario: E2 — every uncaught runtime error is uniform across failure categories, and nothing after the failure point runs
    Given a program that produces output, then misuses a built-in, then would produce more output, plus four more built-in-misuse categories (division by exact zero, wrong argument count, undefined name, wrong-type operand)
    When each is run
    Then only the output before the failure point appears, each produces exactly one "Error: "-prefixed stderr line, and all five cases (plus the deliberate-raise case from E1) share the IDENTICAL exit code
    # Evidence: $ cat e2-demo.ml: (display "before") (newline) (display (car 5)) (display "after")
    #   $ magiclisp eval e2-demo.ml -> stdout "before\n" only, stderr "Error: car expects a pair, found 5", exit 70
    #   $ cat e2-divzero.ml: (display (/ 1 0)) -> "Error: division by exact zero", exit 70
    #   $ cat e2-argcount.ml: (define (f a b) (+ a b)) (display (f 1)) -> "Error: expected exactly 2 argument(s), got 1", exit 70
    #   $ cat e2-undefined.ml: (display this-name-does-not-exist) -> "Error: unbound global: this-name-does-not-exist", exit 70
    #   $ cat e2-wrongtype.ml: (display (+ 1 "a")) -> "Error: + expects integer arguments, found a", exit 70
    #   All five share exit code 70, identical to E1's deliberate-raise cases.

  Scenario: E3 — a read/compile error exits with its own code, distinct from the runtime-error code
    Given a source file with an unterminated list (a read error, before the program starts running)
    When it is run
    Then free-form error text is reported on stderr and the process exits with a code distinct from E2's runtime-error code
    # Evidence: $ cat e3.ml: (display (+ 1  [unterminated]
    #   $ magiclisp eval e3.ml -> "error: read error: unterminated list: missing ')'", exit 65
    #   (65, distinct from the runtime-error exit code 70 shown throughout E1/E2)
    #   Independently re-verified against the release binary.

  Scenario: E4 — the exit procedure ends the program early with a chosen code or with success by default
    Given a program that exits with a specific code, one that exits with no code, and one that exits then attempts further output
    When each is run
    Then the specific code is used, no code means success, and nothing after the exit call executes
    # Evidence: $ cat e4a.ml: (exit 3) -> exit code 3, no output
    #   $ cat e4b.ml: (exit) -> exit code 0, no output
    #   $ cat e4c.ml: (exit 0) (display "should never appear") -> stdout empty, exit 0
    #   (the display call after exit never runs)

  Scenario: E5 — dividing a float by exactly zero is not an error, unlike dividing an integer by zero
    Given the same zero divisor applied to a float dividend and to an integer dividend
    When each division is displayed
    Then the float case succeeds with a recognizable infinity value and exit 0, while the integer case fails with the runtime-error exit code
    # Evidence: $ cat e5-float.ml: (display (/ 1.0 0.0)) (newline) -> +inf.0, exit 0
    #   $ cat e5-int.ml: (display (/ 1 0)) -> "Error: division by exact zero", exit 70
    #   (same operator, same zero divisor — type-dependent outcome made unambiguous)
    #   Independently re-verified against the release binary.

  Scenario: E6 — integration: all four verbatim demo scenarios produce exactly the prescribed output and exit codes
    Given each of the four DEMO scenarios from the behaviour spec
    When each is run
    Then each produces exactly its prescribed stdout/stderr content and exit code
    # Evidence:
    #   1. (error "boom" 42) -> no stdout, exit 70, stderr begins "Error: boom 42"
    #   2. (display "before")(newline)(display (car 5))(display "after") -> stdout "before\n" only, exit 70, stderr begins "Error: "
    #   3. (exit 3) -> exit code 3, no error output
    #   4. (display (/ 1.0 0.0))(newline) -> +inf.0, exit code 0
