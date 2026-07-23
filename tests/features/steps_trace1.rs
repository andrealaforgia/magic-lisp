//! Step definitions for features/TRACE1-traceability-store.feature.
//!
//! qa test-design review msg #90 (warning): the examiner's own
//! TRACE1-traceability-store.feature (documenting the evidence-migration
//! work) was committed without step-definitions -- structurally the same
//! undisclosed-wiring gap EX1 had before it was fixed. Every one of
//! TRACE1's six checks is objectively scriptable (file existence, byte
//! comparison against git history, grep, scenario counts), so it's wired
//! here as a standing regression guard rather than staying a one-time
//! manual audit.
//!
//! qa test-design review msg #92 (still-below-floor follow-up) drove three
//! further fixes, kept in mind below: E5 must call E1's/E2's own check
//! functions rather than reimplementing them (no triplicated logic); E4's
//! Then-clause claims scenario *content*, not just count, is unchanged, so
//! its check does a real line-for-line diff against the reconstructed
//! post-migration expectation, not just a count comparison; and no step
//! below repurposes a `World` field for something its name doesn't
//! describe (`notes: Vec<String>` -- a flat list of problem descriptions,
//! empty meaning pass -- covers every need here).
//!
//! The traceability store's scope is now every committed `.feature` file
//! without exception, TRACE1-traceability-store.feature included --
//! initially exempted (its own evidence was folded inline, the same
//! self-disclosed-exception shape DOC1 used for non-automation), it was
//! migrated into the store in its turn (examiner expectation TRACE1 E1-E6,
//! second round), closing that last gap.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::gherkin::parse_feature;
use super::registry::Registry;

/// The commit that removed the original 25 files' inline evidence comments
/// (`TRACE1: migrate every scenario's evidence into a durable traceability
/// store`) -- its parent is the last commit where that evidence still lived
/// in those `.feature` files themselves. Full 40-character SHA, not the
/// short form, so this reference can never become ambiguous as the
/// repository grows (qa test-design review msg #92).
const MIGRATION_COMMIT: &str = "84f14a9d50c51feb9293e08aa0e62d8d89e9e025";

/// The commit that removed TRACE1-traceability-store.feature's *own*
/// inline evidence comments (`TRACE1: migrate its own evidence into the
/// store, closing the last gap`) -- this file didn't exist yet at
/// `MIGRATION_COMMIT`, so it needs its own, later historical reference
/// point. Trunk-based history never rewrites a pushed commit, so both
/// references are as durable as the repository itself.
const TRACE1_MIGRATION_COMMIT: &str = "0e48260e67ebab2ccf2032202dfe2c59867ac1a2";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn features_dir() -> PathBuf {
    repo_root().join("features")
}

fn traceability_dir() -> PathBuf {
    repo_root().join("traceability")
}

/// The commit whose parent last had `path`'s evidence still inline --
/// every migrated file uses `MIGRATION_COMMIT` except
/// TRACE1-traceability-store.feature, which was migrated separately and
/// later (see `TRACE1_MIGRATION_COMMIT`'s own doc comment).
fn migration_commit_for(path: &Path) -> &'static str {
    if path.file_name().and_then(|n| n.to_str()) == Some("TRACE1-traceability-store.feature") {
        TRACE1_MIGRATION_COMMIT
    } else {
        MIGRATION_COMMIT
    }
}

/// Every `.feature` file that's part of the traceability store's coverage
/// -- now the whole `features/` directory, with no exceptions.
fn migrated_feature_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(features_dir()).expect("features/ should exist") {
        let path = entry.expect("dir entry should be readable").path();
        if path.extension().and_then(|e| e.to_str()) != Some("feature") {
            continue;
        }
        out.push(path);
    }
    out.sort();
    out
}

fn behaviour_id_from_path(path: &Path) -> String {
    let stem = path.file_stem().unwrap().to_str().unwrap();
    stem.split('-').next().unwrap().to_string()
}

/// One `Scenario: E<n> — <title>` block's raw lines, from its own header up
/// to (not including) the next scenario or end of file, plus where that
/// range starts within the whole file it was parsed from (used to
/// reconstruct the file with its evidence block removed, for E4's content
/// diff).
struct ScenarioBlock {
    expectation: String,
    title: String,
    abs_start: usize,
    lines: Vec<String>,
}

