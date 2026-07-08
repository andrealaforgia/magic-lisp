//! Step definitions for features/B10-strings-and-characters.feature.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::registry::Registry;
use super::world::{eval_ok, run, stderr_of, write_source};

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a string, a position within it, a sub-range within it, three or more strings to join, and positions/ranges outside its bounds",
            |_w, _text, _| {},
        )
        .step(
            "length, character retrieval, sub-range extraction, and joining are applied",
            |w, _text, _| {
                let out = eval_ok(
                    "b10-e1.ml",
                    "(display (string-length \"hello\")) \
                     (display (string-ref \"hello\" 1)) \
                     (display (substring \"hello\" 1 4)) \
                     (display (string-append \"foo\" \"bar\" \"baz\"))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "each returns the correct value, three-or-more-string joining works, and out-of-bounds retrieval/extraction is a clean runtime error",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "5eellfoobarbaz");
                for src in ["(display (string-ref \"hello\" 5))", "(display (substring \"hello\" 1 10))"] {
                    let file = write_source("b10-e1-oob.ml", src);
                    let output = run(&["eval", file.to_str().unwrap()]);
                    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                    assert!(!stderr_of(&output).is_empty());
                }
            },
        )
        // --- E2 ---
        .step(
            "equal and unequal string pairs, and pairs ordered each way",
            |_w, _text, _| {},
        )
        .step(
            "string=?, string<?, and string>? are applied",
            |w, _text, _| {
                let out = eval_ok(
                    "b10-e2.ml",
                    "(display (string=? \"abc\" \"abc\")) \
                     (display (string=? \"abc\" \"abd\")) \
                     (display (string<? \"abc\" \"abd\")) \
                     (display (string<? \"abd\" \"abc\")) \
                     (display (string>? \"abd\" \"abc\")) \
                     (display (string>? \"abc\" \"abd\"))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "each returns #t on its matching direction and #f on the reverse, proving genuine comparison rather than a stub",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "#t#f#t#f#t#f");
            },
        )
        // --- E3 ---
        .step(
            "a symbol, a string, a list of characters, and a round trip through string->symbol and back",
            |_w, _text, _| {},
        )
        .step("each conversion is applied", |w, _text, _| {
            let out = eval_ok(
                "b10-e3.ml",
                "(display (symbol->string (quote hello))) \
                 (display (string->symbol \"world\")) \
                 (display (list->string (list #\\h #\\i))) \
                 (display (string->list \"ab\")) \
                 (display (symbol->string (string->symbol \"round-trip\")))",
            );
            w.notes.push(out);
        })
        .step(
            "each produces the correct corresponding value, and the round trip reproduces the original string exactly",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "helloworldhi(a b)round-trip");
            },
        )
        // --- E4 ---
        .step(
            "a lowercase string, an uppercase string, and a string containing a German sharp-s (whose uppercase form expands to two letters)",
            |_w, _text, _| {},
        )
        .step(
            "string-upcase and string-downcase are applied",
            |w, _text, _| {
                let out = eval_ok(
                    "b10-e4.ml",
                    "(display (string-upcase \"abc\")) \
                     (display (string-downcase \"ABC\")) \
                     (display (string-upcase \"straße\"))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "each direction produces the correct result, including correct Unicode case-folding that changes the string's length",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "ABCabcSTRASSE");
            },
        )
        // --- E5 ---
        .step(
            "characters for code-point conversion, equal and unequal character pairs, ordered pairs each way, and matching/non-matching characters for each predicate",
            |_w, _text, _| {},
        )
        .step(
            "char->integer, integer->char, char=?, char<?, char-alphabetic?, char-numeric?, and char-whitespace? are applied",
            |w, _text, _| {
                let out = eval_ok(
                    "b10-e5.ml",
                    "(display (char->integer #\\A)) \
                     (display (integer->char 66)) \
                     (display (char=? #\\a #\\a)) \
                     (display (char=? #\\a #\\b)) \
                     (display (char<? #\\a #\\b)) \
                     (display (char-alphabetic? #\\a)) \
                     (display (char-numeric? #\\5)) \
                     (display (char-whitespace? #\\space)) \
                     (display (char-whitespace? #\\a))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "code-point conversion round-trips correctly, and every comparison/predicate is correct in BOTH directions, not just the matching case",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "65B#t#f#t#t#t#t#f");
                assert_eq!(
                    eval_ok("b10-e5-false-a.ml", "(display (char-alphabetic? #\\5))"),
                    "#f"
                );
                assert_eq!(
                    eval_ok("b10-e5-false-b.ml", "(display (char-numeric? #\\a))"),
                    "#f"
                );
                assert_eq!(
                    eval_ok("b10-e5-false-c.ml", "(display (char<? #\\b #\\a))"),
                    "#f"
                );
            },
        )
        // --- E6 ---
        .step(
            "a plain character literal and the named forms for space, newline, and tab",
            |_w, _text, _| {},
        )
        .step("each is converted to its code point", |w, _text, _| {
            let out = eval_ok(
                "b10-e6.ml",
                "(display (char->integer #\\a)) \
                 (display (char->integer #\\space)) \
                 (display (char->integer #\\newline)) \
                 (display (char->integer #\\tab))",
            );
            w.notes.push(out);
        })
        .step(
            "each yields the correct numeric value, unambiguously confirming the literal was read correctly",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "9732109");
            },
        )
        // --- E7 ---
        .step(
            "a string containing one plain letter and one accented (multi-byte) character",
            |_w, _text, _| {},
        )
        .step(
            "its length is measured and each position is retrieved",
            |w, _text, _| {
                let out = eval_ok(
                    "b10-e7.ml",
                    "(display (string-length \"aé\")) \
                     (display (string-ref \"aé\" 0)) \
                     (display (string-ref \"aé\" 1))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "the length counts exactly two characters, and each position retrieves its correct, distinct character (not swapped, not split by byte)",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "2aé");
            },
        )
        // --- E8 ---
        .step(
            "all seventeen DEMO expressions from the behaviour spec run together in one program",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let out = eval_ok(
                "b10-e8.ml",
                "(display (string-length \"hello\")) (newline) \
                 (display (string-ref \"hello\" 1)) (newline) \
                 (display (substring \"hello\" 1 4)) (newline) \
                 (display (string-append \"foo\" \"bar\")) (newline) \
                 (display (string=? \"abc\" \"abc\")) (newline) \
                 (display (string<? \"abc\" \"abd\")) (newline) \
                 (display (string-upcase \"abc\")) (newline) \
                 (display (symbol->string (quote hello))) (newline) \
                 (display (string->symbol \"world\")) (newline) \
                 (display (char->integer #\\A)) (newline) \
                 (display (integer->char 66)) (newline) \
                 (display (char-alphabetic? #\\a)) (newline) \
                 (display (char-numeric? #\\5)) (newline) \
                 (display (list->string (list #\\h #\\i))) (newline) \
                 (display (string->list \"ab\")) (newline) \
                 (display (string-length \"aé\")) (newline) \
                 (display (string-ref \"aé\" 1)) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "each line of output matches its prescribed value exactly, and the process exits 0",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "5\ne\nell\nfoobar\n#t\n#t\nABC\nhello\nworld\n65\nB\n#t\n#t\nhi\n(a b)\n2\né\n"
                );
            },
        )
}
