Feature: B17 — The interactive REPL
  As a user typing expressions interactively into `magiclisp repl` (or `magiclisp` with no arguments)
  I want to enter expressions one at a time, see each result immediately, and have definitions accumulate across entries
  So that the CLI supports an interactive session on top of B1-B16

  # Builds on B1-B16. Exact wording of the error line beyond the "Error: " prefix is not
  # prescribed. How the session loop or its internal state is implemented is not observable
  # and not part of this behaviour.

  Scenario: E1 — the exact prompt bytes appear once per entry plus a final one before end-of-input
    Given two entries piped into the REPL
    When the raw stdout bytes are inspected
    Then the prompt is exactly ">" followed by a single space with no trailing newline of its own, appearing once before each entry plus once more right before the session closes

  Scenario: E2 — results print write-style, except the unspecified value which prints nothing
    Given an entry evaluating to a number, an entry evaluating to a string, and a define entry (unspecified value)
    When each is entered
    Then the number and string print in write form (the string quoted, not raw) followed by a newline, and the define entry produces no output between its surrounding prompts

  Scenario: E3 — a definition persists across entries, and the latest redefinition wins — for both plain values and functions
    Given x defined, then referenced, then redefined, then referenced again; a single function defined and called with an argument from a later entry; two functions each defined in their own entry with the first called from a third entry; and a zero-argument function defined and called from a later entry
    When each entry is evaluated in sequence
    Then the first reference sees the original value and the second sees the redefined value, and every function case calls the CORRECT function's body with the correct result — no wrong-function execution, no spurious arity error, and no hang

  Scenario: E4 — a runtime error reports exactly one error line, recovers, and leaves bindings intact
    Given a definition, then an entry that misuses a built-in and errors, then a reference to the earlier definition
    When each is evaluated in sequence
    Then the failing entry produces no stdout output and exactly one "Error: "-prefixed stderr line, the session continues to the next prompt, and the earlier definition is still correctly bound afterward

  Scenario: E5 — end-of-input exits cleanly even with no errors
    Given a handful of ordinary entries with no errors, followed by end-of-input
    When the session runs to completion
    Then the process exits with code 0 and stderr is empty

  Scenario: E6 — running with no arguments starts the identical session
    Given the same sequence of entries piped into `magiclisp` with no arguments and into `magiclisp repl`
    When both are run
    Then their stdout and stderr are byte-identical

  Scenario: E7 — integration: the full five-entry verbatim demo produces exactly the prescribed transcript
    Given the DEMO sequence of five entries — (+ 1 2), (define x 10), x, (car 5), x — followed by end-of-input
    When the session runs
    Then stdout, stderr, and the exit code exactly match the prescribed transcript
