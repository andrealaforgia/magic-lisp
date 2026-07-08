//! Matches parsed Gherkin steps to registered Rust step definitions and
//! runs a whole `.feature` file's scenarios, each against a fresh [`World`].
//! Any step with no matching definition, or any definition that fails its
//! assertions, fails the run loudly and by name — a `.feature` scenario
//! can't silently pass without every one of its steps actually executing.

use super::gherkin::parse_feature;
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
    let mut failures = Vec::new();
    for scenario in &feature.scenarios {
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
