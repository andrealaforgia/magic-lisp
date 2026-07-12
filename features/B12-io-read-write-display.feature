Feature: B12 — Input reading and the write/display output distinction
  As a user invoking `magiclisp` with input piped or typed on standard input
  I want to read data and lines from standard input, and print output in either a raw human-readable or an escaped machine-readable style
  So that this iteration's standard-library breadth is complete on top of B1-B11

  # Builds on B1-B11. This completes standard-library breadth for this iteration — no
  # further data types or operations are in scope here. How reading/buffering/flushing is
  # implemented internally is not observable and not part of this behaviour.

  Scenario: E1 — read returns data unevaluated, advances across calls, and EOF is checkable both ways
    Given stdin containing the text "(+ 1 2)", stdin containing two data units, and stdin exhausted after one value
    When read is called and its result is written or checked with eof-object?
    Then it returns the literal unevaluated data (confirmed distinct from separately computing the same expression), advances correctly across repeated calls, and eof-object? is #f for an ordinary value and #t for the end-of-input marker

  Scenario: E2 — read-line reads a string with the line terminator genuinely stripped, same EOF semantics
    Given stdin with two lines "hello" and "world", and a single line "hello\n"
    When read-line is called repeatedly, and the returned string's length is measured
    Then it returns each line without its terminator, the third call at end-of-input satisfies eof-object?, and the returned string's length proves the terminator was actually removed, not just invisible

  Scenario: E3 — display prints strings and characters raw
    Given a string with an embedded newline and a character value
    When each is displayed
    Then the embedded newline produces a real line break (not literal backslash-n), and the character shows as itself, bare

  Scenario: E4 — write prints escaped, re-readable text; ordinary values are identical under write and display
    Given the same embedded-newline string, a symbol, a non-printing character, a number, and a list
    When each is written and, for the number/list/character, also displayed
    Then the embedded newline prints as literal backslash-n under write, the character prints in its named form under write versus bare under display, and the number and list print byte-for-byte identically under both styles

  Scenario: E5 — all output is fully flushed before exit, including with interleaved reads and writes
    Given a program that displays, reads a line, and displays again, ending on output with no trailing newline
    When it runs to completion
    Then every piece of output appears in stdout in the correct order, including the final unflushed-looking output before the process exits

  Scenario: E6 — integration: all three verbatim demo scenarios produce exactly the prescribed output
    Given each of the three DEMO scenarios from the behaviour spec, with their specified stdin
    When each is run
    Then each produces exactly its prescribed output and exits 0
