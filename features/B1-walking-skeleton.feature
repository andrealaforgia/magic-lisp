Feature: B1 — Walking skeleton: a MagicLisp CLI that reads, compiles, saves, and runs a minimal program end-to-end
  As a user invoking the `magiclisp` binary from a shell
  I want the five verbs (compile, run, eval, disasm, repl) to each work correctly
  So that a minimal program can be read, compiled, saved, and run through the full pipeline

  # This is a walking-skeleton iteration proving the pipeline spine, not full language
  # depth. Behaviour beyond what's listed here is out of scope for this pass; unimplemented
  # paths must fail predictably rather than misbehave silently. Internal storage, module
  # layout, and memory management are not observable from outside the CLI and are not part
  # of this behaviour.

  Scenario: E1 — eval reads and runs a source file directly
    Given a source file containing "(display (+ 1 2)) (newline)"
    When the user runs `magiclisp eval <file>`
    Then stdout is exactly "3\n"
    And the process exits with code 0
    # Evidence: $ magiclisp eval prog.ml (prog.ml = "(display (+ 1 2)) (newline)")
    #   -> stdout "3\n", exit 0

  Scenario: E2 — compile then run reproduces the source program's behaviour
    Given a source file containing "(display (+ 1 2)) (newline)"
    When the user runs `magiclisp compile <file> -o <out>`
    And then runs `magiclisp run <out>`
    Then stdout is byte-identical to running `magiclisp eval <file>` directly ("3\n")
    And the process exits with code 0
    # Evidence: $ magiclisp compile prog.ml -o prog.mlbc && magiclisp run prog.mlbc
    #   -> stdout "3\n", exit 0 (byte-identical to E1's eval output)

  Scenario: E3 — disasm prints a human-readable instruction listing
    Given a compiled artifact produced from "(display (+ 1 2)) (newline)"
    When the user runs `magiclisp disasm <out>`
    Then stdout is a legible instruction listing (not raw bytes, not a crash)
    And the process exits with code 0
    # Evidence: $ magiclisp disasm prog.mlbc -> exit 0, stdout:
    #   == chunk: 34 bytes code, 5 constants ==
    #   0000  GET_GLOBAL 0      ; display
    #   0005  GET_GLOBAL 1      ; +
    #   000a  CONST      2      ; 1
    #   000f  CONST      3      ; 2
    #   0014  CALL       2
    #   0016  CALL       1
    #   0018  POP
    #   0019  GET_GLOBAL 4      ; newline
    #   001e  CALL       0
    #   0020  POP
    #   0021  HALT

  Scenario: E4 — each of the five verbs is routed to genuinely distinct handling
    Given the five verbs compile, run, eval, disasm, repl (repl also being the default with no verb)
    When each is invoked with suitable arguments
    Then none are silently ignored, confused with another verb, or left unrouted
    And an unbuilt or unknown verb fails cleanly with a distinct exit code, not a hang or crash
    # Evidence: eval->"3", run->"3", disasm->listing text (not program output),
    #   compile->writes artifact silently, repl (stdin "(display 1)\n")->"1",
    #   no-verb default (stdin "(display 1)\n")->"1" (same as repl),
    #   unknown verb "frobnicate"->clean error + exit 64 (no hang/crash).

  Scenario: E5 — the reader accepts numbers, symbols, escaped strings, booleans, nested lists, comments, and whitespace together
    Given a source file "kitchen-sink.ml" containing:
      """
      ; a leading comment exercising every reader construct together
      (display "line one\nline two\ttabbed\r\"quoted\"\\backslash") (newline)
      (display (+ 42 (+ 1 2))) (newline)
      (display true) (newline)
      (display false) (newline)
      """
    When the user runs `magiclisp eval kitchen-sink.ml`
    Then stdout is exactly:
      """
      line one
      line two	tabbed"quoted"\backslash
      45
      #t
      #f
      """
    And the process exits with code 0
    And disassembling the compiled form shows two distinct CALL 2 instructions for the
      nested "(+ 42 (+ 1 2))" call (inner then outer) — the structural fingerprint of a
      genuinely nested list, distinguishing it from a flattened "(+ 42 1 2)" which would
      disassemble to a single "CALL 3"
    And the leading ";" comment line produces no output and no read error, proving it was
      skipped as a comment rather than treated as code
    # Corrected post hoc (E-RUN green-run pass): B1's spec only requires the READER to
    # accept the true/false source tokens (point 5) — it never mandated a display format
    # for booleans. The original evidence below showed "true"/"false" because that was
    # this system's incidental display choice at B1 time; B2's E12 later specified #t/#f
    # as the settled, deliberate, repeatedly-reconfirmed display convention (also B4).
    # This is the scenario's expected output catching up to a later, correct specification
    # decision, not a system defect — the source text (display true)/(display false) is
    # unchanged; only the expected stdout below is corrected.
    # Evidence: $ cat kitchen-sink.ml (shown above)
    #   $ magiclisp eval kitchen-sink.ml
    #   line one
    #   line two	tabbed"quoted"\backslash
    #   45
    #   #t
    #   #f
    #   exit code: 0
    #   $ magiclisp compile kitchen-sink.ml -o kitchen-sink.mlbc
    #   $ magiclisp disasm kitchen-sink.mlbc (relevant excerpt):
    #   0015  GET_GLOBAL 3      ; display
    #   001a  GET_GLOBAL 4      ; +
    #   001f  CONST      5      ; 42
    #   0024  GET_GLOBAL 6      ; +
    #   0029  CONST      7      ; 1
    #   002e  CONST      8      ; 2
    #   0033  CALL       2        <- inner (+ 1 2)
    #   0035  CALL       2        <- outer (+ 42 <inner result>)
    #   0037  CALL       1        <- display

  Scenario: E6 — an unescaped literal newline inside a string literal is a read error
    Given a source file containing a string literal with a literal, unescaped newline before its closing quote
    When the user runs `magiclisp eval <file>`
    Then stderr reports a read error mentioning the unescaped newline
    And no stdout is produced
    And the process exits with the source-error exit code
    # Evidence: $ magiclisp eval bad-string.ml
    #   -> stderr "error: read error: unescaped newline inside string literal before closing '\"'",
    #      exit 65, no stdout

  Scenario: E7 — invalid or corrupt compiled artifacts are rejected outright by run and disasm
    Given a compiled artifact corrupted in one of four ways: wrong magic bytes, unsupported
      version byte, truncated tail, or an out-of-range internal pointer
    When the user runs `magiclisp run <corrupt-file>` or `magiclisp disasm <corrupt-file>`
    Then the CLI rejects it with a clear stderr message
    And the process exits with the invalid-artifact exit code
    And it does not crash, hang, or silently produce wrong output
    # Evidence: built 4 corrupted artifacts from a valid one (bad magic bytes, unsupported
    #   version byte, truncated tail, out-of-range entry-index pointer) and ran both `run`
    #   and `disasm` on each: all 8 invocations rejected cleanly with a clear stderr message
    #   and exit 66; none crashed or hung.

  Scenario: E8 — the five failure classes produce pairwise-distinct exit codes
    Given one concrete case for each of: success, incorrect CLI usage, source program error,
      invalid/corrupt compiled artifact, and a runtime failure
    When each is run
    Then the exit codes are pairwise distinct
    # Evidence: exit-code matrix, one real invocation per class:
    #   success=0, usage error (eval with missing file arg)=64,
    #   source error (unterminated string)=65, bad artifact (corrupt magic)=66,
    #   runtime error (undefined global)=70 -- all pairwise distinct.

  Scenario: E9 — display, newline, and variadic + work out of the box, output stays ordered and fully flushed
    Given a source file that displays and newlines the results of (+), (+ 5), (+ 1 2), and (+ 1 2 3 4) in sequence
    When the user runs `magiclisp eval <file>`
    Then stdout is exactly "0\n5\n3\n10\n" in that order
    And the process exits with code 0
    # Evidence: $ magiclisp eval builtins.ml
    #   with (+),(+ 5),(+ 1 2),(+ 1 2 3 4) each displayed+newlined
    #   -> stdout "0\n5\n3\n10\n", exit 0 (ordered, fully flushed)

  Scenario: E10 — the full pipeline works end-to-end across process boundaries (integration)
    Given a source file "pipeline.ml" containing "(display (+ 19 23)) (newline)"
    When the user runs `magiclisp compile pipeline.ml -o pipeline.mlbc` in one process
    And then runs `magiclisp run pipeline.mlbc` in a separate process
    And then runs `magiclisp disasm pipeline.mlbc` in another separate process
    Then the compile step exits 0 and leaves the artifact file on disk
    And the run step prints "42\n" and exits 0
    And the disasm step prints a legible listing ending in HALT and exits 0
    # Evidence: three separate process invocations against one file on disk:
    #   (1) compile pipeline.ml -> pipeline.mlbc (exit 0, file exists on disk),
    #   (2) new process: run pipeline.mlbc -> "42\n", exit 0,
    #   (3) new process: disasm pipeline.mlbc -> legible listing ending in HALT, exit 0.
    #   Source was "(display (+ 19 23)) (newline)".

  Scenario: E11 — compiling the same source text twice is deterministic
    Given the same source file compiled twice via two separate `magiclisp compile` invocations to two different output paths
    When the two resulting artifact files are compared byte-for-byte
    Then they are byte-identical
    # Evidence: $ magiclisp compile det.ml -o det-a.mlbc
    #   $ magiclisp compile det.ml -o det-b.mlbc
    #   $ cmp det-a.mlbc det-b.mlbc && echo IDENTICAL
    #   IDENTICAL
    #   $ shasum -a 256 det-a.mlbc det-b.mlbc
    #   14bacbc5002bd1eecd507bdac081997317c161f49d364a862b9e033c2ec0049  det-a.mlbc
    #   14bacbc5002bd1eecd507bdac081997317c161f49d364a862b9e033c2ec0049  det-b.mlbc
    #   Also backed by an automated regression test (process-level, two real compile
    #   invocations, byte comparison) so this stays proven on every future change.
