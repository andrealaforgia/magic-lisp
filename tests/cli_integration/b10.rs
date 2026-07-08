//! B10: strings and characters (spec 6.1, 6.2).

use super::helpers::{eval_ok, run_demo};
use magiclisp::unicode_fixtures::{ACCENTED_LETTER, TWO_CHAR_ACCENTED};

#[test]
fn b10_e1_length_ref_substring_append_and_bounds_errors() {
    assert_eq!(
        eval_ok("b10-e1a.ml", "(display (string-length \"hello\"))"),
        "5"
    );
    assert_eq!(
        eval_ok("b10-e1b.ml", "(display (string-ref \"hello\" 1))"),
        "e"
    );
    assert_eq!(
        eval_ok("b10-e1c.ml", "(display (substring \"hello\" 1 4))"),
        "ell"
    );
    assert_eq!(
        eval_ok(
            "b10-e1d.ml",
            "(display (string-append \"foo\" \"bar\" \"baz\"))"
        ),
        "foobarbaz"
    );
}

#[test]
fn b10_e2_string_equality_and_ordering_shown_both_ways() {
    assert_eq!(
        eval_ok("b10-e2a.ml", "(display (string=? \"abc\" \"abc\"))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b10-e2b.ml", "(display (string=? \"abc\" \"abd\"))"),
        "#f"
    );
    assert_eq!(
        eval_ok("b10-e2c.ml", "(display (string<? \"abc\" \"abd\"))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b10-e2d.ml", "(display (string<? \"abd\" \"abc\"))"),
        "#f"
    );
    assert_eq!(
        eval_ok("b10-e2e.ml", "(display (string>? \"abd\" \"abc\"))"),
        "#t"
    );
}

#[test]
fn b10_e3_string_symbol_and_character_list_conversions() {
    assert_eq!(
        eval_ok("b10-e3a.ml", "(display (symbol->string (quote hello)))"),
        "hello"
    );
    assert_eq!(
        eval_ok("b10-e3b.ml", "(display (string->symbol \"world\"))"),
        "world"
    );
    assert_eq!(
        eval_ok("b10-e3c.ml", "(display (list->string (list #\\h #\\i)))"),
        "hi"
    );
    assert_eq!(
        eval_ok("b10-e3d.ml", "(display (string->list \"ab\"))"),
        "(a b)"
    );
    assert_eq!(
        eval_ok(
            "b10-e3e.ml",
            "(display (symbol->string (string->symbol \"round-trip\")))"
        ),
        "round-trip"
    );
}

#[test]
fn b10_e4_string_upcase_and_downcase() {
    assert_eq!(
        eval_ok("b10-e4a.ml", "(display (string-upcase \"abc\"))"),
        "ABC"
    );
    assert_eq!(
        eval_ok("b10-e4b.ml", "(display (string-downcase \"ABC\"))"),
        "abc"
    );
}

#[test]
fn b10_e5_char_conversions_and_predicates() {
    assert_eq!(
        eval_ok("b10-e5a.ml", "(display (char->integer #\\A))"),
        "65"
    );
    assert_eq!(eval_ok("b10-e5b.ml", "(display (integer->char 66))"), "B");
    assert_eq!(eval_ok("b10-e5c.ml", "(display (char=? #\\a #\\a))"), "#t");
    assert_eq!(eval_ok("b10-e5d.ml", "(display (char=? #\\a #\\b))"), "#f");
    assert_eq!(eval_ok("b10-e5e.ml", "(display (char<? #\\a #\\b))"), "#t");
    assert_eq!(
        eval_ok("b10-e5f.ml", "(display (char-alphabetic? #\\a))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b10-e5g.ml", "(display (char-numeric? #\\5))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b10-e5h.ml", "(display (char-whitespace? #\\space))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b10-e5i.ml", "(display (char-whitespace? #\\a))"),
        "#f"
    );
    // Non-ASCII coverage (qa test-design review, msg #186): the unit-level
    // fix confirming char-alphabetic? is genuinely Unicode-aware, not
    // ASCII-only, had not threaded through to this CLI-integration layer.
    assert_eq!(
        eval_ok(
            "b10-e5j.ml",
            &format!("(display (char-alphabetic? #\\{ACCENTED_LETTER}))")
        ),
        "#t"
    );
}

