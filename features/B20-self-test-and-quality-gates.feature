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
    # Evidence: $ cargo test --all -> 781 lib + 306 CLI-integration + 27 BDD-feature tests, all passing
    #   Specific tests confirmed to exist and pass:
    #   reader::tests::skips_a_single_block_comment
    #   reader::tests::a_block_comment_fully_containing_another_is_consumed_as_one_outer_comment
    #   reader::tests::reads_a_dotted_pair_with_a_single_fixed_head_item
    #   b1::e2_compile_then_run_reproduces_eval_output_across_process_boundaries (real file round trip)
    #   b5::b5_e2_mutating_a_captured_variable_through_one_closure_is_visible_through_another
    #   b6::b6_e1_self_tail_call_loop_counts_to_ten_million (plus the BDD suite's own RSS-flatness assertions)
    #   b1::e8_success_exit_code_for_a_valid_program
    #   b1::e8_usage_error_exit_code_for_a_missing_required_argument
    #   b1::e8_source_error_exit_code_for_unreadable_source
    #   b1::e8_bad_artifact_exit_code_for_a_corrupt_artifact
    #   b1::e8_runtime_error_exit_code_for_an_undefined_global
    #   Independently confirmed all 11 named tests actually exist in the source tree (not fabricated).
    #   Note: this scenario's own BDD verification test is marked #[ignore] by default — it
    #   recursively spawns a full nested `cargo test --all` (several minutes, a rebuilt isolated
    #   target directory), three to four orders of magnitude slower than the rest of the BDD
    #   suite combined (qa test-design warning). Run it explicitly before a release or in a
    #   dedicated CI job: `cargo test --test features -- --ignored b20_self_test`.
    #   Independently re-verified: still passes when invoked explicitly (335.55s).

  Scenario: E2 — the formatting check produces no differences
    Given the project's formatting check
    When it is run
    Then it passes with no differences
    # Evidence: $ cargo fmt --check -> no output, exit 0
    #   Independently re-verified.

  Scenario: E3 — the linter with its default rule set produces no warnings
    Given the project's linter run across all targets and features
    When it is run
    Then it reports no warnings
    # Evidence: $ cargo clippy --all-targets --all-features -> no warnings, exit 0
    #   Independently re-verified.

  Scenario: E4 — the project builds on stable Rust, uses only std-library runtime dependencies, and contains no unsafe code
    Given the stable Rust toolchain, the project's dependency manifest, and the source tree
    When each is inspected
    Then the build succeeds on stable, no runtime dependencies beyond the standard library are declared, and unsafe code is forbidden at compile time in both crate roots
    # Evidence: $ rustc --version -> a stable release (no nightly-only features required)
    #   $ cargo build --release -> succeeds
    #   Cargo.toml's [dependencies] section is empty; `cargo tree` shows no dependencies
    #   src/lib.rs:1 and src/main.rs:1 both declare #![forbid(unsafe_code)]

  Scenario: E5 — compiling the same source twice produces byte-identical output, for macro-free and macro-using files alike
    Given a macro-free source file and a macro-using source file (using gensym during expansion), each compiled twice
    When the two resulting artifacts are compared
    Then both pairs are byte-for-byte identical
    # Evidence: macro-free file compiled twice -> diff shows no output, sha256 hashes match
    #   macro-using file (define-macro swap! using gensym) compiled twice -> also byte-identical
    #   (gensym's counter resets deterministically per compile, so macro expansion doesn't
    #   introduce nondeterminism either)
    #   Independently re-verified both cases against the release binary.

  Scenario: E6 — integration: the documented test command, both quality gates, and the determinism check all hold together
    Given the single documented test command, the formatting check, the linter, and the double-compile determinism check
    When each is run
    Then the test command completes successfully with all five required categories present, the formatting check passes with no differences, the linter reports no warnings, and a sample program compiled twice yields byte-identical output
    # Evidence: as demonstrated in E1-E5 above, all confirmed together in one review pass.