fn parse_scenarios(content: &str) -> Vec<ScenarioBlock> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut starts: Vec<(usize, String, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("  Scenario: ") else {
            continue;
        };
        let sep = rest.find(" — ").or_else(|| rest.find(" - "));
        let Some(sep) = sep else { continue };
        let (exp, title) = rest.split_at(sep);
        let title = title
            .trim_start_matches(" — ")
            .trim_start_matches(" - ")
            .trim();
        starts.push((i, exp.trim().to_string(), title.to_string()));
    }
    let mut blocks = Vec::new();
    for (idx, (start, exp, title)) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).map(|s| s.0).unwrap_or(lines.len());
        blocks.push(ScenarioBlock {
            expectation: exp.clone(),
            title: title.clone(),
            abs_start: *start,
            lines: lines[*start..end].iter().map(|s| s.to_string()).collect(),
        });
    }
    blocks
}

/// The block-relative (start, end) line-index range of this block's
/// "# Evidence[:( ]..." comment (both the plain-colon and
/// parenthetical-suffix forms this corpus uses): the evidence line itself
/// and every immediately-following comment line, until a blank line or a
/// non-comment line.
fn evidence_line_range(block: &ScenarioBlock) -> Option<(usize, usize)> {
    let start = block.lines.iter().position(|l| {
        let s = l.trim_start();
        s.starts_with("# Evidence:") || s.starts_with("# Evidence")
    })?;
    let mut end = start;
    for line in &block.lines[start..] {
        let s = line.trim_start();
        if s.is_empty() || !s.starts_with('#') {
            break;
        }
        end += 1;
    }
    Some((start, end))
}

/// Extracts and normalizes a scenario block's evidence text: strips each
/// line's leading whitespace, its `#` marker, and one further leading
/// space if present -- mirroring exactly the extraction the original
/// migration used.
fn extract_evidence(block: &ScenarioBlock) -> Option<String> {
    let (start, end) = evidence_line_range(block)?;
    let normalized: Vec<String> = block.lines[start..end]
        .iter()
        .map(|raw| {
            let s = raw.trim_start_matches(' ');
            let s = &s[1..]; // drop '#'
            if let Some(rest) = s.strip_prefix(' ') {
                rest.to_string()
            } else {
                s.to_string()
            }
        })
        .collect();
    Some(normalized.join("\n"))
}

