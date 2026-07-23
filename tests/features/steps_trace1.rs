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

/// Whether `rev_and_path` (e.g. `"84f14a9^:features/B24-foo.feature"`)
/// resolves to a real blob -- distinguishes "this file predates the
/// migration and has real historical content to check" from "this file
/// was born after the migration and was never part of what it covers."
/// `historical_scenarios`/`check_content_unchanged` must skip the latter
/// rather than hard-fail on it: a brand-new `.feature` file has no
/// pre-migration inline evidence to lose or alter in the first place
/// (examiner msgs #464/#469: reproduced live, a new file's non-existence
/// at the pinned commit made `git show` fail and panicked five scenarios).
fn existed_at(rev_and_path: &str) -> bool {
    existed_at_in(&repo_root(), rev_and_path)
}

/// [`existed_at`], against an arbitrary git working directory rather than
/// always this project's own -- lets a test exercise the real check
/// against a small, hermetic, throwaway repo (qa test-design review: the
/// only way to prove the fallback/skip logic works without waiting for,
/// or polluting, this project's own history with a real post-migration
/// file).
fn existed_at_in(dir: &Path, rev_and_path: &str) -> bool {
    Command::new("git")
        .arg("cat-file")
        .arg("-e")
        .arg(rev_and_path)
        .current_dir(dir)
        // Suppresses git's own "fatal: path ... does not exist" on the
        // expected-false path -- that's this function's normal, checked
        // return value, not a test failure, and shouldn't read like one in
        // CI output (qa test-design review).
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git should run")
        .success()
}

/// The first commit (following renames) that added `rel` to the
/// repository -- `rel`'s own introducing commit, used as its content
/// baseline when it postdates both fixed migration commits (warden
/// security review, High: skipping such a file's checks entirely, rather
/// than deriving its own baseline, permanently exempted every future
/// `.feature` file from ever having its content compared against
/// anything, once its evidence was accepted).
fn first_commit_introducing(rel: &str) -> String {
    first_commit_introducing_in(&repo_root(), rel)
}

fn first_commit_introducing_in(dir: &Path, rel: &str) -> String {
    let out = Command::new("git")
        .args([
            "log",
            "--follow",
            "--diff-filter=A",
            "--format=%H",
            "--reverse",
            "--",
            rel,
        ])
        .current_dir(dir)
        .output()
        .expect("git should run");
    assert!(
        out.status.success(),
        "git log --follow failed for {rel}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or_else(|| panic!("{rel}: no commit in this repository's history ever added it"))
        .to_string()
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
        let rel = format!("features/{}", path.file_name().unwrap().to_str().unwrap());
        let commit = migration_commit_for(&path);
        let rev_and_path = format!("{commit}^:{rel}");
        if !existed_at(&rev_and_path) {
            continue;
        }
        let behaviour = behaviour_id_from_path(&path);
        let content = git_show(&rev_and_path);
        for block in parse_scenarios(&content) {
            out.push((behaviour.clone(), block));
        }
    }
    out
}

