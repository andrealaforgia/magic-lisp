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
    # Evidence: $ cat e1a.ml
    #   (define d (read)) (write d) (newline) (display (+ 1 2))
    #   $ echo "(+ 1 2)" | magiclisp eval e1a.ml -> (+ 1 2) / 3, exit 0
    #   $ cat e1b.ml: (display (read)) (display (read))
    #   $ printf "1 2" | magiclisp eval e1b.ml -> 12, exit 0 (advances across two calls)
    #   $ cat e1c.ml: (display (eof-object? (read))) (display (eof-object? (read)))
    #   $ printf "1" | magiclisp eval e1c.ml -> #f#t, exit 0 (ordinary value -> #f, EOF -> #t)

  Scenario: E2 — read-line reads a string with the line terminator genuinely stripped, same EOF semantics
    Given stdin with two lines "hello" and "world", and a single line "hello\n"
    When read-line is called repeatedly, and the returned string's length is measured
    Then it returns each line without its terminator, the third call at end-of-input satisfies eof-object?, and the returned string's length proves the terminator was actually removed, not just invisible
    # Evidence: $ cat e2a.ml
    #   (display (read-line)) (newline) (display (read-line)) (newline) (display (eof-object? (read-line))) (newline)
    #   $ printf "hello\nworld\n" | magiclisp eval e2a.ml -> hello / world / #t, exit 0
    #   $ cat e2b.ml: (display (string-length (read-line)))
    #   $ printf "hello\n" | magiclisp eval e2b.ml -> 5, exit 0 (not 6 — terminator removed)

  Scenario: E3 — display prints strings and characters raw
    Given a string with an embedded newline and a character value
    When each is displayed
    Then the embedded newline produces a real line break (not literal backslash-n), and the character shows as itself, bare
    # Evidence: $ cat e3.ml: (display "a\nb") (newline) (display #\a)
    #   $ magiclisp eval e3.ml -> a / b / a (three lines: real break inside the string, then the bare character), exit 0

  Scenario: E4 — write prints escaped, re-readable text; ordinary values are identical under write and display
    Given the same embedded-newline string, a symbol, a non-printing character, a number, and a list
    When each is written and, for the number/list/character, also displayed
    Then the embedded newline prints as literal backslash-n under write, the character prints in its named form under write versus bare under display, and the number and list print byte-for-byte identically under both styles
    # Evidence: $ cat e4.ml
    #   (write "a\nb") (newline) (write (quote sym)) (newline) (write #\space) (newline)
    #   (display #\space) (newline) (write 42) (newline) (display 42) (newline)
    #   (write (list 1 2 3)) (newline) (display (list 1 2 3))
    #   $ magiclisp eval e4.ml ->
    #   "a\nb" / sym / #\space / (a bare space) / 42 / 42 / (1 2 3) / (1 2 3), exit 0
    #   Independently re-verified write's literal escaping at the byte level: raw output is
    #   the 6 bytes `"`,`a`,`\`,`n`,`b`,`"` — not a real newline.

  Scenario: E5 — all output is fully flushed before exit, including with interleaved reads and writes
    Given a program that displays, reads a line, and displays again, ending on output with no trailing newline
    When it runs to completion
    Then every piece of output appears in stdout in the correct order, including the final unflushed-looking output before the process exits
    # Evidence: $ cat e5.ml: (display "start") (newline) (display (read-line)) (newline) (display "end")
    #   $ printf "middle\n" | magiclisp eval e5.ml -> start / middle / end, exit 0
    #   Independently re-verified at the byte level: raw stdout ends in the literal bytes
    #   "...middle\nend" with no trailing newline, and "end" is still fully present.

  Scenario: E6 — integration: all three verbatim demo scenarios produce exactly the prescribed output
    Given each of the three DEMO scenarios from the behaviour spec, with their specified stdin
    When each is run
    Then each produces exactly its prescribed output and exits 0
    # Evidence:
    #   Case 1: $ cat e6.ml: (define d (read)) (write d) (newline) (display (+ 1 2)) (newline)
    #     $ printf "(+ 1 2)\n" | magiclisp eval e6.ml -> (+ 1 2) / 3, exit 0
    #   Case 2: $ cat e6b.ml: (display (read-line)) (newline) (display (read-line)) (newline)
    #     (display (eof-object? (read-line))) (newline)
    #     $ printf "hello\nworld\n" | magiclisp eval e6b.ml -> hello / world / #t, exit 0
    #   Case 3 (no stdin): $ cat e6c.ml: (write "a\nb") (newline) (display "a\nb") (newline) (write (quote sym)) (newline)
    #     $ magiclisp eval e6c.ml -> "a\nb" / a / b / sym, exit 0