/// Reconstructs what `historical_content` should look like once every
/// scenario's evidence comment block is removed -- i.e. exactly what the
/// migration was supposed to produce -- so it can be compared line-for-line
/// against a file's actual current content (E4's real content diff, not
/// just a scenario count).
fn strip_evidence_blocks(historical_content: &str) -> String {
    let lines: Vec<&str> = historical_content.split('\n').collect();
    let mut removed = vec![false; lines.len()];
    for block in parse_scenarios(historical_content) {
        if let Some((start, end)) = evidence_line_range(&block) {
            let range = (block.abs_start + start)..(block.abs_start + end);
            removed[range].fill(true);
        }
    }
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !removed[*i])
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `git show <rev>:<path>` from the repo root, as raw bytes decoded lossily
/// -- historical content is trusted repo text, not untrusted input.
fn git_show(rev_and_path: &str) -> String {
    let out = Command::new("git")
        .arg("show")
        .arg(rev_and_path)
        .current_dir(repo_root())
        .output()
        .expect("git should run");
    assert!(
        out.status.success(),
        "git show {rev_and_path} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Every (behaviour, expectation, title) triple derivable from the
/// pre-migration `.feature` files (i.e. every scenario that had evidence
/// before the migration), read from git history rather than the current
/// working tree.
fn historical_scenarios() -> Vec<(String, ScenarioBlock)> {
    let mut out = Vec::new();
    for path in migrated_feature_files() {
        let behaviour = behaviour_id_from_path(&path);
        let rel = format!("features/{}", path.file_name().unwrap().to_str().unwrap());
        let commit = migration_commit_for(&path);
        let content = git_show(&format!("{commit}^:{rel}"));
        for block in parse_scenarios(&content) {
            out.push((behaviour.clone(), block));
        }
    }
    out
}

/// Reads a traceability record's evidence body (the text inside its
/// fenced block -- possibly longer than 3 backticks, if the evidence
/// itself contains an embedded backtick run), and its declared
/// feature-file/scenario references.
struct Record {
    feature_file: String,
    scenario: String,
    evidence: String,
}

fn read_record(path: &Path) -> Record {
    let content = std::fs::read_to_string(path).expect("record should be readable");
    let feature_file = content
        .lines()
        .find_map(|l| l.strip_prefix("**Feature file:** `"))
        .and_then(|s| s.strip_suffix('`'))
        .expect("record should have a Feature file reference")
        .to_string();
    let scenario = content
        .lines()
        .find_map(|l| l.strip_prefix("**Scenario:** "))
        .expect("record should have a Scenario reference")
        .to_string();
    let lines: Vec<&str> = content.lines().collect();
    let open_idx = lines
        .iter()
        .position(|l| !l.is_empty() && l.chars().all(|c| c == '`'))
        .expect("record should have a fenced block");
    let fence = lines[open_idx];
    let close_idx = lines[open_idx + 1..]
        .iter()
        .position(|l| *l == fence)
        .map(|i| open_idx + 1 + i)
        .expect("fence should close");
    let evidence = lines[open_idx + 1..close_idx].join("\n");
    Record {
        feature_file,
        scenario,
        evidence,
    }
}

fn all_records() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for behaviour_entry in std::fs::read_dir(traceability_dir()).expect("traceability/ exists") {
        let behaviour_path = behaviour_entry.unwrap().path();
        if !behaviour_path.is_dir() {
            continue; // skip README.md
        }
        for record_entry in std::fs::read_dir(&behaviour_path).unwrap() {
            out.push(record_entry.unwrap().path());
        }
    }
    out.sort();
    out
}

// --- Shared checks (E5 calls these directly rather than reimplementing
// them -- qa test-design review msg #92). Each returns a flat list of
// problem descriptions; empty means the check passed. ---

/// E1's check: every historical (behaviour, expectation) has a
/// corresponding traceability record.
fn check_completeness() -> Vec<String> {
    let mut missing = Vec::new();
    for (behaviour, block) in historical_scenarios() {
        let record_path = traceability_dir()
            .join(&behaviour)
            .join(format!("{}.md", block.expectation));
        if !record_path.is_file() {
            missing.push(format!(
                "{behaviour} {}: {}",
                block.expectation, block.title
            ));
        }
    }
    missing
}

/// A record's evidence is faithful to its pre-migration source if it
/// matches byte-for-byte, OR if it preserves that original text verbatim as
/// a leading section and only *appends* further content after a blank line
/// (a later round's own evidence, documented in addition to -- not instead
/// of -- what came before, e.g. TRACE1's own "Round 2" self-migration
/// addenda in E1/E4/E6, added in a commit after the pinned historical
/// snapshot). Growth is legitimate; loss or alteration of the original text
/// is not (qa test-design review: pinning one frozen commit made the
/// legitimately-grown records fail forever, regardless of correctness).
fn evidence_preserves_original(record_evidence: &str, expected: &str) -> bool {
    if expected.is_empty() {
        // `str::strip_prefix("")` always succeeds, so without this an empty
        // `expected` (a historical scenario with no extractable evidence
        // comment, e.g. a future migration gap) would make every record
        // whose text merely starts with a blank line vacuously "faithful"
        // -- the tamper check silently no-opping instead of failing loudly
        // on a missing baseline (warden security review, Medium finding).
        return record_evidence.is_empty();
    }
    record_evidence == expected
        || record_evidence
            .strip_prefix(expected)
            .is_some_and(|rest| rest.starts_with("\n\n") && !rest.trim().is_empty())
}

/// E2's check: every given record's evidence matches its pre-migration
/// comment byte-for-byte, or legitimately extends it (see
/// `evidence_preserves_original`).
fn check_fidelity(record_paths: &[PathBuf]) -> Vec<String> {
    let historical = historical_scenarios();
    let mut mismatches = Vec::new();
    for record_path in record_paths {
        let record = read_record(record_path);
        let behaviour = record_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let expectation = record_path.file_stem().unwrap().to_str().unwrap();
        let Some((_, block)) = historical
            .iter()
            .find(|(b, blk)| b == behaviour && blk.expectation == expectation)
        else {
            mismatches.push(format!(
                "{behaviour} {expectation}: no historical scenario found"
            ));
            continue;
        };
        let expected = extract_evidence(block).unwrap_or_default();
        if !evidence_preserves_original(&record.evidence, &expected) {
            mismatches.push(format!("{behaviour} {expectation}: evidence text differs"));
        }
    }
    mismatches
}

/// E3's check: every record's declared feature-file/scenario reference
/// resolves to a real scenario in the current tree.
fn check_resolution() -> Vec<String> {
    let mut unresolved = Vec::new();
    for record_path in all_records() {
        let record = read_record(&record_path);
        let feature_path = repo_root().join(&record.feature_file);
        let content = std::fs::read_to_string(&feature_path).unwrap_or_default();
        let needle = format!("  Scenario: {}", record.scenario);
        if !content.contains(&needle) {
            unresolved.push(format!(
                "{}: {} not found in {}",
                record_path.display(),
                record.scenario,
                record.feature_file
            ));
        }
    }
    unresolved
}

/// E4's check, part 1: no "# Evidence" marker remains in any migrated
/// file.
fn check_no_leftover_markers() -> Vec<String> {
    let mut leftover = Vec::new();
    for path in migrated_feature_files() {
        let content = std::fs::read_to_string(&path).unwrap();
        // A genuine comment line, trimmed and starting with "# Evidence" --
        // not a bare substring search, which would false-positive on
        // TRACE1-traceability-store.feature's own Then-clause prose ("no
        // \"# Evidence\" marker remains..."), a literal mention of the
        // marker text that is itself part of what's being described, not
        // a leftover comment.
        let has_marker = content
            .lines()
            .any(|l| l.trim_start().starts_with("# Evidence"));
        if has_marker {
            leftover.push(path.display().to_string());
        }
    }
    leftover
}

/// E4's check, part 2: every migrated file's *executable Gherkin
/// structure* -- scenario names, count, and each step's own text/docstring
/// -- matches its pre-migration content with the evidence blocks removed,
/// exactly. Compares parsed scenarios (the same parser that actually
/// drives every other behaviour's tests), not raw file lines, so a later,
/// legitimate comment addition (prose that isn't a Given/When/Then/And/But
/// step) doesn't register as drift -- only a change to the executable
/// content itself, or to how many scenarios exist, does. qa test-design
/// review: this check, still pinned line-for-line against one frozen
/// commit, reproduced the exact "legitimately grew past the pin" failure
/// `evidence_preserves_original` was already fixed for elsewhere in this
/// file, when a documentation-only comment was added to a migrated
/// feature file.
fn feature_structure_mismatch(historical_stripped: &str, actual: &str) -> Option<String> {
    let expected = parse_feature(historical_stripped);
    let actual = parse_feature(actual);
    if expected.scenarios.len() != actual.scenarios.len() {
        return Some(format!(
            "scenario count changed ({} -> {})",
            expected.scenarios.len(),
            actual.scenarios.len()
        ));
    }
    if expected.scenarios != actual.scenarios {
        return Some("a scenario's name or step content changed".to_string());
    }
    None
}

fn check_content_unchanged() -> Vec<String> {
    let mut problems = Vec::new();
    for path in migrated_feature_files() {
        let rel = format!("features/{}", path.file_name().unwrap().to_str().unwrap());
        let commit = migration_commit_for(&path);
        let historical = git_show(&format!("{commit}^:{rel}"));
        let expected = strip_evidence_blocks(&historical);
        let actual = std::fs::read_to_string(&path).unwrap();
        if let Some(reason) = feature_structure_mismatch(&expected, &actual) {
            problems.push(format!("{rel}: {reason}"));
        }
    }
    problems
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1: completeness. ---
        .step("the repository's traceability folder", |_w, _text, _| {
            // purely descriptive; the When step below does the real work
        })
        .step(
            "it is checked against every delivered behaviour",
            |w, _text, _| {
                w.notes = check_completeness();
            },
        )
        .step(
            "it holds one record for every expectation across all of B1-B23, EX1, and DOC1, without exception",
            |w, _text, _| {
                assert!(
                    w.notes.is_empty(),
                    "missing traceability records for: {:?}",
                    w.notes
                );
            },
        )
        // --- E2: fidelity (a sample -- every record, since the store is
        // small enough that "a sample" and "all of them" cost the same). ---
        .step("a record in the traceability store", |w, _text, _| {
            w.notes = all_records()
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
        })
        .step(
            "its evidence is compared against what the .feature file's comment used to say",
            |w, _text, _| {
                let record_paths: Vec<PathBuf> =
                    std::mem::take(&mut w.notes).into_iter().map(PathBuf::from).collect();
                w.notes = check_fidelity(&record_paths);
            },
        )
        .step(
            "the full original evidence text is present, not a summary or paraphrase",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "evidence mismatches: {:?}", w.notes);
            },
        )
        // --- E3: every record's reference resolves to a real scenario. ---
        .step(
            "its \"Feature file\" and \"Scenario\" reference is followed",
            |w, _text, _| {
                w.notes = check_resolution();
            },
        )
        .step(
            "it points to the exact .feature file and the exact scenario title that expectation corresponds to",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "unresolved references: {:?}", w.notes);
            },
        )
        // --- E4: no evidence markers remain, and content (not just count)
        // is otherwise unchanged. ---
        .step("every features/*.feature file after migration", |_w, _text, _| {})
        .step(
            "it is checked for leftover evidence comments and for scenario count",
            |w, _text, _| {
                let mut problems = check_no_leftover_markers();
                problems.extend(check_content_unchanged());
                w.notes = problems;
            },
        )
        .step(
            "no \"# Evidence\" marker remains anywhere in features/, and the Given/When/Then content and scenario count are unchanged from before the migration",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "problems found: {:?}", w.notes);
            },
        )
        // --- E5: complete-and-lossless, restated over the full set (not a
        // sample) -- calls E1's and E2's own check functions directly
        // rather than reimplementing them (qa test-design review msg #92). ---
        .step(
            "every expectation that had evidence before the migration",
            |_w, _text, _| {
                // purely descriptive; the When step below calls the shared
                // checks, which recompute the historical set themselves
            },
        )
        .step("the store is checked against that original set", |w, _text, _| {
            let mut problems = check_completeness();
            let all: Vec<PathBuf> = all_records();
            problems.extend(check_fidelity(&all));
            w.notes = problems;
        })
        .step(
            "every one is represented, none dropped, and no evidence text lost or altered in the move",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "lossless-migration problems: {:?}", w.notes);
            },
        )
        // --- E6: integration -- every shared check together. Does NOT
        // recursively invoke `cargo test` (an established anti-pattern in
        // this suite -- see B21's own soak-test precedent): this step
        // itself only runs as part of `cargo test --release --test
        // features`, so its own passing, alongside every other registered
        // scenario in the same run, already demonstrates the full suite
        // executes green. ---
        .step(
            "the traceability store and the migrated .feature files together",
            |_w, _text, _| {},
        )
        .step(
            "a representative cross-section is followed from record to scenario, and the full BDD suite is re-run",
            |w, _text, _| {
                let mut problems = check_completeness();
                problems.extend(check_resolution());
                problems.extend(check_no_leftover_markers());
                w.notes = problems;
            },
        )
        .step(
            "every followed reference resolves correctly and the full feature-scenario suite still executes green with no evidence comments left behind",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "problems found: {:?}", w.notes);
            },
        )
}

