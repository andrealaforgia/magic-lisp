//! Reads MagicLisp source text into a tree of [`Sexpr`] values.

#[derive(Debug, Clone, PartialEq)]
pub enum Sexpr {
    Int(i64),
    Bool(bool),
    Str(String),
    Symbol(String),
    List(Vec<Sexpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadError {
    pub message: String,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "read error: {}", self.message)
    }
}

fn err(message: impl Into<String>) -> ReadError {
    ReadError {
        message: message.into(),
    }
}

/// Caps list-nesting depth so pathological source (e.g. thousands of unmatched
/// '(') fails cleanly instead of risking a native stack overflow.
const MAX_NESTING_DEPTH: usize = 512;

struct Scanner<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    depth: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Scanner {
            chars: src.chars().peekable(),
            depth: 0,
        }
    }

    fn skip_atmosphere(&mut self) {
        loop {
            match self.chars.peek() {
                Some(c) if c.is_whitespace() => {
                    self.chars.next();
                }
                Some(';') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn is_delimiter(c: char) -> bool {
        c.is_whitespace() || c == '(' || c == ')' || c == '"' || c == ';'
    }

    fn read_form(&mut self) -> Result<Option<Sexpr>, ReadError> {
        self.skip_atmosphere();
        match self.chars.peek() {
            None => Ok(None),
            Some('(') => {
                self.chars.next();
                self.read_list().map(Some)
            }
            Some(')') => Err(err("unexpected ')' with no matching '('")),
            Some('"') => self.read_string().map(Some),
            Some(_) => self.read_atom().map(Some),
        }
    }

    fn read_list(&mut self) -> Result<Sexpr, ReadError> {
        self.depth += 1;
        let result = if self.depth > MAX_NESTING_DEPTH {
            Err(err(format!(
                "list nesting exceeds the maximum supported depth ({MAX_NESTING_DEPTH})"
            )))
        } else {
            self.read_list_body()
        };
        self.depth -= 1;
        result
    }

    fn read_list_body(&mut self) -> Result<Sexpr, ReadError> {
        let mut items = Vec::new();
        loop {
            self.skip_atmosphere();
            if let Some(')') = self.chars.peek() {
                self.chars.next();
                return Ok(Sexpr::List(items));
            }
            let form = self
                .read_form()?
                .ok_or_else(|| err("unterminated list: missing ')'"))?;
            items.push(form);
        }
    }

    fn read_string(&mut self) -> Result<Sexpr, ReadError> {
        self.chars.next(); // consume opening quote
        let mut s = String::new();
        loop {
            match self.chars.next() {
                None => return Err(err("unterminated string literal: missing closing '\"'")),
                Some('"') => return Ok(Sexpr::Str(s)),
                Some('\n') => {
                    return Err(err(
                        "unescaped newline inside string literal before closing '\"'",
                    ));
                }
                Some('\\') => match self.chars.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some(other) => {
                        return Err(err(format!("unsupported escape sequence '\\{other}'")));
                    }
                    None => return Err(err("unterminated escape sequence at end of input")),
                },
                Some(c) => s.push(c),
            }
        }
    }

    fn read_atom(&mut self) -> Result<Sexpr, ReadError> {
        let mut text = String::new();
        while let Some(&c) = self.chars.peek() {
            if Self::is_delimiter(c) {
                break;
            }
            text.push(c);
            self.chars.next();
        }
        if text.is_empty() {
            return Err(err("unexpected character while reading a token"));
        }
        Ok(atom_from_text(&text))
    }
}

fn atom_from_text(text: &str) -> Sexpr {
    match text {
        "true" => Sexpr::Bool(true),
        "false" => Sexpr::Bool(false),
        _ => match text.parse::<i64>() {
            Ok(n) => Sexpr::Int(n),
            Err(_) => Sexpr::Symbol(text.to_string()),
        },
    }
}

pub fn read_program(src: &str) -> Result<Vec<Sexpr>, ReadError> {
    let mut scanner = Scanner::new(src);
    let mut forms = Vec::new();
    while let Some(form) = scanner.read_form()? {
        forms.push(form);
    }
    Ok(forms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_whole_number() {
        assert_eq!(read_program("42").unwrap(), vec![Sexpr::Int(42)]);
    }

    #[test]
    fn reads_a_negative_number() {
        assert_eq!(read_program("-7").unwrap(), vec![Sexpr::Int(-7)]);
    }

    #[test]
    fn reads_a_symbol() {
        assert_eq!(
            read_program("display").unwrap(),
            vec![Sexpr::Symbol("display".to_string())]
        );
    }

    #[test]
    fn reads_booleans() {
        assert_eq!(
            read_program("true false").unwrap(),
            vec![Sexpr::Bool(true), Sexpr::Bool(false)]
        );
    }

    #[test]
    fn reads_a_string_with_escapes() {
        let src = r#""a\nb\tc\r\"d\\e""#;
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::Str("a\nb\tc\r\"d\\e".to_string())]
        );
    }

    #[test]
    fn reads_a_well_formed_nested_list() {
        assert_eq!(
            read_program("(+ 1 (+ 2 3))").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("+".to_string()),
                Sexpr::Int(1),
                Sexpr::List(vec![
                    Sexpr::Symbol("+".to_string()),
                    Sexpr::Int(2),
                    Sexpr::Int(3),
                ]),
            ])]
        );
    }

    #[test]
    fn skips_line_comments() {
        let src = "; a leading comment\n(display 1) ; trailing comment\n";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("display".to_string()),
                Sexpr::Int(1),
            ])]
        );
    }

    #[test]
    fn treats_arbitrary_whitespace_as_a_separator() {
        let src = "  (\tdisplay\n  1  )  \r\n";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("display".to_string()),
                Sexpr::Int(1),
            ])]
        );
    }

    #[test]
    fn reads_multiple_top_level_forms() {
        assert_eq!(
            read_program("(display 1) (newline)").unwrap(),
            vec![
                Sexpr::List(vec![Sexpr::Symbol("display".to_string()), Sexpr::Int(1)]),
                Sexpr::List(vec![Sexpr::Symbol("newline".to_string())]),
            ]
        );
    }

    #[test]
    fn reads_a_source_file_exercising_every_supported_construct_together() {
        let src = r#"
            ; comment before a call
            (display "a\nb\tc\r\"d\\e") (newline)
            (display (+ 42 (+ 1 2))) (newline)
            (display true) (newline)
            (display false) (newline)
        "#;
        let forms = read_program(src).unwrap();
        assert_eq!(forms.len(), 8);
    }

    #[test]
    fn rejects_a_raw_unescaped_newline_inside_a_string_literal() {
        let src = "\"broken\nstring\"";
        let err = read_program(src).unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn unterminated_string_is_a_read_error() {
        let src = "\"never closed";
        assert!(read_program(src).is_err());
    }

    #[test]
    fn unbalanced_close_paren_is_a_read_error() {
        assert!(read_program(")").is_err());
    }

    #[test]
    fn unbalanced_open_paren_is_a_read_error() {
        assert!(read_program("(display 1").is_err());
    }

    #[test]
    fn read_error_display_includes_the_underlying_message() {
        let e = ReadError {
            message: "something specific went wrong".to_string(),
        };
        assert_eq!(e.to_string(), "read error: something specific went wrong");
    }

    #[test]
    fn each_delimiter_character_ends_a_preceding_atom_without_whitespace() {
        assert_eq!(
            read_program("a(b)").unwrap(),
            vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::List(vec![Sexpr::Symbol("b".to_string())]),
            ]
        );
        assert_eq!(
            read_program("(a)b").unwrap(),
            vec![
                Sexpr::List(vec![Sexpr::Symbol("a".to_string())]),
                Sexpr::Symbol("b".to_string()),
            ]
        );
        assert_eq!(
            read_program("a;comment\nb").unwrap(),
            vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::Symbol("b".to_string())
            ]
        );
        assert_eq!(
            read_program("a\"str\"").unwrap(),
            vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::Str("str".to_string())
            ]
        );
    }

    #[test]
    fn a_comment_with_no_trailing_newline_before_eof_is_skipped_cleanly() {
        assert_eq!(read_program("; just a comment").unwrap(), vec![]);
    }

    #[test]
    fn accepts_nesting_comfortably_under_the_configured_maximum() {
        let src = format!("{}1{}", "(".repeat(100), ")".repeat(100));
        assert!(read_program(&src).is_ok());
    }

    #[test]
    fn accepts_nesting_of_exactly_the_configured_maximum_depth() {
        let src = format!(
            "{}1{}",
            "(".repeat(MAX_NESTING_DEPTH),
            ")".repeat(MAX_NESTING_DEPTH)
        );
        assert!(read_program(&src).is_ok());
    }

    #[test]
    fn rejects_nesting_of_one_more_than_the_configured_maximum_depth_with_a_clear_message() {
        // Well-formed but pathologically deep: without an explicit depth
        // limit this would recurse once per '(' and risk a stack overflow
        // on attacker-supplied source (a security-review finding on B1).
        let src = format!(
            "{}1{}",
            "(".repeat(MAX_NESTING_DEPTH + 1),
            ")".repeat(MAX_NESTING_DEPTH + 1)
        );
        let err = read_program(&src).unwrap_err();
        assert!(
            err.message.contains("nesting") && err.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            err.message
        );
    }

    #[test]
    fn depth_is_restored_after_a_list_closes_so_sibling_lists_get_a_fresh_budget() {
        // Each sibling is well under the limit on its own; if depth weren't
        // correctly decremented on exit from the first list, it would carry
        // over and push the second list over budget.
        let one_sibling = format!("{}1{}", "(".repeat(300), ")".repeat(300));
        let src = format!("{one_sibling} {one_sibling}");
        assert!(read_program(&src).is_ok());
    }
}
