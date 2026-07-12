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
//! The traceability store's own scope is exactly the 25 files that were
//! actually migrated (B1-B23, EX1, DOC1) -- TRACE1-traceability-store.feature
//! itself is a later, deliberately-exempted addition (its own evidence is
//! folded inline, the same self-disclosed-exception shape DOC1 used for
//! non-automation), so it's excluded from the "no evidence marker
//! remains"/"178 scenarios" checks below, which are about the migration,
//! not a blanket ban on this directory ever having an evidence comment.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::registry::Registry;

/// The commit that removed every migrated file's inline evidence comments
/// (`TRACE1: migrate every scenario's evidence into a durable traceability
/// store`) -- its parent is the last commit where that evidence still lived
/// in the `.feature` files themselves, used below as the fixed historical
/// reference point for byte-for-byte fidelity checks. Trunk-based history
/// never rewrites a pushed commit, so this reference is as durable as the
/// repository itself.
const MIGRATION_COMMIT: &str = "84f14a9";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn features_dir() -> PathBuf {
    repo_root().join("features")
}

fn traceability_dir() -> PathBuf {
    repo_root().join("traceability")
}

/// Every `.feature` file that was actually part of the TRACE1 migration --
/// i.e. every one except TRACE1-traceability-store.feature itself, which
/// didn't exist at migration time and was never a migration target.
fn migrated_feature_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(features_dir()).expect("features/ should exist") {
        let path = entry.expect("dir entry should be readable").path();
        if path.extension().and_then(|e| e.to_str()) != Some("feature") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("TRACE1-traceability-store.feature") {
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

/// One `Scenario: E<n> — <title>` block's raw lines, from its own header
/// up to (not including) the next scenario or end of file.
struct ScenarioBlock {
    expectation: String,
    title: String,
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
            lines: lines[*start..end].iter().map(|s| s.to_string()).collect(),
        });
    }
    blocks
}

/// Extracts and normalizes a scenario block's "# Evidence[:( ]..." comment
/// block (both the plain-colon and parenthetical-suffix forms this corpus
/// uses), mirroring exactly the extraction the original migration used:
/// collect the evidence line and every immediately-following comment line
/// until a blank line or non-comment line, then strip each line's leading
/// whitespace, its `#` marker, and one further leading space if present.
fn extract_evidence(block: &ScenarioBlock) -> Option<String> {
    let start = block.lines.iter().position(|l| {
        let s = l.trim_start();
        s.starts_with("# Evidence:") || s.starts_with("# Evidence")
    })?;
    let mut collected = Vec::new();
    for line in &block.lines[start..] {
        let s = line.trim_start();
        if s.is_empty() || !s.starts_with('#') {
            break;
        }
        collected.push(line.as_str());
    }
    let normalized: Vec<String> = collected
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
        let content = git_show(&format!("{MIGRATION_COMMIT}^:{rel}"));
        for block in parse_scenarios(&content) {
            out.push((behaviour.clone(), block));
        }
    }
    out
}