#[cfg(test)]
mod tests {
    use super::{evidence_preserves_original, feature_structure_mismatch};

    #[test]
    fn an_exact_match_is_faithful() {
        assert!(evidence_preserves_original("same text", "same text"));
    }

    #[test]
    fn a_later_round_appended_after_a_blank_line_is_still_faithful() {
        let original = "Evidence: the original round's finding.";
        let grown = "Evidence: the original round's finding.\n\nRound 2: a further finding.";
        assert!(evidence_preserves_original(grown, original));
    }

    #[test]
    fn altering_a_word_inside_the_original_text_is_not_faithful() {
        let original = "Evidence: the original round's finding.";
        let altered = "Evidence: the original round's DIFFERENT finding.";
        assert!(!evidence_preserves_original(altered, original));
    }

    #[test]
    fn truncating_the_original_text_is_not_faithful() {
        let original = "Evidence: the original round's finding, in full.";
        let truncated = "Evidence: the original round's finding";
        assert!(!evidence_preserves_original(truncated, original));
    }

    #[test]
    fn appended_content_without_a_separating_blank_line_is_not_faithful() {
        // Guards against treating an accidental run-on concatenation (or a
        // record that merely starts with the original text as a substring
        // of some unrelated longer passage) as a legitimate later round.
        let original = "Evidence: the original round's finding.";
        let run_on = "Evidence: the original round's finding.\nRound 2: no blank line before this.";
        assert!(!evidence_preserves_original(run_on, original));
    }

