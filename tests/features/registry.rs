//! Matches parsed Gherkin steps to registered Rust step definitions and
//! runs a whole `.feature` file's scenarios, each against a fresh [`World`].
//! Any step with no matching definition, or any definition that fails its
//! assertions, fails the run loudly and by name — a `.feature` scenario
//! can't silently pass without every one of its steps actually executing.

use super::gherkin::{Scenario, parse_feature};
use super::world::World;

/// `(world, matched step text, docstring)` — the matched text is passed
/// through so a handler registered under several different exact wordings
/// (e.g. "the same generic action, applied to whatever the Given queued
/// up") can, when needed, self-extract a literal (like a quoted code
/// snippet) straight out of its own key rather than needing one dedicated
/// closure per wording.
pub(crate) type StepFn = fn(&mut World, &str, Option<&str>);

#[derive(Default)]
pub(crate) struct Registry {
    steps: Vec<(&'static str, StepFn)>,
}

impl Registry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Registers a step definition under its exact expected wording (after
    /// the leading Given/When/Then/And keyword is stripped and any wrapped
    /// continuation lines are joined with single spaces — see
    /// `gherkin::parse_feature`). Deliberately exact-match, not a
    /// templating engine: most of this project's scenarios are
    /// bespoke prose rather than a small set of reusable parameterized
    /// phrasings, so little would be gained by a generic pattern language,
    /// and exact matching means a wording drift in the `.feature` file is
    /// caught immediately as an unbound step instead of silently
    /// matching something unintended.
    pub(crate) fn step(mut self, text: &'static str, f: StepFn) -> Self {
        self.steps.push((text, f));
        self
    }

    fn find(&self, text: &str) -> Option<StepFn> {
        self.steps.iter().find(|(t, _)| *t == text).map(|(_, f)| *f)
    }
}

/// Parses and runs every scenario in `src` (a `.feature` file's contents)
/// against `registry`, panicking with every scenario's failure (unbound
/// step or failed assertion) collected together, not just the first.
pub(crate) fn run_feature(feature_label: &str, src: &str, registry: &Registry) {
    let feature = parse_feature(src);
    assert!(
        !feature.scenarios.is_empty(),
        "{feature_label}: parsed zero scenarios -- the Gherkin parser likely doesn't \
         understand this file's layout, not that the file has no scenarios"
    );
    run_scenarios(feature_label, &feature.scenarios, registry);
}

/// Like [`run_feature`], but only runs the scenarios whose name starts with
/// one of `scenario_prefixes` (e.g. `"E3 "`, matching the `.feature` file's
/// own `"E3 — ..."` naming, trailing space included so `"E1 "` can't also
/// match `"E10 ..."`). Lets one `.feature` file's fast, always-on checks
/// and its slow, sustained-soak checks live as two separate `#[test]`
/// functions -- one plain, one `#[ignore]`d -- instead of forcing every
/// scenario in the file under a single un-ignorable test (qa test-design
/// review: an unconditional ~60s+ soak running on every default `cargo
/// test` invocation, several times over across the whole suite).
pub(crate) fn run_feature_subset(
    feature_label: &str,
    src: &str,
    registry: &Registry,
    scenario_prefixes: &[&str],
) {
    let feature = parse_feature(src);
    let scenarios: Vec<Scenario> = feature
        .scenarios
        .into_iter()
        .filter(|s| scenario_prefixes.iter().any(|p| s.name.starts_with(p)))
        .collect();
    assert!(
        !scenarios.is_empty(),
        "{feature_label}: none of {scenario_prefixes:?} matched a scenario name -- likely a \
         prefix or wording drift against the .feature file"
    );
    run_scenarios(feature_label, &scenarios, registry);
}

/// Asserts every scenario in `src` is covered by at least one of
/// `all_prefixes` -- meant to be called with the UNION of every split
/// (fast + soak, or however many) [`run_feature_subset`] prefix lists for
/// one `.feature` file. `run_feature_subset` alone only guards against a
/// prefix matching *nothing*; it has no way to know about a sibling
/// split's prefixes, so a future scenario left out of every list would
/// silently never run under any invocation, default or `--ignored` (warden
/// security review) -- this closes that gap with a loud, always-on check.
pub(crate) fn assert_full_scenario_coverage(feature_label: &str, src: &str, all_prefixes: &[&str]) {
    let feature = parse_feature(src);
    let uncovered: Vec<&str> = feature
        .scenarios
        .iter()
        .filter(|s| !all_prefixes.iter().any(|p| s.name.starts_with(p)))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        uncovered.is_empty(),
        "{feature_label}: scenario(s) not covered by any prefix list, so they'd never run \
         under any split test: {uncovered:?}"
    );
}

