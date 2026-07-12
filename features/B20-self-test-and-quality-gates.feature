Feature: B20 — Self-test suite and quality gates
  As a developer verifying the delivered implementation is sound and reproducible
  I want a single documented test command with adequate coverage, clean formatting/lint gates, and confirmed determinism
  So that the implementation is trustworthy on top of B1-B19

  # Builds on B1-B19. The internal test framework, test-suite layout/organization, and the
  # specific mechanism used to guarantee determinism are not observable/prescribed — only
  # that the suite exists, runs via one documented command, covers the required categories,
  # and that all the gate checks and the determinism check pass.

  Scenario: E1 — a single documented command runs a test suite covering all five required categories
    Given the project's documented test command
    When it is run
    Then it completes successfully, and specific named tests exist covering the reader (comment handling, dotted-pair reading), a real bytecode round trip through a file written to and read back from disk, closures sharing a captured variable, tail-call recursion reaching real depth without growing memory, and one example of each of the five established exit-code outcomes

  Scenario: E2 — the formatting check produces no differences
    Given the project's formatting check
    When it is run
    Then it passes with no differences

  Scenario: E3 — the linter with its default rule set produces no warnings
    Given the project's linter run across all targets and features
    When it is run
    Then it reports no warnings

  Scenario: E4 — the project builds on stable Rust, uses only std-library runtime dependencies, and contains no unsafe code
    Given the stable Rust toolchain, the project's dependency manifest, and the source tree
    When each is inspected
    Then the build succeeds on stable, no runtime dependencies beyond the standard library are declared, and unsafe code is forbidden at compile time in both crate roots

  Scenario: E5 — compiling the same source twice produces byte-identical output, for macro-free and macro-using files alike
    Given a macro-free source file and a macro-using source file (using gensym during expansion), each compiled twice
    When the two resulting artifacts are compared
    Then both pairs are byte-for-byte identical

  Scenario: E6 — integration: the documented test command, both quality gates, and the determinism check all hold together
    Given the single documented test command, the formatting check, the linter, and the double-compile determinism check
    When each is run
    Then the test command completes successfully with all five required categories present, the formatting check passes with no differences, the linter reports no warnings, and a sample program compiled twice yields byte-identical output
