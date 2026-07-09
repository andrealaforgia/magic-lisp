Feature: B18 — Crash-free robustness across all malformed input
  As a user running `magiclisp` against arbitrary, possibly malformed, input
  I want the process to never crash outright no matter what it's handed
  So that this completes the errors/disassembler/REPL/robustness iteration on top of B1-B17

  # Builds on B1-B17. This completes this iteration — no further behaviours are in scope
  # here. Exactly how malformed input is detected (at what stage, by what validation
  # strategy) is not observable and not part of this behaviour — only that it's always
  # detected cleanly and mapped to one of the established outcomes.

  Scenario: E1 — broken source text always ends with the source-error exit code, never a crash
    Given unbalanced parentheses, an unterminated string, a raw unescaped newline inside a string, an oversized whole-number literal, and a stray misplaced dot
    When each is run
    Then each ends the process with the source-error exit code and no crash output
    # Evidence: $ magiclisp eval unbalanced-parens.ml   -> "unterminated list: missing ')'", exit 65
    #   $ magiclisp eval unterminated-string.ml -> "unterminated string literal: missing closing '\"'", exit 65
    #   $ magiclisp eval raw-newline.ml         -> "unescaped newline inside string literal...", exit 65
    #   $ magiclisp eval oversized-int.ml       -> "integer literal out of range or malformed: ...", exit 65
    #   $ magiclisp eval stray-dot.ml           -> "expected ')' immediately after the tail of a dotted list", exit 65
    #   Independently re-verified all five against the release binary.

  Scenario: E2 — an invalid outer-container artifact always ends with the file-format-error exit code
    Given an artifact with non-zero flags where none are allowed, handed to both run and disasm
    When each is run
    Then each ends with the file-format-error exit code
    # Evidence: $ magiclisp run flags-set.mlbc    -> "MLBC file sets unsupported flags: 0x0001", exit 66
    #   $ magiclisp disasm flags-set.mlbc -> "MLBC file sets unsupported flags: 0x0001", exit 66
    #   (bad magic / unsupported version / truncated / out-of-range pointer — established in B1's
    #   E7 — still reject with the file-format-error exit code for both verbs, unchanged)

  Scenario: E3 — a valid-outer-container but internally-broken artifact is reported cleanly, never a crash
    Given an artifact with an undefined opcode byte, an out-of-range constant-pool index, and an instruction truncated mid-operand — each corrupting only internal code/constant bytes, not the header
    When each is run (and, for the undefined-opcode case, also disassembled)
    Then each is reported as a runtime error or file-format error (either acceptable) with no crash, and disasm may instead gracefully label an unrecognized opcode and continue rather than erroring, since exit 0 with no crash also satisfies "always lands on an established outcome"
    # Evidence: $ magiclisp run bad-opcode.mlbc      -> partial stdout "1", then "Error: undefined opcode: 250", exit 70
    #   $ magiclisp disasm bad-opcode.mlbc   -> succeeds, shows "<unknown opcode 250>" in place of that
    #     instruction, exit 0 (no crash — a defensible disassembler design: showing corrupted regions
    #     rather than aborting the whole dump, still landing on an established exit code)
    #   $ magiclisp run bad-const-index.mlbc -> "Error: constant index 4294967295 out of range", exit 70
    #   $ magiclisp run mid-instruction.mlbc -> partial stdout "1", then "Error: truncated instruction operand", exit 70

  Scenario: E4 — command-line misuse always ends with the usage-error exit code
    Given an unrecognized verb and a required argument left off
    When each is run
    Then each ends with the usage-error exit code
    # Evidence: $ magiclisp frobnicate -> "unknown verb 'frobnicate' (expected one of: compile, run, eval, disasm, repl)", exit 64
    #   $ magiclisp eval       -> "usage: magiclisp eval <file>", exit 64
    #   Independently re-verified both against the release binary.

  Scenario: E5 — a genuine breadth sweep across many malformed inputs never crashes or exits outside the established set
    Given dozens of malformed source snippets spanning many categories beyond E1's five, and every possible single-byte corruption of a valid compiled artifact run through both run and disasm
    When the full sweep is executed
    Then every single run exits on an established exit code with no signal-killed/crash termination
    # Evidence: 31 malformed source snippets: 31/31 (100%) exited cleanly on an established code
    #   (unbalanced/excess parens/brackets, empty/whitespace/comment-only input, bad escapes, bad
    #   radix literals, vector/char-literal malformations, stray dot placements, and more)
    #   65 byte offsets of a compiled artifact, each corrupted and run through both run and disasm
    #   (130 runs): 100% exited cleanly on an established code. Zero signal deaths, zero
    #   unrecognized exit codes across the whole sweep.
    #   Independently re-verified with a second, differently-constructed sweep: every one of 85
    #   byte offsets of a separately-compiled artifact corrupted to 0xFF, run through both run and
    #   disasm (170 runs) — zero crashes, zero out-of-set exit codes.

  Scenario: E6 — integration: all six verbatim demo cases exit with exactly the prescribed code
    Given each of the six DEMO cases from the behaviour spec
    When each is run
    Then each exits with exactly its prescribed established exit code, with no crash output
    # Evidence: 1. "(((" -> exit 65 (source error)
    #   2. unterminated string quote -> exit 65 (source error)
    #   3. oversized whole-number literal -> exit 65 (source error)
    #   4. arbitrary non-MLBC bytes to run -> exit 66 (file-format error)
    #   5. truncated valid-prefix artifact -> exit 66 (file-format error)
    #   6. unrecognized verb -> exit 64 (usage error)
    #   Independently re-verified cases 1, 2, 3, and 6 directly against the release binary.
