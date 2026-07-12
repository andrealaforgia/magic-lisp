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

  Scenario: E2 — every uncaught runtime error is uniform across failure categories, and nothing after the failure point runs
    Given a program that produces output, then misuses a built-in, then would produce more output, plus four more built-in-misuse categories (division by exact zero, wrong argument count, undefined name, wrong-type operand)
    When each is run
    Then only the output before the failure point appears, each produces exactly one "Error: "-prefixed stderr line, and all five cases (plus the deliberate-raise case from E1) share the IDENTICAL exit code

  Scenario: E3 — a read/compile error exits with its own code, distinct from the runtime-error code
    Given a source file with an unterminated list (a read error, before the program starts running)
    When it is run
    Then free-form error text is reported on stderr and the process exits with a code distinct from E2's runtime-error code

  Scenario: E4 — the exit procedure ends the program early with a chosen code or with success by default
    Given a program that exits with a specific code, one that exits with no code, and one that exits then attempts further output
    When each is run
    Then the specific code is used, no code means success, and nothing after the exit call executes

  Scenario: E5 — dividing a float by exactly zero is not an error, unlike dividing an integer by zero
    Given the same zero divisor applied to a float dividend and to an integer dividend
    When each division is displayed
    Then the float case succeeds with a recognizable infinity value and exit 0, while the integer case fails with the runtime-error exit code

  Scenario: E6 — integration: all four verbatim demo scenarios produce exactly the prescribed output and exit codes
    Given each of the four DEMO scenarios from the behaviour spec
    When each is run
    Then each produces exactly its prescribed stdout/stderr content and exit code