/// Every (behaviour, expectation, title) triple in every CURRENTLY
/// committed `.feature` file, read straight from the working tree --
/// unlike `historical_scenarios`, this needs no pre-migration git
/// snapshot, so completeness holds uniformly for a behaviour's very first
/// `.feature` file too (B25: a brand-new file has no historical comment
/// to diff against, but its scenarios still need traceability records).
fn current_scenarios() -> Vec<(String, ScenarioBlock)> {
    let mut out = Vec::new();
    for path in migrated_feature_files() {
        let behaviour = behaviour_id_from_path(&path);
        let content = std::fs::read_to_string(&path).unwrap();
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
    parse_record(&content)
}

/// [`read_record`]'s parsing, split out so a record's content can also be
/// read from git history (a specific historical revision), not only from
/// the working tree -- used to check a post-migration record's evidence
/// against its own first-committed version (warden security review, High:
/// see `content_baseline_rev_and_path`'s doc comment for the equivalent
/// gap this closes for `.feature` file structure).
fn parse_record(content: &str) -> Record {
    let feature_file = content
        .lines()
        .find_map(|l| l.strip_prefix("**Feature file:** `"))
        .and_then(|s| s.strip_suffix('`'))
        .expect("record should have a Feature file reference")
        .to_string();
    // warden security review: `repo_root().join(&record.feature_file)`
    // would follow an absolute path outright (`PathBuf::join` replaces the
    // base on an absolute component), and `..` components could escape
    // `features/`. Requires repo write access to exploit -- the same
    // trust level as every other finding in this file -- but cheap to
    // fail loudly on rather than silently permit.
    assert!(
        !Path::new(&feature_file).is_absolute() && !feature_file.contains(".."),
        "record's Feature file reference must be a plain relative path under features/, got {feature_file:?}"
    );
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

/// The pure decision behind E1's completeness check: which (behaviour,
/// expectation) scenarios have no corresponding record, given a predicate
/// for "does a record exist". Hermetically testable against literal
/// in-memory fixtures, no filesystem needed (qa test-design review: the
/// switch from historical- to current-scenario-based completeness had no
/// committed coverage of its own decision logic).
fn missing_records(
    scenarios: &[(String, ScenarioBlock)],
    record_exists: impl Fn(&str, &str) -> bool,
) -> Vec<String> {
    scenarios
        .iter()
        .filter(|(behaviour, block)| !record_exists(behaviour, &block.expectation))
        .map(|(behaviour, block)| format!("{behaviour} {}: {}", block.expectation, block.title))
        .collect()
}

/// E1's check: every CURRENT (behaviour, expectation) has a corresponding
/// traceability record -- checked against what exists today, not only
/// what existed at the migration commit, so a brand-new behaviour's
/// first-ever `.feature` file is covered too (B25).
fn check_completeness() -> Vec<String> {
    missing_records(&current_scenarios(), |behaviour, expectation| {
        traceability_dir()
            .join(behaviour)
            .join(format!("{expectation}.md"))
            .is_file()
    })
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

/// The pure per-record fidelity decision (qa test-design review: neither
/// branch had committed coverage of its own logic, since every record in
/// this repository today happens to be migrated, so the untested branch
/// was permanently unreachable by anything in the real suite).
/// `historical_evidence` is `Some(extracted comment text)` when the
/// record's expectation was found in its migrated file's historical
/// snapshot, `None` when that specific expectation is missing from an
/// otherwise-migrated file (a real problem); `own_first_commit_evidence`
/// is the fallback baseline for a file that postdates both fixed
/// migration commits (warden security review, High).
fn fidelity_verdict(
    record_evidence: &str,
    file_existed_at_migration: bool,
    historical_evidence: Option<&str>,
    own_first_commit_evidence: &str,
) -> Option<&'static str> {
    if file_existed_at_migration {
        match historical_evidence {
            None => Some("no historical scenario found"),
            Some(expected) if evidence_preserves_original(record_evidence, expected) => None,
            Some(_) => Some("evidence text differs"),
        }
    } else if evidence_preserves_original(record_evidence, own_first_commit_evidence) {
        None
    } else {
        Some("evidence text differs from its own first-committed version")
    }
}

/// E2's check: every given record's evidence matches its pre-migration
/// comment byte-for-byte, or legitimately extends it, via
/// [`fidelity_verdict`]'s decision. `existed_at` is resolved once per
/// FEATURE FILE and reused across that file's records (qa test-design
/// review: the answer never varies within one file, so re-deriving it per
/// record spawned ~7x more `git` subprocesses than necessary).
fn check_fidelity(record_paths: &[PathBuf]) -> Vec<String> {
    let historical = historical_scenarios();
    let mut existed_cache: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
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
        let commit = migration_commit_for(Path::new(&record.feature_file));
        let feature_file = record.feature_file.clone();
        let file_existed = *existed_cache
            .entry(feature_file.clone())
            .or_insert_with(|| existed_at(&format!("{commit}^:{feature_file}")));

        let historical_evidence = historical
            .iter()
            .find(|(b, blk)| b == behaviour && blk.expectation == expectation)
            .map(|(_, blk)| extract_evidence(blk).unwrap_or_default());

        let own_first_commit_evidence = if file_existed {
            String::new() // unused by fidelity_verdict in this branch
        } else {
            let record_rel = record_path
                .strip_prefix(repo_root())
                .expect("record path should be under the repo root")
                .to_str()
                .unwrap();
            let first_commit = first_commit_introducing(record_rel);
            parse_record(&git_show(&format!("{first_commit}:{record_rel}"))).evidence
        };

        if let Some(reason) = fidelity_verdict(
            &record.evidence,
            file_existed,
            historical_evidence.as_deref(),
            &own_first_commit_evidence,
        ) {
            mismatches.push(format!("{behaviour} {expectation}: {reason}"));
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
    if expected.name != actual.name {
        // warden security review, Medium: `Feature.name` was never
        // compared, even though `Feature` derives `PartialEq` over both
        // `name` and `scenarios` -- a renamed `Feature:` line passed with
        // zero complaint.
        return Some(format!(
            "Feature name changed ({:?} -> {:?})",
            expected.name, actual.name
        ));
    }
    if expected.scenarios != actual.scenarios {
        // qa test-design review: name which scenario/step differs, not
        // just that something did, so a real failure doesn't need manual
        // diffing to localize.
        let first_diff = expected
            .scenarios
            .iter()
            .zip(actual.scenarios.iter())
            .find(|(e, a)| e != a)
            .map(|(e, _)| e.name.clone())
            .unwrap_or_else(|| "<unknown -- lengths matched but zip found nothing>".to_string());
        return Some(format!(
            "a scenario's name or step content changed (first differing scenario: {first_diff:?})"
        ));
    }
    None
}

/// The `<rev>:<path>` to diff `path`'s content against for E4's
/// content-unchanged check: the fixed migration commit if `path` existed
/// there, otherwise `path`'s own first-committed version. Every file gets
/// a REAL baseline to compare against; none are ever simply exempted
/// (warden security review, High: the previous fix skipped this check
/// entirely for any file postdating the fixed migration commits --
/// permanently, since nothing re-derives a per-file baseline -- which
/// silently exempted every future `.feature` file's Given/When/Then
/// structure from ever being checked against anything, once its evidence
/// was accepted).
fn content_baseline_rev_and_path(path: &Path) -> String {
    let rel = format!("features/{}", path.file_name().unwrap().to_str().unwrap());
    let pinned = format!("{}^:{rel}", migration_commit_for(path));
    if existed_at(&pinned) {
        return pinned;
    }
    format!("{}:{rel}", first_commit_introducing(&rel))
}

fn check_content_unchanged() -> Vec<String> {
    let mut problems = Vec::new();
    for path in migrated_feature_files() {
        let rel = format!("features/{}", path.file_name().unwrap().to_str().unwrap());
        let historical = git_show(&content_baseline_rev_and_path(&path));
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
    use std::path::Path;
    use std::process::Command;

    use super::{
        ScenarioBlock, content_baseline_rev_and_path, evidence_preserves_original, existed_at,
        existed_at_in, feature_structure_mismatch, fidelity_verdict, first_commit_introducing_in,
        missing_records,
    };

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

    #[test]
    fn existed_at_is_true_for_a_real_pre_migration_file() {
        assert!(existed_at(&format!(
            "{}^:features/B1-walking-skeleton.feature",
            super::MIGRATION_COMMIT
        )));
    }

    #[test]
    fn existed_at_is_false_for_a_real_file_at_the_commit_right_before_its_own_first_one() {
        // qa test-design review: a path that never existed at all doesn't
        // test the actual hazard (a file that exists on disk NOW but
        // didn't exist at some earlier real commit). Uses a real tracked
        // file's own real history instead of a stand-in path.
        let rel = "features/B23-dotted-list-round-trip.feature";
        let first_commit = super::first_commit_introducing(rel);
        assert!(!existed_at(&format!("{first_commit}^:{rel}")));
    }

    #[test]
    fn existed_at_is_false_for_a_file_that_never_existed_at_all() {
        assert!(!existed_at(&format!(
            "{}^:features/this-file-does-not-exist.feature",
            super::MIGRATION_COMMIT
        )));
    }

    #[test]
    #[should_panic(expected = "plain relative path")]
    fn parse_record_rejects_an_absolute_feature_file_reference() {
        super::parse_record(
            "**Scenario:** E1 — x\n**Feature file:** `/etc/passwd`\n\n## Evidence\n\n```\ntext\n```\n",
        );
    }

    #[test]
    #[should_panic(expected = "plain relative path")]
    fn parse_record_rejects_a_feature_file_reference_containing_dot_dot() {
        super::parse_record(
            "**Scenario:** E1 — x\n**Feature file:** `features/../../../etc/passwd`\n\n## Evidence\n\n```\ntext\n```\n",
        );
    }

    #[test]
    fn content_baseline_uses_the_pinned_migration_commit_for_an_already_migrated_file() {
        let path = Path::new("features/B1-walking-skeleton.feature");
        assert_eq!(
            content_baseline_rev_and_path(path),
            format!(
                "{}^:features/B1-walking-skeleton.feature",
                super::MIGRATION_COMMIT
            )
        );
    }

    #[test]
    fn a_files_own_first_commit_is_correctly_identified_in_a_hermetic_repo() {
        // A small, throwaway git repo -- NOT this project's own history --
        // with two commits: the first has no feature file at all, the
        // second adds one. Proves existed_at_in/first_commit_introducing_in
        // genuinely distinguish "exists now" from "existed at some earlier
        // commit" end to end, without waiting for (or polluting) this
        // project's real history with an actual post-migration file (qa
        // test-design review: the only committed way to exercise the
        // fallback logic, since every file in THIS repo today happens to
        // predate the fixed migration commits).
        let dir = std::env::temp_dir().join(format!(
            "magiclisp-trace1-hermetic-{}-{}",
            std::process::id(),
            "a_files_own_first_commit_is_correctly_identified_in_a_hermetic_repo"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .expect("git should run");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "trace1-hermetic-test@example.com"]);
        git(&["config", "user.name", "TRACE1 hermetic test"]);
        std::fs::write(
            dir.join("unrelated.txt"),
            "commit 1 -- no feature file yet\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "commit 1"]);

        std::fs::create_dir_all(dir.join("features")).unwrap();
        std::fs::write(
            dir.join("features/NEW-behaviour.feature"),
            "Feature: a behaviour born in commit 2\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "commit 2 -- adds the feature file"]);

        let rel = "features/NEW-behaviour.feature";
        let introducing = first_commit_introducing_in(&dir, rel);
        assert!(
            existed_at_in(&dir, &format!("{introducing}:{rel}")),
            "the file should exist at its own introducing commit"
        );
        assert!(
            !existed_at_in(&dir, &format!("{introducing}^:{rel}")),
            "the file should NOT exist at the commit right before its own introduction"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn scenario(behaviour: &str, expectation: &str, title: &str) -> (String, ScenarioBlock) {
        (
            behaviour.to_string(),
            ScenarioBlock {
                expectation: expectation.to_string(),
                title: title.to_string(),
                abs_start: 0,
                lines: Vec::new(),
            },
        )
    }

    #[test]
    fn missing_records_reports_only_the_scenario_the_predicate_says_has_no_record() {
        // qa test-design review: check_completeness's switch to
        // current-scenario-based lookup had no committed coverage of its
        // own decision logic (every real file today already has every
        // record, so the "missing" branch was unreachable by anything in
        // the real suite).
        let scenarios = vec![
            scenario("B25", "E1", "first"),
            scenario("B25", "E2", "second"),
        ];
        let missing = missing_records(&scenarios, |b, e| b == "B25" && e == "E1");
        assert_eq!(missing, vec!["B25 E2: second".to_string()]);
    }

    #[test]
    fn missing_records_reports_nothing_when_every_scenario_has_a_record() {
        let scenarios = vec![scenario("B25", "E1", "first")];
        assert!(missing_records(&scenarios, |_, _| true).is_empty());
    }

    #[test]
    fn fidelity_verdict_flags_a_migrated_files_missing_historical_scenario() {
        assert_eq!(
            fidelity_verdict("some evidence", true, None, ""),
            Some("no historical scenario found")
        );
    }

    #[test]
    fn fidelity_verdict_flags_a_migrated_files_altered_evidence() {
        assert_eq!(
            fidelity_verdict("altered", true, Some("original"), ""),
            Some("evidence text differs")
        );
    }

    #[test]
    fn fidelity_verdict_accepts_a_migrated_files_unaltered_evidence() {
        assert_eq!(
            fidelity_verdict("original", true, Some("original"), ""),
            None
        );
    }

    #[test]
    fn fidelity_verdict_accepts_a_post_migration_records_unaltered_evidence() {
        // warden security review, High: this is the exact branch that was
        // previously either a hard skip or an always-false-positive --
        // now a real, hermetically-tested comparison.
        assert_eq!(
            fidelity_verdict("first-commit text", false, None, "first-commit text"),
            None
        );
    }

    #[test]
    fn fidelity_verdict_flags_a_post_migration_records_altered_evidence() {
        assert_eq!(
            fidelity_verdict("tampered", false, None, "first-commit text"),
            Some("evidence text differs from its own first-committed version")
        );
    }
}