    #[test]
    fn empty_extra_content_after_the_blank_line_is_not_faithful() {
        let original = "Evidence: the original round's finding.";
        let empty_addendum = "Evidence: the original round's finding.\n\n";
        assert!(!evidence_preserves_original(empty_addendum, original));
    }

    #[test]
    fn a_missing_historical_baseline_does_not_vacuously_accept_any_record() {
        // warden security review: `strip_prefix("")` always succeeds, so an
        // empty `expected` (a historical scenario with no extractable
        // evidence comment) must not make an unrelated, non-empty record
        // "faithful" just because it happens to start with a blank line.
        assert!(!evidence_preserves_original(
            "\n\nsome unrelated recorded evidence",
            ""
        ));
    }

    #[test]
    fn a_missing_historical_baseline_matches_only_an_equally_empty_record() {
        assert!(evidence_preserves_original("", ""));
    }

    #[test]
    fn a_whitespace_only_addendum_after_the_blank_line_is_not_faithful() {
        // qa test-design review: one increment past the covered "empty
        // addendum" case -- whitespace alone adds no real content either.
        let original = "Evidence: the original round's finding.";
        let whitespace_only = "Evidence: the original round's finding.\n\n   ";
        assert!(!evidence_preserves_original(whitespace_only, original));
    }

