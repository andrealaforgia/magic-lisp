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
    # Evidence: $ printf '1\n2\n' | magiclisp repl | od -c
    #   >   1  \n   >   2  \n   >     (three prompts for two entries: one each, plus the final one)

  Scenario: E2 — results print write-style, except the unspecified value which prints nothing
    Given an entry evaluating to a number, an entry evaluating to a string, and a define entry (unspecified value)
    When each is entered
    Then the number and string print in write form (the string quoted, not raw) followed by a newline, and the define entry produces no output between its surrounding prompts
    # Evidence: $ printf '(+ 1 2)\n' | magiclisp repl -> "> 3\n> ", exit 0
    #   $ printf '"hi"\n' | magiclisp repl -> "> \"hi\"\n> ", exit 0 (quoted — write style)
    #   $ printf '(define x 10)\n' | magiclisp repl -> "> > ", exit 0 (nothing between the two prompts)

  Scenario: E3 — a definition persists across entries, and the latest redefinition wins — for both plain values and functions
    Given x defined, then referenced, then redefined, then referenced again; a single function defined and called with an argument from a later entry; two functions each defined in their own entry with the first called from a third entry; and a zero-argument function defined and called from a later entry
    When each entry is evaluated in sequence
    Then the first reference sees the original value and the second sees the redefined value, and every function case calls the CORRECT function's body with the correct result — no wrong-function execution, no spurious arity error, and no hang
    # Evidence: $ printf '(define x 10)\nx\n(define x 20)\nx\n' | magiclisp repl
    #   -> "> > 10\n> > 20\n> ", exit 0
    #   $ printf '(define (inc n) (+ n 1))\n(inc 5)\n' | magiclisp repl -> "> > 6\n> ", exit 0
    #   $ printf '(define g (lambda (n) (+ n 1)))\n(define h (lambda (x) (* x 100)))\n(g 3)\n' | magiclisp repl
    #   -> "> > > 4\n> ", exit 0 (calls g's body correctly, not h's — a critical bug found and fixed here:
    #   cross-entry function calls previously either executed the wrong function's body, threw a
    #   spurious arity error, or — for a zero-argument function — hung indefinitely)
    #   $ perl -e 'alarm 10; exec @ARGV' bash -c "printf '(define (f) 42)\n(f)\n' | magiclisp repl"
    #   -> "> > 42\n> ", exit 0, well within a 10-second watchdog (no hang)
    #   Independently re-verified all four cases against the release binary, including the
    #   zero-argument case under a watchdog.

  Scenario: E4 — a runtime error reports exactly one error line, recovers, and leaves bindings intact
    Given a definition, then an entry that misuses a built-in and errors, then a reference to the earlier definition
    When each is evaluated in sequence
    Then the failing entry produces no stdout output and exactly one "Error: "-prefixed stderr line, the session continues to the next prompt, and the earlier definition is still correctly bound afterward
    # Evidence: $ printf '(define x 10)\n(car 5)\nx\n' | magiclisp repl
    #   stdout: "> > > 10\n> " (three prompts back-to-back before "10" — define and the error both produce
    #     no stdout output — then x still evaluates to 10)
    #   stderr: "Error: car expects a pair, found 5"
    #   exit code: 0

  Scenario: E5 — end-of-input exits cleanly even with no errors
    Given a handful of ordinary entries with no errors, followed by end-of-input
    When the session runs to completion
    Then the process exits with code 0 and stderr is empty
    # Evidence: $ printf '1\n2\n3\n' | magiclisp repl -> exit code 0, stderr empty

  Scenario: E6 — running with no arguments starts the identical session
    Given the same sequence of entries piped into `magiclisp` with no arguments and into `magiclisp repl`
    When both are run
    Then their stdout and stderr are byte-identical
    # Evidence: $ printf '(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n' | magiclisp > bare.out 2> bare.err
    #   $ printf '(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n' | magiclisp repl > repl.out 2> repl.err
    #   $ diff bare.out repl.out -> no output (byte-identical)
    #   $ diff bare.err repl.err -> no output (byte-identical)

  Scenario: E7 — integration: the full five-entry verbatim demo produces exactly the prescribed transcript
    Given the DEMO sequence of five entries — (+ 1 2), (define x 10), x, (car 5), x — followed by end-of-input
    When the session runs
    Then stdout, stderr, and the exit code exactly match the prescribed transcript
    # Evidence: $ printf '(+ 1 2)\n(define x 10)\nx\n(car 5)\nx\n' | magiclisp repl
    #   stdout: "> 3\n> > 10\n> > 10\n> "
    #   stderr: "Error: car expects a pair, found 5"
    #   exit code: 0
    #   Independently re-verified at the byte level (od -c) against the release binary.
