//! A minimal Gherkin parser: just enough of the `.feature` file syntax
//! actually used by this project's feature files (Feature/Scenario headers,
//! Given/When/Then/And step lines that may wrap across multiple physical
//! lines for readability, and triple-quoted docstrings attached to a step)
//! to drive real step definitions from the files themselves — no external
//! dependency needed for a subset this small and stable.

#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// The step's text with its leading keyword (Given/When/Then/And/But)
    /// stripped and any wrapped continuation lines joined with a single
    /// space, so a step's wording in the .feature file can span several
    /// physical lines without affecting how it's matched.
    pub(crate) text: String,
    /// The content of an attached `"""..."""` docstring, if this step ends
    /// with one, dedented relative to the opening `"""`'s own indentation
    /// and otherwise preserved verbatim (including embedded quotes,
    /// backslashes, and tabs — these are literal expected bytes, not
    /// escape sequences to interpret).
    pub(crate) docstring: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Scenario {
    pub(crate) name: String,
    pub(crate) steps: Vec<Step>,
}

#[derive(Debug, Clone)]
pub(crate) struct Feature {
    pub(crate) name: String,
    pub(crate) scenarios: Vec<Scenario>,
}

const STEP_KEYWORDS: [&str; 5] = ["Given ", "When ", "Then ", "And ", "But "];

pub(crate) fn parse_feature(src: &str) -> Feature {
    let mut lines = src.lines().peekable();
    let mut feature_name = String::new();
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut current: Option<Scenario> = None;

    while let Some(raw_line) = lines.next() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Feature:") {
            feature_name = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Scenario:") {
            if let Some(s) = current.take() {
                scenarios.push(s);
            }
            current = Some(Scenario {
                name: rest.trim().to_string(),
                steps: Vec::new(),
            });
            continue;
        }

        let keyword_match = STEP_KEYWORDS.iter().find_map(|kw| trimmed.strip_prefix(kw));

        if let Some(rest) = keyword_match {
            let Some(scenario) = current.as_mut() else {
                continue; // narrative text before the first Scenario (As a.../I want...)
            };
            let mut text = rest.trim().to_string();
            let docstring = if text.ends_with(':') {
                text.pop(); // drop the trailing ':' that introduces the docstring
                Some(read_docstring(&mut lines))
            } else {
                None
            };
            scenario.steps.push(Step { text, docstring });
        } else if let Some(scenario) = current.as_mut() {
            // A continuation line: part of the previous step's wording,
            // wrapped only for readability in the source file.
            if let Some(last) = scenario.steps.last_mut() {
                last.text.push(' ');
                last.text.push_str(trimmed);
            }
        }
        // Text before any Scenario (the Feature's "As a.../I want.../So
        // that..." narrative) is intentionally ignored — it documents
        // intent, not executable behaviour.
    }
    if let Some(s) = current.take() {
        scenarios.push(s);
    }
    Feature {
        name: feature_name,
        scenarios,
    }
}

/// Reads a `"""`-delimited docstring, dedenting each content line by the
/// opening `"""` marker's own indentation (if that much whitespace is
/// present) and preserving everything else verbatim.
fn read_docstring<'a>(lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>) -> String {
    let opener = lines.next().unwrap_or("");
    let indent = opener.len() - opener.trim_start().len();
    let mut content = Vec::new();
    for line in lines.by_ref() {
        if line.trim() == "\"\"\"" {
            break;
        }
        let dedented =
            if line.len() >= indent && line.get(..indent).is_some_and(|p| p.trim().is_empty()) {
                &line[indent..]
            } else {
                line
            };
        content.push(dedented.to_string());
    }
    content.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_feature_name_and_scenario_names() {
        let src = "Feature: My feature\n  As a user\n\n  Scenario: First\n    Given a thing\n    When it happens\n    Then it works\n";
        let feature = parse_feature(src);
        assert_eq!(feature.name, "My feature");
        assert_eq!(feature.scenarios.len(), 1);
        assert_eq!(feature.scenarios[0].name, "First");
        assert_eq!(feature.scenarios[0].steps.len(), 3);
        assert_eq!(feature.scenarios[0].steps[0].text, "a thing");
        assert_eq!(feature.scenarios[0].steps[1].text, "it happens");
        assert_eq!(feature.scenarios[0].steps[2].text, "it works");
    }

    #[test]
    fn joins_a_step_wrapped_across_continuation_lines() {
        let src = "Feature: F\n  Scenario: S\n    Then this is a long step\n      that wraps onto\n      a second line\n";
        let feature = parse_feature(src);
        assert_eq!(
            feature.scenarios[0].steps[0].text,
            "this is a long step that wraps onto a second line"
        );
    }

    #[test]
    fn attaches_a_dedented_docstring_to_its_step() {
        let src = "Feature: F\n  Scenario: S\n    Given a file containing:\n      \"\"\"\n      line one\n      line two\n      \"\"\"\n";
        let feature = parse_feature(src);
        let step = &feature.scenarios[0].steps[0];
        assert_eq!(step.text, "a file containing");
        assert_eq!(step.docstring.as_deref(), Some("line one\nline two"));
    }

    #[test]
    fn ignores_comment_and_blank_lines() {
        let src = "Feature: F\n  # a comment\n\n  Scenario: S\n    # another comment\n    Given a thing\n";
        let feature = parse_feature(src);
        assert_eq!(feature.scenarios[0].steps.len(), 1);
    }

    #[test]
    fn parses_multiple_scenarios() {
        let src = "Feature: F\n  Scenario: One\n    Given a\n  Scenario: Two\n    Given b\n";
        let feature = parse_feature(src);
        assert_eq!(feature.scenarios.len(), 2);
        assert_eq!(feature.scenarios[0].name, "One");
        assert_eq!(feature.scenarios[1].name, "Two");
    }
}