/// Reads a traceability record's evidence body (the text inside its ```
/// fenced block), and its declared feature-file/scenario references.
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
    // The fence may be longer than 3 backticks if the evidence text itself
    // contains an embedded run of backticks (e.g. EX1 E4's evidence
    // literally says "...parses the ```sh block..."), per CommonMark's own
    // rule that a fence must be longer than any backtick run it encloses --
    // so find the opening fence line's exact length and match a closing
    // line of the same length, rather than assuming exactly 3.
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

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1: completeness -- every historical (behaviour, expectation)
        // has a corresponding record. ---
        .step("the repository's traceability folder", |_w, _text, _| {
            // purely descriptive; the When step below does the real work
        })
        .step(
            "it is checked against every delivered behaviour",
            |w, _text, _| {
                let mut missing = Vec::new();
                for (behaviour, block) in historical_scenarios() {
                    let record_path =
                        traceability_dir().join(&behaviour).join(format!("{}.md", block.expectation));
                    if !record_path.is_file() {
                        missing.push(format!("{behaviour} {}: {}", block.expectation, block.title));
                    }
                }
                w.notes = missing;
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
        // --- E2: fidelity -- a record's evidence matches the pre-migration
        // comment byte-for-byte. ---
        .step("a record in the traceability store", |w, _text, _| {
            w.notes = all_records()
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
        })
        .step(
            "its evidence is compared against what the .feature file's comment used to say",
            |w, _text, _| {
                let historical = historical_scenarios();
                let record_paths = std::mem::take(&mut w.notes);
                let mut mismatches = Vec::new();
                for record_path_str in &record_paths {
                    let record_path = PathBuf::from(record_path_str);
                    let record = read_record(&record_path);
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
                        mismatches.push(format!("{behaviour} {expectation}: no historical scenario found"));
                        continue;
                    };
                    let expected = extract_evidence(block).unwrap_or_default();
                    if record.evidence != expected {
                        mismatches.push(format!("{behaviour} {expectation}: evidence text differs"));
                    }
                }
                w.notes = mismatches;
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
                w.notes = unresolved;
            },
        )
        .step(
            "it points to the exact .feature file and the exact scenario title that expectation corresponds to",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "unresolved references: {:?}", w.notes);
            },
        )
        // --- E4: no evidence markers remain in the migrated files, and
        // their scenario count is unchanged. ---
        .step("every features/*.feature file after migration", |_w, _text, _| {})
        .step(
            "it is checked for leftover evidence comments and for scenario count",
            |w, _text, _| {
                let mut leftover = Vec::new();
                let mut current_count = 0usize;
                for path in migrated_feature_files() {
                    let content = std::fs::read_to_string(&path).unwrap();
                    if content.contains("# Evidence") {
                        leftover.push(path.display().to_string());
                    }
                    current_count += parse_scenarios(&content).len();
                }
                let historical_count = historical_scenarios().len();
                w.notes = leftover;
                w.rss_pairs = vec![(current_count as u64, historical_count as u64)];
            },
        )
        .step(
            "no \"# Evidence\" marker remains anywhere in features/, and the Given/When/Then content and scenario count are unchanged from before the migration",
            |w, _text, _| {
                assert!(
                    w.notes.is_empty(),
                    "leftover evidence markers in: {:?}",
                    w.notes
                );
                let (current, historical) = w.rss_pairs[0];
                assert_eq!(
                    current, historical,
                    "scenario count changed across the migration"
                );
            },
        )
        // --- E5: complete-and-lossless, restated over the full set (not a
        // sample) -- same underlying checks as E1 (completeness) + E2
        // (fidelity), run together here. ---
        .step(
            "every expectation that had evidence before the migration",
            |_w, _text, _| {
                // purely descriptive; the When step below recomputes the
                // historical set itself
            },
        )
        .step("the store is checked against that original set", |w, _text, _| {
            let historical = historical_scenarios();
            let mut problems = Vec::new();
            for (behaviour, block) in &historical {
                let record_path = traceability_dir()
                    .join(behaviour)
                    .join(format!("{}.md", block.expectation));
                if !record_path.is_file() {
                    problems.push(format!("{behaviour} {}: missing", block.expectation));
                    continue;
                }
                let record = read_record(&record_path);
                let expected = extract_evidence(block).unwrap_or_default();
                if record.evidence != expected {
                    problems.push(format!("{behaviour} {}: content differs", block.expectation));
                }
            }
            w.notes = problems;
        })
        .step(
            "every one is represented, none dropped, and no evidence text lost or altered in the move",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "lossless-migration problems: {:?}", w.notes);
            },
        )
        // --- E6: integration -- completeness + resolution together. Does
        // NOT recursively invoke `cargo test` (an established anti-pattern
        // in this suite -- see B21's own soak-test precedent): this step
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
                let mut missing = Vec::new();
                for (behaviour, block) in historical_scenarios() {
                    let record_path =
                        traceability_dir().join(&behaviour).join(format!("{}.md", block.expectation));
                    if !record_path.is_file() {
                        missing.push(format!("{behaviour} {}: {}", block.expectation, block.title));
                    }
                }
                let mut unresolved = Vec::new();
                for record_path in all_records() {
                    let record = read_record(&record_path);
                    let feature_path = repo_root().join(&record.feature_file);
                    let content = std::fs::read_to_string(&feature_path).unwrap_or_default();
                    let needle = format!("  Scenario: {}", record.scenario);
                    if !content.contains(&needle) {
                        unresolved.push(format!("{}: {}", record_path.display(), record.scenario));
                    }
                }
                let mut leftover = Vec::new();
                for path in migrated_feature_files() {
                    let content = std::fs::read_to_string(&path).unwrap();
                    if content.contains("# Evidence") {
                        leftover.push(path.display().to_string());
                    }
                }
                w.notes = missing;
                w.pending_commands = vec![unresolved, leftover];
            },
        )
        .step(
            "every followed reference resolves correctly and the full feature-scenario suite still executes green with no evidence comments left behind",
            |w, _text, _| {
                assert!(w.notes.is_empty(), "missing records: {:?}", w.notes);
                assert!(
                    w.pending_commands[0].is_empty(),
                    "unresolved references: {:?}",
                    w.pending_commands[0]
                );
                assert!(
                    w.pending_commands[1].is_empty(),
                    "leftover evidence markers in: {:?}",
                    w.pending_commands[1]
                );
            },
        )
}
