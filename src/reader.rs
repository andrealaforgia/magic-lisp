//! Reads MagicLisp source text into a tree of [`Sexpr`] values.

#[derive(Debug, Clone, PartialEq)]
pub enum Sexpr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Symbol(String),
    List(Vec<Sexpr>),
    /// `(a b . rest)` — an improper list with a fixed head and a non-list
    /// tail. Used exclusively for parameter-list syntax in this language.
    DottedList(Vec<Sexpr>, Box<Sexpr>),
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
        c.is_whitespace() || c == '(' || c == ')' || c == '"' || c == ';' || c == '\''
    }

    fn peek_is_lone_dot(&self) -> bool {
        let mut lookahead = self.chars.clone();
        if lookahead.next() != Some('.') {
            return false;
        }
        match lookahead.peek() {
            None => true,
            Some(&c) => Self::is_delimiter(c),
        }
    }

    /// Reads one datum, if any remain. Every recursive descent into this
    /// function increments `self.depth` for the duration of the call (and
    /// restores it on return), so the depth limit applies uniformly to
    /// *any* recursive construct — list nesting, quote shorthand, and
    /// anything added later — by construction, rather than requiring each
    /// new caller to separately opt in (a prior version only checked depth
    /// inside list-reading, so quote-shorthand nesting bypassed it entirely
    /// — a security-review finding on B2).
    fn read_form(&mut self) -> Result<Option<Sexpr>, ReadError> {
        self.skip_atmosphere();
        if self.chars.peek().is_none() {
            return Ok(None);
        }
        self.depth += 1;
        let result = if self.depth > MAX_NESTING_DEPTH {
            Err(err(format!(
                "list nesting exceeds the maximum supported depth ({MAX_NESTING_DEPTH})"
            )))
        } else {
            self.read_form_body().map(Some)
        };
        self.depth -= 1;
        result
    }

    fn read_form_body(&mut self) -> Result<Sexpr, ReadError> {
        match self.chars.peek() {
            None => unreachable!("read_form already checked for end of input"),
            Some('(') => {
                self.chars.next();
                self.read_list_body()
            }
            Some(')') => Err(err("unexpected ')' with no matching '('")),
            Some('"') => self.read_string(),
            Some('\'') => {
                self.chars.next();
                let datum = self
                    .read_form()?
                    .ok_or_else(|| err("expected a datum after '\''"))?;
                Ok(Sexpr::List(vec![Sexpr::Symbol("quote".to_string()), datum]))
            }
            Some(_) => self.read_atom(),
        }
    }

    fn read_list_body(&mut self) -> Result<Sexpr, ReadError> {
        let mut items = Vec::new();
        loop {
            self.skip_atmosphere();
            if let Some(')') = self.chars.peek() {
                self.chars.next();
                return Ok(Sexpr::List(items));
            }
            if self.peek_is_lone_dot() {
                self.chars.next(); // consume '.'
                self.skip_atmosphere();
                let tail = self
                    .read_form()?
                    .ok_or_else(|| err("expected a datum after '.' in a dotted list"))?;
                self.skip_atmosphere();
                return match self.chars.next() {
                    Some(')') => Ok(Sexpr::DottedList(items, Box::new(tail))),
                    _ => Err(err(
                        "expected ')' immediately after the tail of a dotted list",
                    )),
                };
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
        atom_from_text(&text)
    }
}

/// Whether `text` should be parsed as a number at all, vs. treated as an
/// ordinary symbol. A token counts as numeric-looking if, after an optional
/// leading sign, it starts with a digit, or with a decimal point that is
/// itself followed by a digit — e.g. `-`, `->vector`, `+`, and a lone `.`
/// (the dotted-pair marker) all stay symbols, while `-7`, `.5`, and `-.5`
/// are numeric.
fn looks_numeric(text: &str) -> bool {
    let mut chars = text.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    let first_digit_candidate = if first == '+' || first == '-' {
        chars.next()
    } else {
        Some(first)
    };
    match first_digit_candidate {
        Some(c) if c.is_ascii_digit() => true,
        Some('.') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    }
}

fn parse_radix_int(body: &str, radix: u32, original: &str) -> Result<Sexpr, ReadError> {
    i64::from_str_radix(body, radix)
        .map(Sexpr::Int)
        .map_err(|_| err(format!("invalid or out-of-range radix literal: {original}")))
}

fn atom_from_text(text: &str) -> Result<Sexpr, ReadError> {
    match text {
        "true" | "#t" => return Ok(Sexpr::Bool(true)),
        "false" | "#f" => return Ok(Sexpr::Bool(false)),
        _ => {}
    }
    if let Some(body) = text.strip_prefix("#x").or_else(|| text.strip_prefix("#X")) {
        return parse_radix_int(body, 16, text);
    }
    if let Some(body) = text.strip_prefix("#b").or_else(|| text.strip_prefix("#B")) {
        return parse_radix_int(body, 2, text);
    }
    if let Some(body) = text.strip_prefix("#o").or_else(|| text.strip_prefix("#O")) {
        return parse_radix_int(body, 8, text);
    }
    if looks_numeric(text) {
        return if text.contains('.') || text.contains('e') || text.contains('E') {
            text.parse::<f64>()
                .map(Sexpr::Float)
                .map_err(|_| err(format!("invalid float literal: {text}")))
        } else {
            text.parse::<i64>()
                .map(Sexpr::Int)
                .map_err(|_| err(format!("integer literal out of range or malformed: {text}")))
        };
    }
    Ok(Sexpr::Symbol(text.to_string()))
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
    fn reads_a_decimal_point_float() {
        assert_eq!(read_program("1.5").unwrap(), vec![Sexpr::Float(1.5)]);
    }

    #[test]
    fn reads_a_leading_dot_float() {
        assert_eq!(read_program(".5").unwrap(), vec![Sexpr::Float(0.5)]);
    }

    #[test]
    fn reads_a_negative_leading_dot_float() {
        assert_eq!(read_program("-.5").unwrap(), vec![Sexpr::Float(-0.5)]);
    }

    #[test]
    fn reads_an_exponent_form_float_with_no_decimal_point() {
        assert_eq!(read_program("1e3").unwrap(), vec![Sexpr::Float(1000.0)]);
    }

    #[test]
    fn reads_a_decimal_point_and_exponent_float() {
        assert_eq!(read_program("1.5e-3").unwrap(), vec![Sexpr::Float(0.0015)]);
    }

    #[test]
    fn a_lone_dot_stays_a_symbol_not_a_malformed_float() {
        // Guards looks_numeric's ".5 is numeric but a bare '.' is not"
        // distinction directly, independent of dotted-pair-list parsing.
        assert_eq!(
            read_program(".").unwrap(),
            vec![Sexpr::Symbol(".".to_string())]
        );
    }

    #[test]
    fn reads_a_hexadecimal_integer_literal() {
        assert_eq!(read_program("#x1A").unwrap(), vec![Sexpr::Int(26)]);
    }

    #[test]
    fn reads_a_binary_integer_literal() {
        assert_eq!(read_program("#b101").unwrap(), vec![Sexpr::Int(5)]);
    }

    #[test]
    fn reads_an_octal_integer_literal() {
        assert_eq!(read_program("#o17").unwrap(), vec![Sexpr::Int(15)]);
    }

    #[test]
    fn reads_a_negative_radix_integer_literal() {
        assert_eq!(read_program("#x-1A").unwrap(), vec![Sexpr::Int(-26)]);
    }

    #[test]
    fn an_integer_literal_outside_the_signed_64_bit_range_is_a_read_error() {
        let src = format!("{}0", i64::MAX);
        let err = read_program(&src).unwrap_err();
        assert!(
            err.message.contains("range") || err.message.contains("malformed"),
            "expected an out-of-range error, got: {}",
            err.message
        );
    }

    #[test]
    fn an_out_of_range_hex_literal_is_a_read_error() {
        assert!(read_program("#xFFFFFFFFFFFFFFFFF").is_err());
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
        // Structural, not just a count: proves the comment was skipped (no
        // stray form for it), the string escapes decoded correctly, the list
        // nested (not flattened), and both booleans read distinctly.
        assert_eq!(
            forms,
            vec![
                Sexpr::List(vec![
                    Sexpr::Symbol("display".to_string()),
                    Sexpr::Str("a\nb\tc\r\"d\\e".to_string()),
                ]),
                Sexpr::List(vec![Sexpr::Symbol("newline".to_string())]),
                Sexpr::List(vec![
                    Sexpr::Symbol("display".to_string()),
                    Sexpr::List(vec![
                        Sexpr::Symbol("+".to_string()),
                        Sexpr::Int(42),
                        Sexpr::List(vec![
                            Sexpr::Symbol("+".to_string()),
                            Sexpr::Int(1),
                            Sexpr::Int(2),
                        ]),
                    ]),
                ]),
                Sexpr::List(vec![Sexpr::Symbol("newline".to_string())]),
                Sexpr::List(vec![
                    Sexpr::Symbol("display".to_string()),
                    Sexpr::Bool(true),
                ]),
                Sexpr::List(vec![Sexpr::Symbol("newline".to_string())]),
                Sexpr::List(vec![
                    Sexpr::Symbol("display".to_string()),
                    Sexpr::Bool(false),
                ]),
                Sexpr::List(vec![Sexpr::Symbol("newline".to_string())]),
            ]
        );
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
    fn an_open_paren_ends_a_preceding_atom_without_whitespace() {
        assert_eq!(
            read_program("a(b)").unwrap(),
            vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::List(vec![Sexpr::Symbol("b".to_string())]),
            ]
        );
    }

    #[test]
    fn a_close_paren_ends_a_preceding_atom_without_whitespace() {
        assert_eq!(
            read_program("(a)b").unwrap(),
            vec![
                Sexpr::List(vec![Sexpr::Symbol("a".to_string())]),
                Sexpr::Symbol("b".to_string()),
            ]
        );
    }

    #[test]
    fn a_semicolon_ends_a_preceding_atom_without_whitespace() {
        assert_eq!(
            read_program("a;comment\nb").unwrap(),
            vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::Symbol("b".to_string())
            ]
        );
    }

    #[test]
    fn a_double_quote_ends_a_preceding_atom_without_whitespace() {
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
        // MAX_NESTING_DEPTH - 1 opens, not MAX_NESTING_DEPTH: since depth is
        // now incremented once per read_form call (see its doc comment),
        // the innermost atom itself consumes one unit of budget too, so
        // MAX_NESTING_DEPTH levels of '(' would land one over the limit.
        let src = format!(
            "{}1{}",
            "(".repeat(MAX_NESTING_DEPTH - 1),
            ")".repeat(MAX_NESTING_DEPTH - 1)
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

    #[test]
    fn reads_quote_shorthand_on_a_symbol_as_a_quote_form() {
        assert_eq!(
            read_program("'x").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("quote".to_string()),
                Sexpr::Symbol("x".to_string()),
            ])]
        );
    }

    #[test]
    fn reads_quote_shorthand_around_a_list() {
        assert_eq!(
            read_program("'(+ 1 2)").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("quote".to_string()),
                Sexpr::List(vec![
                    Sexpr::Symbol("+".to_string()),
                    Sexpr::Int(1),
                    Sexpr::Int(2),
                ]),
            ])]
        );
    }

    #[test]
    fn a_quote_character_ends_a_preceding_atom_without_whitespace() {
        assert_eq!(
            read_program("a'b").unwrap(),
            vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::List(vec![
                    Sexpr::Symbol("quote".to_string()),
                    Sexpr::Symbol("b".to_string()),
                ]),
            ]
        );
    }

    #[test]
    fn reads_hash_t_and_hash_f_as_booleans() {
        assert_eq!(
            read_program("#t #f").unwrap(),
            vec![Sexpr::Bool(true), Sexpr::Bool(false)]
        );
    }

    #[test]
    fn reads_a_dotted_pair_with_a_single_fixed_head_item() {
        assert_eq!(
            read_program("(a . b)").unwrap(),
            vec![Sexpr::DottedList(
                vec![Sexpr::Symbol("a".to_string())],
                Box::new(Sexpr::Symbol("b".to_string())),
            )]
        );
    }

    #[test]
    fn reads_a_dotted_list_with_multiple_fixed_head_items() {
        assert_eq!(
            read_program("(a b . c)").unwrap(),
            vec![Sexpr::DottedList(
                vec![
                    Sexpr::Symbol("a".to_string()),
                    Sexpr::Symbol("b".to_string())
                ],
                Box::new(Sexpr::Symbol("c".to_string())),
            )]
        );
    }

    #[test]
    fn rejects_a_dotted_list_missing_a_tail_datum() {
        assert!(read_program("(a . )").is_err());
    }

    #[test]
    fn rejects_a_dotted_list_with_extra_items_after_the_tail() {
        assert!(read_program("(a . b c)").is_err());
    }

    #[test]
    fn a_lone_dot_symbol_outside_a_list_is_still_read_as_a_symbol() {
        // The dotted-pair marker is only special inside a list body.
        assert_eq!(
            read_program(".").unwrap(),
            vec![Sexpr::Symbol(".".to_string())]
        );
    }

    #[test]
    fn quote_shorthand_nesting_up_to_the_configured_maximum_still_succeeds() {
        // MAX_NESTING_DEPTH - 1, matching the analogous list-nesting boundary
        // test above (the trailing atom also consumes one unit of budget).
        let src = format!("{}x", "'".repeat(MAX_NESTING_DEPTH - 1));
        assert!(read_program(&src).is_ok());
    }

    #[test]
    fn quote_shorthand_nesting_is_bounded_by_the_same_depth_limit_as_lists() {
        // A security-review finding on B2: the quote-shorthand ('x) parsing
        // recursed into read_form with no depth accounting at all, bypassing
        // the guard that already protected '('-nesting — a source file of
        // repeated quote characters could abort the process via native stack
        // overflow before this guard was consulted.
        let src = format!("{}x", "'".repeat(MAX_NESTING_DEPTH + 1));
        let err = read_program(&src).unwrap_err();
        assert!(
            err.message.contains("nesting") && err.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            err.message
        );
    }
}
