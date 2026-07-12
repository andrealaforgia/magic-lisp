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

  Scenario: E2 — an invalid outer-container artifact always ends with the file-format-error exit code
    Given an artifact with non-zero flags where none are allowed, handed to both run and disasm
    When each is run
    Then each ends with the file-format-error exit code

  Scenario: E3 — a valid-outer-container but internally-broken artifact is reported cleanly, never a crash
    Given an artifact with an undefined opcode byte, an out-of-range constant-pool index, and an instruction truncated mid-operand — each corrupting only internal code/constant bytes, not the header
    When each is run (and, for the undefined-opcode case, also disassembled)
    Then each is reported as a runtime error or file-format error (either acceptable) with no crash, and disasm may instead gracefully label an unrecognized opcode and continue rather than erroring, since exit 0 with no crash also satisfies "always lands on an established outcome"

  Scenario: E4 — command-line misuse always ends with the usage-error exit code
    Given an unrecognized verb and a required argument left off
    When each is run
    Then each ends with the usage-error exit code

  Scenario: E5 — a genuine breadth sweep across many malformed inputs never crashes or exits outside the established set
    Given dozens of malformed source snippets spanning many categories beyond E1's five, and every possible single-byte corruption of a valid compiled artifact run through both run and disasm
    When the full sweep is executed
    Then every single run exits on an established exit code with no signal-killed/crash termination

  Scenario: E6 — integration: all six verbatim demo cases exit with exactly the prescribed code
    Given each of the six DEMO cases from the behaviour spec
    When each is run
    Then each exits with exactly its prescribed established exit code, with no crash output
