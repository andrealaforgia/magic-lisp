//! Reads MagicLisp source text into a tree of [`Sexpr`] values.

#[derive(Debug, Clone, PartialEq)]
pub enum Sexpr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    Symbol(String),
    List(Vec<Sexpr>),
    /// `(a b . rest)` — an improper list with a fixed head and a non-list
    /// tail. Used exclusively for parameter-list syntax in this language.
    DottedList(Vec<Sexpr>, Box<Sexpr>),
    /// `#(a b c)` — a vector literal (spec 3.1 grammar: `vector-lit`).
    Vector(Vec<Sexpr>),
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

    /// Everything not yet consumed, as an owned string -- used by
    /// [`read_one`], whose caller needs the leftover text handed back to
    /// continue a later read from wherever this one left off.
    fn remaining(&self) -> String {
        self.chars.clone().collect()
    }

    /// Skips whitespace and every comment form (spec 3.1): a `;` line
    /// comment, a `#| ... |#` block comment (which NESTS -- a complete
    /// `#| |#` fully inside an outer one is consumed as part of the
    /// outer, not treated as ending it early), and a `#;` datum comment
    /// (which discards exactly the one complete datum immediately
    /// following it, via an ordinary recursive `read_form` call, as if
    /// that datum had never been written).
    ///
    /// Fallible (unlike before B19): a `#|` that never finds its closing
    /// `|#`, or a `#;` at the very end of input with no datum following
    /// it to discard, are both read errors now, not silently accepted.
    fn skip_atmosphere(&mut self) -> Result<(), ReadError> {
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
                Some('#') => {
                    if !self.skip_hash_atmosphere()? {
                        break;
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// The `#`-prefixed half of [`Self::skip_atmosphere`], split into its
    /// own `#[inline(never)]` function so its extra local state (the
    /// lookahead clone, the nested match) never inflates the stack frame
    /// of the ordinary whitespace/`;`-comment path -- that path runs once
    /// per level of `read_form`'s own list-nesting recursion, so growing
    /// its frame size at all risks tipping an already precisely-
    /// calibrated `MAX_NESTING_DEPTH` over into a real stack overflow
    /// (confirmed: inlining this directly into `skip_atmosphere`
    /// regressed `accepts_nesting_of_exactly_the_configured_maximum_depth`
    /// from passing to a genuine native stack overflow, with no other
    /// change to the recursion's own call count).
    ///
    /// Returns `true` if a comment was consumed (caller should keep
    /// looping) and `false` if the `#` starts a real datum instead (`#t`,
    /// `#x10`, `#(`, `#\a`, ...), left unconsumed for `read_form_body`'s
    /// own dispatch to handle.
    #[inline(never)]
    fn skip_hash_atmosphere(&mut self) -> Result<bool, ReadError> {
        let mut lookahead = self.chars.clone();
        lookahead.next(); // '#'
        match lookahead.peek() {
            Some('|') => {
                self.skip_block_comment()?;
                Ok(true)
            }
            Some(';') => {
                self.chars.next(); // '#'
                self.chars.next(); // ';'
                self.read_form()?
                    .ok_or_else(|| err("expected a datum after '#;'"))?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Consumes a `#| ... |#` block comment already confirmed to start at
    /// the current position, tracking nesting depth with a plain counter
    /// (not recursion, so no nesting-depth limit applies here the way it
    /// does for datum structure -- a long CHAIN of nested markers costs no
    /// native stack at all, just one loop iteration each).
    fn skip_block_comment(&mut self) -> Result<(), ReadError> {
        self.chars.next(); // '#'
        self.chars.next(); // '|'
        let mut depth = 1usize;
        loop {
            match self.chars.next() {
                None => return Err(err("unterminated block comment: missing '|#'")),
                Some('#') if self.chars.peek() == Some(&'|') => {
                    self.chars.next();
                    depth += 1;
                }
                Some('|') if self.chars.peek() == Some(&'#') => {
                    self.chars.next();
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn is_delimiter(c: char) -> bool {
        c.is_whitespace()
            || c == '('
            || c == ')'
            || c == '"'
            || c == ';'
            || c == '\''
            || c == '`'
            || c == ','
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
        self.skip_atmosphere()?;
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
            Some('#') => self.read_hash_form(),
            Some('\'') => {
                self.chars.next();
                let datum = self
                    .read_form()?
                    .ok_or_else(|| err("expected a datum after '\''"))?;
                Ok(Sexpr::List(vec![Sexpr::Symbol("quote".to_string()), datum]))
            }
            // `` `x `` (quasiquote) and `,x`/`,@x` (unquote/unquote-splicing,
            // spec 3.4) desugar exactly like `'x` above -- routed through
            // `self.read_form()`, not `read_form_body` directly, so the same
            // `MAX_NESTING_DEPTH` guard that already covers quote-shorthand
            // nesting covers these by construction too.
            Some('`') => {
                self.chars.next();
                let datum = self
                    .read_form()?
                    .ok_or_else(|| err("expected a datum after '`'"))?;
                Ok(Sexpr::List(vec![
                    Sexpr::Symbol("quasiquote".to_string()),
                    datum,
                ]))
            }
            Some(',') => {
                self.chars.next();
                let splicing = self.chars.peek() == Some(&'@');
                if splicing {
                    self.chars.next();
                }
                let tag = if splicing {
                    "unquote-splicing"
                } else {
                    "unquote"
                };
                let datum = self.read_form()?.ok_or_else(|| {
                    err(format!(
                        "expected a datum after ',{}'",
                        if splicing { "@" } else { "" }
                    ))
                })?;
                Ok(Sexpr::List(vec![Sexpr::Symbol(tag.to_string()), datum]))
            }
            Some(_) => self.read_atom(),
        }
    }

    /// Dispatches every `#`-prefixed form. `#\` (character literals) and
    /// `#(` (vector literals) need dedicated readers, since their bodies
    /// can themselves contain delimiter characters (`#\(` is the literal
    /// `(` character; `#(1 2)` nests ordinary datum syntax) that
    /// `read_atom`'s single-token tokenizer isn't equipped to handle.
    /// Everything else `#`-prefixed (`#t`, `#f`, `#x..`/`#b..`/`#o..`
    /// radix literals) is an ordinary delimiter-bounded token, unaffected
    /// by this dispatch, and falls through to the existing `read_atom` path.
    fn read_hash_form(&mut self) -> Result<Sexpr, ReadError> {
        let mut lookahead = self.chars.clone();
        lookahead.next(); // '#' itself
        match lookahead.peek() {
            Some('(') => {
                self.chars.next(); // '#'
                self.chars.next(); // '('
                self.read_vector_body()
            }
            Some('\\') => self.read_character(),
            _ => self.read_atom(),
        }
    }

    fn read_character(&mut self) -> Result<Sexpr, ReadError> {
        self.chars.next(); // '#'
        self.chars.next(); // '\'
        let first = self
            .chars
            .next()
            .ok_or_else(|| err("expected a character after '#\\'"))?;
        // A single non-alphabetic character (e.g. `(`, a digit, a symbol
        // character) is always the literal itself, never the start of a
        // named form like "space" -- only letters continue into a name.
        let mut name = String::new();
        name.push(first);
        if first.is_alphabetic() {
            while let Some(&c) = self.chars.peek() {
                if Self::is_delimiter(c) {
                    break;
                }
                name.push(c);
                self.chars.next();
            }
        }
        let mut chars = name.chars();
        let ch = match (chars.next(), chars.as_str()) {
            (Some(only), "") => only,
            (Some(_), _) => match name.as_str() {
                "space" => ' ',
                "newline" => '\n',
                "tab" => '\t',
                other => {
                    return Err(err(format!(
                        "unknown named character literal: '#\\{other}'"
                    )));
                }
            },
            (None, _) => unreachable!("name always has at least the first char pushed"),
        };
        Ok(Sexpr::Char(ch))
    }

    fn read_vector_body(&mut self) -> Result<Sexpr, ReadError> {
        let mut items = Vec::new();
        loop {
            self.skip_atmosphere()?;
            if let Some(')') = self.chars.peek() {
                self.chars.next();
                return Ok(Sexpr::Vector(items));
            }
            let form = self
                .read_form()?
                .ok_or_else(|| err("unterminated vector literal: missing ')'"))?;
            items.push(form);
        }
    }

    fn read_list_body(&mut self) -> Result<Sexpr, ReadError> {
        let mut items = Vec::new();
        loop {
            self.skip_atmosphere()?;
            if let Some(')') = self.chars.peek() {
                self.chars.next();
                return Ok(Sexpr::List(items));
            }
            if self.peek_is_lone_dot() {
                self.chars.next(); // consume '.'
                self.skip_atmosphere()?;
                let tail = self
                    .read_form()?
                    .ok_or_else(|| err("expected a datum after '.' in a dotted list"))?;
                self.skip_atmosphere()?;
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

/// Reads exactly one datum from `src`, returning it together with
/// everything left unconsumed afterward -- for callers (the `read` native,
/// spec 4.8) that read one unit of data at a time from a stream, each call
/// continuing from wherever the previous one left off, rather than parsing
/// a whole program's worth of text up front like [`read_program`] does.
/// Returns `Ok((None, ...))` (with only leading whitespace/comments
/// consumed) at end of input, mirroring `read_program`'s own "no more
/// forms" signal.
pub fn read_one(src: &str) -> Result<(Option<Sexpr>, String), ReadError> {
    let mut scanner = Scanner::new(src);
    let form = scanner.read_form()?;
    Ok((form, scanner.remaining()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `read_program` on a dedicated, fixed-size thread rather than
    /// whatever thread the test harness happens to spawn a given test on
    /// -- that harness default is neither controlled nor guaranteed by
    /// this project (unlike the stack sizes deliberately chosen elsewhere
    /// in this codebase, e.g. `COMPILE_STACK_SIZE`/`VM_STACK_SIZE`), so a
    /// boundary-depth test pinned only to it is fragile against ordinary
    /// codegen drift (B19: adding `#|`/`#;` support grew `read_form`'s own
    /// recursive call chain just enough that several of these exact tests
    /// started genuinely overflowing the stack under the harness's
    /// previously-adequate default, with no change to their own logical
    /// nesting depth). 8 MiB comfortably exceeds a typical OS process
    /// main-thread stack (this reader has no dedicated big-stack thread
    /// of its own in production either -- `cli.rs`'s `compile_source`
    /// calls `read_program` directly, before `compile_program`'s own
    /// dedicated thread ever starts), so this pins every boundary-depth
    /// test to the same real-world budget production code actually runs
    /// under, rather than an incidental, uncontrolled harness default.
    fn read_program_on_a_fixed_stack(src: &str) -> Result<Vec<Sexpr>, ReadError> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(8 * 1024 * 1024)
                .spawn_scoped(scope, || read_program(src))
                .expect("should spawn the fixed-size thread")
                .join()
                .expect("read_program itself must not crash the calling thread")
        })
    }

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
    fn an_invalid_digit_for_binary_is_a_read_error() {
        // qa flagged radix coverage as narrower than the rest of B4's
        // discipline: only overflow was tested, not an invalid digit for
        // the given radix (2 isn't a valid binary digit).
        assert!(read_program("#b2").is_err());
    }

    #[test]
    fn an_invalid_digit_for_octal_is_a_read_error() {
        // 8 isn't a valid octal digit.
        assert!(read_program("#o8").is_err());
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

    // --- B19: block comments (#| |#, nesting) and the #; datum comment ---

    #[test]
    fn skips_a_single_block_comment() {
        let src = "#| a block comment |# (display 1)";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("display".to_string()),
                Sexpr::Int(1),
            ])]
        );
    }

    #[test]
    fn a_block_comment_fully_containing_another_is_consumed_as_one_outer_comment() {
        // The load-bearing nesting case: the inner `#| |#` must NOT end
        // the outer one early -- if it did, " still outer |#" would be
        // left as leftover (non-comment) source text and fail to read as
        // a valid datum.
        let src = "#| outer #| nested |# still outer |# (display 2)";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("display".to_string()),
                Sexpr::Int(2),
            ])]
        );
    }

    #[test]
    fn rejects_a_block_comment_missing_its_closing_marker() {
        assert!(read_program("#| never closed (display 1)").is_err());
    }

    #[test]
    fn a_bare_hash_inside_a_block_comment_not_followed_by_a_pipe_does_not_open_a_nested_comment() {
        // A `#` on its own (not immediately followed by `|`) must not be
        // mistaken for the start of a nested block comment -- otherwise
        // the depth counter would over-count, and the comment would keep
        // consuming source text (potentially past its own real closing
        // marker) looking for an extra `|#` that was never opened.
        let src = "#| a # b |# (display 1)";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("display".to_string()),
                Sexpr::Int(1),
            ])]
        );
    }

    #[test]
    fn a_bare_pipe_inside_a_block_comment_not_followed_by_a_hash_does_not_close_it_early() {
        // Mirrors the bare-`#` case above: a `|` on its own (not
        // immediately followed by `#`) must not be mistaken for a closing
        // marker -- otherwise the comment would end early, right after
        // the bare `|`, spilling the rest of the intended comment out as
        // real source text.
        let src = "#| a | b |# (display 2)";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("display".to_string()),
                Sexpr::Int(2),
            ])]
        );
    }

    #[test]
    fn a_datum_comment_removes_exactly_the_next_bare_datum() {
        let src = "(+ 1 #;99 2)";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("+".to_string()),
                Sexpr::Int(1),
                Sexpr::Int(2),
            ])]
        );
    }

    #[test]
    fn a_datum_comment_removes_one_whole_compound_datum_not_just_one_token() {
        // Proves the marker discards an entire following datum regardless
        // of how many tokens it spans -- a compound list, not just a bare
        // atom -- rather than e.g. only swallowing the next single token.
        let src = "(+ 1 #;(a b c) 2)";
        assert_eq!(
            read_program(src).unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("+".to_string()),
                Sexpr::Int(1),
                Sexpr::Int(2),
            ])]
        );
    }

    #[test]
    fn rejects_a_datum_comment_with_no_datum_following_it() {
        assert!(read_program("(display 1) #;").is_err());
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
        assert!(read_program_on_a_fixed_stack(&src).is_ok());
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
        let err = read_program_on_a_fixed_stack(&src).unwrap_err();
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
        assert!(read_program_on_a_fixed_stack(&src).is_ok());
    }

    #[test]
    fn quote_shorthand_nesting_is_bounded_by_the_same_depth_limit_as_lists() {
        // A security-review finding on B2: the quote-shorthand ('x) parsing
        // recursed into read_form with no depth accounting at all, bypassing
        // the guard that already protected '('-nesting — a source file of
        // repeated quote characters could abort the process via native stack
        // overflow before this guard was consulted.
        let src = format!("{}x", "'".repeat(MAX_NESTING_DEPTH + 1));
        let err = read_program_on_a_fixed_stack(&src).unwrap_err();
        assert!(
            err.message.contains("nesting") && err.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            err.message
        );
    }

    // --- B8: character and vector literals (spec 3.1 grammar) ---

    #[test]
    fn reads_a_plain_character_literal() {
        assert_eq!(read_program("#\\a").unwrap(), vec![Sexpr::Char('a')]);
    }

    #[test]
    fn reads_a_character_literal_whose_own_character_is_a_delimiter() {
        // '(' is itself a delimiter that read_atom's ordinary tokenizer
        // would stop at -- #\( needs dedicated handling to read correctly
        // rather than misparsing as an empty token followed by a list.
        assert_eq!(read_program("#\\(").unwrap(), vec![Sexpr::Char('(')]);
    }

    #[test]
    fn reads_the_named_space_character_literal() {
        assert_eq!(read_program("#\\space").unwrap(), vec![Sexpr::Char(' ')]);
    }

    #[test]
    fn reads_the_named_newline_character_literal() {
        assert_eq!(read_program("#\\newline").unwrap(), vec![Sexpr::Char('\n')]);
    }

    #[test]
    fn reads_the_named_tab_character_literal() {
        assert_eq!(read_program("#\\tab").unwrap(), vec![Sexpr::Char('\t')]);
    }

    #[test]
    fn rejects_an_unknown_named_character_literal() {
        assert!(read_program("#\\bogus").is_err());
    }

    #[test]
    fn reads_an_empty_vector_literal() {
        assert_eq!(read_program("#()").unwrap(), vec![Sexpr::Vector(vec![])]);
    }

    #[test]
    fn reads_a_vector_literal_with_elements() {
        assert_eq!(
            read_program("#(1 2 3)").unwrap(),
            vec![Sexpr::Vector(vec![
                Sexpr::Int(1),
                Sexpr::Int(2),
                Sexpr::Int(3)
            ])]
        );
    }

    #[test]
    fn reads_a_nested_vector_literal() {
        assert_eq!(
            read_program("#(1 #(2 3))").unwrap(),
            vec![Sexpr::Vector(vec![
                Sexpr::Int(1),
                Sexpr::Vector(vec![Sexpr::Int(2), Sexpr::Int(3)]),
            ])]
        );
    }

    #[test]
    fn rejects_an_unterminated_vector_literal() {
        assert!(read_program("#(1 2").is_err());
    }

    #[test]
    fn still_reads_hash_t_and_hash_f_after_adding_hash_dispatch() {
        // #t/#f/#x../#b../#o.. must keep routing through the ordinary
        // read_atom path now that '#' has a dedicated dispatcher.
        assert_eq!(read_program("#t").unwrap(), vec![Sexpr::Bool(true)]);
        assert_eq!(read_program("#f").unwrap(), vec![Sexpr::Bool(false)]);
        assert_eq!(read_program("#x1A").unwrap(), vec![Sexpr::Int(26)]);
    }

    // --- B12 E1: read_one, spec 4.8's underlying single-datum reader ---

    #[test]
    fn read_one_reads_a_single_datum_and_returns_the_rest_unconsumed() {
        let (form, rest) = read_one("(+ 1 2) (+ 3 4)").unwrap();
        assert_eq!(
            form,
            Some(Sexpr::List(vec![
                Sexpr::Symbol("+".to_string()),
                Sexpr::Int(1),
                Sexpr::Int(2),
            ]))
        );
        assert_eq!(rest, " (+ 3 4)");
    }

    #[test]
    fn read_one_advances_correctly_across_two_consecutive_calls() {
        let (first, rest) = read_one("1 2").unwrap();
        assert_eq!(first, Some(Sexpr::Int(1)));
        let (second, rest) = read_one(&rest).unwrap();
        assert_eq!(second, Some(Sexpr::Int(2)));
        assert_eq!(rest, "");
    }

    #[test]
    fn read_one_returns_none_at_end_of_input() {
        let (form, rest) = read_one("   ").unwrap();
        assert_eq!(form, None);
        assert_eq!(rest, "");
    }

    #[test]
    fn read_one_on_an_unterminated_datum_is_a_clean_error_not_a_partial_success() {
        // A malformed/incomplete list (e.g. streamed input that hasn't
        // finished arriving yet) must error cleanly, not silently succeed
        // with a truncated or corrupted `Sexpr` -- a caller (the `read`
        // native) that retries once more input has arrived needs a clean
        // signal to distinguish "not done yet" from "a real parse".
        assert!(read_one("(1 2").is_err());
    }

    #[test]
    fn a_caller_can_retry_read_one_with_growing_input_after_an_unterminated_error() {
        // Confirms retrying against the SAME logical stream, once complete,
        // still parses correctly -- an error on incomplete input doesn't
        // leave any state that could corrupt a subsequent, complete attempt
        // (read_one is stateless: each call is independent of any prior
        // one, so this is really confirming that independence holds).
        assert!(read_one("(1 2").is_err());
        let (form, rest) = read_one("(1 2 3)").unwrap();
        assert_eq!(
            form,
            Some(Sexpr::List(vec![
                Sexpr::Int(1),
                Sexpr::Int(2),
                Sexpr::Int(3)
            ]))
        );
        assert_eq!(rest, "");
    }

    // --- B13: quasiquote/unquote/unquote-splicing shorthand (spec 3.4) ---

    #[test]
    fn reads_quasiquote_shorthand_on_a_symbol_as_a_quasiquote_form() {
        assert_eq!(
            read_program("`x").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("quasiquote".to_string()),
                Sexpr::Symbol("x".to_string()),
            ])]
        );
    }

    #[test]
    fn reads_unquote_shorthand_on_a_symbol_as_an_unquote_form() {
        assert_eq!(
            read_program(",x").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("unquote".to_string()),
                Sexpr::Symbol("x".to_string()),
            ])]
        );
    }

    #[test]
    fn reads_unquote_splicing_shorthand_distinctly_from_plain_unquote() {
        assert_eq!(
            read_program(",@x").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("unquote-splicing".to_string()),
                Sexpr::Symbol("x".to_string()),
            ])]
        );
    }

    #[test]
    fn reads_a_quasiquoted_list_containing_unquote_and_unquote_splicing() {
        assert_eq!(
            read_program("`(a ,b ,@c)").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("quasiquote".to_string()),
                Sexpr::List(vec![
                    Sexpr::Symbol("a".to_string()),
                    Sexpr::List(vec![
                        Sexpr::Symbol("unquote".to_string()),
                        Sexpr::Symbol("b".to_string()),
                    ]),
                    Sexpr::List(vec![
                        Sexpr::Symbol("unquote-splicing".to_string()),
                        Sexpr::Symbol("c".to_string()),
                    ]),
                ]),
            ])]
        );
    }

    #[test]
    fn a_backtick_or_comma_ends_a_preceding_atom_without_whitespace() {
        assert_eq!(
            read_program("(a`b,c)").unwrap(),
            vec![Sexpr::List(vec![
                Sexpr::Symbol("a".to_string()),
                Sexpr::List(vec![
                    Sexpr::Symbol("quasiquote".to_string()),
                    Sexpr::Symbol("b".to_string()),
                ]),
                Sexpr::List(vec![
                    Sexpr::Symbol("unquote".to_string()),
                    Sexpr::Symbol("c".to_string()),
                ]),
            ])]
        );
    }

    #[test]
    fn quasiquote_shorthand_nesting_up_to_the_configured_maximum_still_succeeds() {
        let src = format!("{}x", "`".repeat(MAX_NESTING_DEPTH - 1));
        assert!(read_program_on_a_fixed_stack(&src).is_ok());
    }

    #[test]
    fn quasiquote_shorthand_nesting_is_bounded_by_the_same_depth_limit_as_lists() {
        // Mirrors the analogous quote-shorthand security-review fix: `` `x ``
        // and `,x` route through `self.read_form()`, the same depth-checked
        // entry point, not a bespoke unguarded recursion.
        let src = format!("{}x", "`".repeat(MAX_NESTING_DEPTH + 1));
        let err = read_program_on_a_fixed_stack(&src).unwrap_err();
        assert!(
            err.message.contains("nesting") && err.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            err.message
        );
    }

    #[test]
    fn unquote_shorthand_nesting_is_bounded_by_the_same_depth_limit_as_lists() {
        // qa test-design review (msg #219): the backtick-repeated boundary
        // test above doesn't, by itself, prove the SAME holds for `,` and
        // `,@` -- structurally identical in the code, but not directly
        // tested until now.
        let src = format!("{}x", ",".repeat(MAX_NESTING_DEPTH + 1));
        let err = read_program_on_a_fixed_stack(&src).unwrap_err();
        assert!(
            err.message.contains("nesting") && err.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            err.message
        );
    }

    #[test]
    fn unquote_splicing_shorthand_nesting_is_bounded_by_the_same_depth_limit_as_lists() {
        let src = format!("{}x", ",@".repeat(MAX_NESTING_DEPTH + 1));
        let err = read_program_on_a_fixed_stack(&src).unwrap_err();
        assert!(
            err.message.contains("nesting") && err.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            err.message
        );
    }

    #[test]
    fn unquote_with_no_following_datum_is_a_clean_read_error() {
        assert!(read_program(",").is_err());
        assert!(read_program(",@").is_err());
    }
}
