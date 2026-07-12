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

  Scenario: E2 — each record carries that expectation's evidence in full
    Given a record in the traceability store
    When its evidence is compared against what the .feature file's comment used to say
    Then the full original evidence text is present, not a summary or paraphrase

  Scenario: E3 — each record references the specific .feature file and scenario that crystallises it
    Given a record in the traceability store
    When its "Feature file" and "Scenario" reference is followed
    Then it points to the exact .feature file and the exact scenario title that expectation corresponds to

  Scenario: E4 — the .feature files' inline evidence comments are cleaned out, scenarios unchanged
    Given every features/*.feature file after migration
    When it is checked for leftover evidence comments and for scenario count
    Then no "# Evidence" marker remains anywhere in features/, and the Given/When/Then content and scenario count are unchanged from before the migration

  Scenario: E5 — the migration is complete and lossless
    Given every expectation that had evidence before the migration
    When the store is checked against that original set
    Then every one is represented, none dropped, and no evidence text lost or altered in the move

  Scenario: E6 — integration: the traceability system is navigable and the BDD suites still execute
    Given the traceability store and the migrated .feature files together
    When a representative cross-section is followed from record to scenario, and the full BDD suite is re-run
    Then every followed reference resolves correctly and the full feature-scenario suite still executes green with no evidence comments left behind