fn run_scenarios(feature_label: &str, scenarios: &[Scenario], registry: &Registry) {
    let mut failures = Vec::new();
    for scenario in scenarios {
        let mut world = World::default();
        let mut unbound = Vec::new();
        for step in &scenario.steps {
            match registry.find(&step.text) {
                Some(f) => {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        f(&mut world, &step.text, step.docstring.as_deref())
                    }));
                    if let Err(payload) = outcome {
                        let message = payload
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| payload.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "step panicked with a non-string payload".into());
                        failures.push(format!(
                            "{feature_label} :: {} :: step {:?} FAILED: {message}",
                            scenario.name, step.text
                        ));
                        break;
                    }
                }
                None => unbound.push(step.text.clone()),
            }
        }
        if !unbound.is_empty() {
            failures.push(format!(
                "{feature_label} :: {}: no step definition bound for: {unbound:?}",
                scenario.name
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "\n\n{} scenario failure(s) in {feature_label}:\n{}\n",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "Feature: fixture\n\n  \
        Scenario: E1 — first\n    Given nothing\n\n  \
        Scenario: E4 — fourth\n    Given nothing\n\n  \
        Scenario: E10 — tenth\n    Given nothing\n";

    fn no_op_registry() -> Registry {
        Registry::new().step("nothing", |_w, _t, _d| {})
    }

    #[test]
    fn run_feature_subset_runs_only_the_matching_prefixes() {
        // A run that only matches E1 and E4 must not also pull in E10 --
        // if it did, this run would still pass today (its step is a no-op),
        // so the real assertion is scenario *count*.
        let feature = parse_feature(SRC);
        let matched: Vec<_> = feature
            .scenarios
            .iter()
            .filter(|s| ["E1 ", "E4 "].iter().any(|p| s.name.starts_with(p)))
            .collect();
        assert_eq!(
            matched.len(),
            2,
            "expected exactly E1 and E4, got {matched:?}"
        );
    }

    #[test]
    fn an_e1_prefix_does_not_also_match_e10() {
        assert!(!"E10 — tenth".starts_with("E1 "));
        assert!("E1 — first".starts_with("E1 "));
    }

    #[test]
    #[should_panic(expected = "none of")]
    fn run_feature_subset_panics_when_no_scenario_matches_the_given_prefixes() {
        run_feature_subset("fixture", SRC, &no_op_registry(), &["E99 "]);
    }

    #[test]
    fn run_feature_subset_actually_executes_the_matched_scenarios() {
        // Runs clean against the real registry -- proves the whole path
        // (parse -> filter -> dispatch) works end to end, not just the
        // filter predicate in isolation.
        run_feature_subset("fixture", SRC, &no_op_registry(), &["E1 ", "E4 "]);
    }

    #[test]
    fn full_scenario_coverage_passes_when_the_union_of_prefixes_covers_every_scenario() {
        assert_full_scenario_coverage("fixture", SRC, &["E1 ", "E4 ", "E10 "]);
    }

    #[test]
    #[should_panic(expected = "E10")]
    fn full_scenario_coverage_panics_when_a_scenario_is_left_out_of_every_prefix_list() {
        // The exact drift warden's review warned about: a scenario (E10
        // here) not named by any split's prefix list would otherwise
        // silently never run under any invocation.
        assert_full_scenario_coverage("fixture", SRC, &["E1 ", "E4 "]);
    }
}
