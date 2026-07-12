Feature: TRACE1 — durable traceability store for expectations and evidence
  As a reader auditing the project's delivered behaviours
  I want expectations and their evidence to live as durable, first-class records in the
  repository, each pointing back to the exact executable scenario that proves it
  So that I can trace expectation -> evidence -> BDD scenario without reading raw Gherkin
  comments, and evidence is no longer scattered across every .feature file's comments

  # Owner-requested, reopening the closed project (not a frozen-spec behaviour, cites no SPEC
  # section). Folder name/location, on-disk record format, and field layout were left to
  # downstream's choice; delivered as traceability/<Behaviour>/<Expectation>.md, one file per
  # expectation, plus traceability/README.md as a browsable index.

  Scenario: E1 — a durable, committed folder holds one record per expectation, for every behaviour
    Given the repository's traceability folder
    When it is checked against every delivered behaviour
    Then it holds one record for every expectation across all of B1-B23, EX1, and DOC1, without exception
    # Evidence: Examiner independently re-derived every pre-migration "# Evidence" block from
    #   git history (commit 84f14a9^, all 25 pre-migration features/*.feature files) and
    #   confirmed a traceability/<Behaviour>/<Expectation>.md record exists for all 178 --
    #   zero missing.

  Scenario: E2 — each record carries that expectation's evidence in full
    Given a record in the traceability store
    When its evidence is compared against what the .feature file's comment used to say
    Then the full original evidence text is present, not a summary or paraphrase
    # Evidence: Examiner wrote an independent extraction script (not reusing the Builder's) and
    #   byte-for-byte compared all 178 pre-migration evidence blocks against their store
    #   records. All 178 matched exactly, including B1/E5 -- a genuinely tricky case whose
    #   evidence embeds a literal carriage-return byte as test data (mid-line, not a line
    #   break); naive text-mode reads (both the Examiner's first attempt and, per the commit
    #   message, an approach the Builder had to specifically avoid) silently mis-split it, but
    #   a binary-safe re-read confirmed the store's copy is exactly correct.

  Scenario: E3 — each record references the specific .feature file and scenario that crystallises it
    Given a record in the traceability store
    When its "Feature file" and "Scenario" reference is followed
    Then it points to the exact .feature file and the exact scenario title that expectation corresponds to
    # Evidence: Examiner spot-checked a representative cross-section (B1 first, B12 middle,
    #   B23 last-numbered, plus EX1 and DOC1) -- every record's cited scenario title matches
    #   the live scenario in the referenced .feature file exactly.

  Scenario: E4 — the .feature files' inline evidence comments are cleaned out, scenarios unchanged
    Given every features/*.feature file after migration
    When it is checked for leftover evidence comments and for scenario count
    Then no "# Evidence" marker remains anywhere in features/, and the Given/When/Then content and scenario count are unchanged from before the migration
    # Evidence: `grep -rl '# Evidence' features/` returns nothing. Scenario count confirmed
    #   identical before and after (178 both times) by independently counting `Scenario:`
    #   lines in the pre- and post-migration git trees.

  Scenario: E5 — the migration is complete and lossless
    Given every expectation that had evidence before the migration
    When the store is checked against that original set
    Then every one is represented, none dropped, and no evidence text lost or altered in the move
    # Evidence: same 178/178 byte-for-byte match as E2, run against every behaviour (not a
    #   sample) -- this is the complete-and-lossless claim directly, not just illustrated by
    #   one example.

  Scenario: E6 — integration: the traceability system is navigable and the BDD suites still execute
    Given the traceability store and the migrated .feature files together
    When a representative cross-section is followed from record to scenario, and the full BDD suite is re-run
    Then every followed reference resolves correctly and the full feature-scenario suite still executes green with no evidence comments left behind
    # Evidence: `cargo test --release --test features` (all 31 registered scenario groups,
    #   spanning B1-B23 and EX1) -- 31 passed, 0 failed, 1 ignored (DOC1, which has no
    #   automated runner by design), finished in 213.11s. Cross-section reference check as
    #   in E3. Independently re-run by the Examiner against commit 84f14a9.