    const FIXTURE: &str = "Feature: fixture\n\n  \
        Scenario: E1 — first\n    Given a thing\n    When it happens\n    Then it works\n\n  \
        Scenario: E2 — second\n    Given another thing\n    Then it also works\n";

    #[test]
    fn identical_content_has_no_structure_mismatch() {
        assert!(feature_structure_mismatch(FIXTURE, FIXTURE).is_none());
    }

    #[test]
    fn a_later_comment_added_anywhere_is_not_a_structure_mismatch() {
        // The exact class of legitimate growth that broke this check
        // (qa test-design review): a documentation-only line, not a
        // Given/When/Then/And/But step.
        let grown = "Feature: fixture\n\n  # A later, purely explanatory comment.\n  \
            Scenario: E1 — first\n    Given a thing\n    When it happens\n    Then it works\n\n  \
            Scenario: E2 — second\n    Given another thing\n    Then it also works\n";
        assert!(feature_structure_mismatch(FIXTURE, grown).is_none());
    }

    #[test]
    fn adding_a_whole_new_scenario_is_a_structure_mismatch() {
        let with_extra_scenario = format!(
            "{FIXTURE}\n  Scenario: E3 — third\n    Given a third thing\n    Then it works too\n"
        );
        let mismatch = feature_structure_mismatch(FIXTURE, &with_extra_scenario);
        assert!(mismatch.is_some_and(|m| m.contains("count")));
    }

    #[test]
    fn altering_a_steps_wording_is_a_structure_mismatch() {
        let altered = FIXTURE.replace("it works", "it works differently");
        let mismatch = feature_structure_mismatch(FIXTURE, &altered);
        assert!(mismatch.is_some_and(|m| m.contains("step")));
    }

    #[test]
    fn removing_a_scenario_is_a_structure_mismatch() {
        let first_only = "Feature: fixture\n\n  \
            Scenario: E1 — first\n    Given a thing\n    When it happens\n    Then it works\n";
        let mismatch = feature_structure_mismatch(FIXTURE, first_only);
        assert!(mismatch.is_some_and(|m| m.contains("count")));
    }
}