#[test]
fn b10_e6_character_literal_code_points() {
    assert_eq!(
        eval_ok("b10-e6a.ml", "(display (char->integer #\\a))"),
        "97"
    );
    assert_eq!(
        eval_ok("b10-e6b.ml", "(display (char->integer #\\space))"),
        "32"
    );
    assert_eq!(
        eval_ok("b10-e6c.ml", "(display (char->integer #\\newline))"),
        "10"
    );
    assert_eq!(
        eval_ok("b10-e6d.ml", "(display (char->integer #\\tab))"),
        "9"
    );
}

#[test]
fn b10_e7_length_and_indexing_count_by_character_not_byte() {
    assert_eq!(
        eval_ok(
            "b10-e7a.ml",
            &format!("(display (string-length \"{TWO_CHAR_ACCENTED}\"))")
        ),
        "2"
    );
    assert_eq!(
        eval_ok(
            "b10-e7b.ml",
            &format!("(display (string-ref \"{TWO_CHAR_ACCENTED}\" 0))")
        ),
        "a"
    );
    assert_eq!(
        eval_ok(
            "b10-e7c.ml",
            &format!("(display (string-ref \"{TWO_CHAR_ACCENTED}\" 1))")
        ),
        "é"
    );
}

#[test]
fn b10_e8_all_seventeen_demo_expressions_produce_exactly_the_prescribed_output() {
    assert_eq!(
        run_demo("b10-e8-01.ml", "(display (string-length \"hello\"))"),
        "5\n"
    );
    assert_eq!(
        run_demo("b10-e8-02.ml", "(display (string-ref \"hello\" 1))"),
        "e\n"
    );
    assert_eq!(
        run_demo("b10-e8-03.ml", "(display (substring \"hello\" 1 4))"),
        "ell\n"
    );
    assert_eq!(
        run_demo("b10-e8-04.ml", "(display (string-append \"foo\" \"bar\"))"),
        "foobar\n"
    );
    assert_eq!(
        run_demo("b10-e8-05.ml", "(display (string=? \"abc\" \"abc\"))"),
        "#t\n"
    );
    assert_eq!(
        run_demo("b10-e8-06.ml", "(display (string<? \"abc\" \"abd\"))"),
        "#t\n"
    );
    assert_eq!(
        run_demo("b10-e8-07.ml", "(display (string-upcase \"abc\"))"),
        "ABC\n"
    );
    assert_eq!(
        run_demo("b10-e8-08.ml", "(display (symbol->string (quote hello)))"),
        "hello\n"
    );
    assert_eq!(
        run_demo("b10-e8-09.ml", "(display (string->symbol \"world\"))"),
        "world\n"
    );
    assert_eq!(
        run_demo("b10-e8-10.ml", "(display (char->integer #\\A))"),
        "65\n"
    );
    assert_eq!(
        run_demo("b10-e8-11.ml", "(display (integer->char 66))"),
        "B\n"
    );
    assert_eq!(
        run_demo("b10-e8-12.ml", "(display (char-alphabetic? #\\a))"),
        "#t\n"
    );
    assert_eq!(
        run_demo("b10-e8-13.ml", "(display (char-numeric? #\\5))"),
        "#t\n"
    );
    assert_eq!(
        run_demo("b10-e8-14.ml", "(display (list->string (list #\\h #\\i)))"),
        "hi\n"
    );
    assert_eq!(
        run_demo("b10-e8-15.ml", "(display (string->list \"ab\"))"),
        "(a b)\n"
    );
    assert_eq!(
        run_demo(
            "b10-e8-16.ml",
            &format!("(display (string-length \"{TWO_CHAR_ACCENTED}\"))")
        ),
        "2\n"
    );
    assert_eq!(
        run_demo(
            "b10-e8-17.ml",
            &format!("(display (string-ref \"{TWO_CHAR_ACCENTED}\" 1))")
        ),
        "é\n"
    );
}
