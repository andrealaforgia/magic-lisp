//! Step definitions for features/B4-iteration-and-numeric-semantics.feature.

use magiclisp::exitcode::SOURCE_ERROR;

use super::registry::Registry;
use super::world::{eval_ok, first_quoted, run, run_pending, stderr_of, write_source};

fn eval_quoted_and_store(w: &mut super::world::World, text: &str, _doc: Option<&str>) {
    let code = first_quoted(text).expect("step should embed a quoted expression");
    w.notes.push(eval_ok("b4-quoted.ml", &code));
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        .step(
            "\"(display (do ((i 0 (+ i 1)) (s 0 (+ s i))) ((= i 5) s)))\" is evaluated",
            eval_quoted_and_store,
        )
        .step(
            "\"(display (+ 9223372036854775807 1))\" is evaluated",
            eval_quoted_and_store,
        )
        .step("each is evaluated", |w, _text, _| run_pending(w, "b4-each"))
        .step("each is run", |w, _text, _| run_pending(w, "b4-run"))
        // --- E1 ---
        .step(
            "a do-style loop with variables i and s, i stepping by 1 and s accumulating i each pass, stopping when i reaches 5",
            |_w, _text, _| {},
        )
        .step("it displays \"10\"", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "10");
        })
        // --- E2 ---
        .step(
            "literals in plain decimal, exponent, hex, binary, and octal forms",
            |w, _text, _| {
                w.notes = vec![
                    eval_ok("b4-e2-dec.ml", "(display 1.5)"),
                    eval_ok("b4-e2-exp.ml", "(display 1e3)"),
                    eval_ok("b4-e2-decexp.ml", "(display 1.5e-3)"),
                    eval_ok("b4-e2-hex.ml", "(display #x1A)"),
                    eval_ok("b4-e2-bin.ml", "(display #b101)"),
                    eval_ok("b4-e2-oct.ml", "(display #o17)"),
                ];
            },
        )
        .step("each is read and displayed", |_w, _text, _| { /* the Given already ran and stored everything */
        })
        .step(
            "each shows the correct value: a decimal point or exponent yields a float, radix-prefixed digits yield the correct integer",
            |w, _text, _| {
                assert_eq!(w.notes[0], "1.5");
                assert_eq!(w.notes[1], "1000.0");
                assert_eq!(w.notes[2], "0.0015");
                assert_eq!(w.notes[3], "26");
                assert_eq!(w.notes[4], "5");
                assert_eq!(w.notes[5], "15");
            },
        )
        // --- E3 ---
        .step(
            "an integer literal one digit past the maximum representable integer",
            |w, _text, _| {
                w.files.push(write_source(
                    "b4-e3.ml",
                    &format!("(display {}0)", i64::MAX),
                ));
            },
        )
        .step("it is read", |w, _text, _| {
            let file = w.last_file().clone();
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step(
            "it is rejected with a read error and the source-error exit code, not a silent wrap or crash",
            |w, _text, _| {
                assert_eq!(w.last_output().status.code(), Some(SOURCE_ERROR));
                assert!(!stderr_of(w.last_output()).is_empty());
            },
        )
        // --- E4 ---
        .step(
            "a variety of float values: a non-round-trip-trivial decimal, a whole-number float, an ordinary-magnitude float, a very large and a very small-magnitude float, the three IEEE special values, and negative zero versus positive zero",
            |w, _text, _| {
                w.notes = vec![
                    eval_ok("b4-e4-a.ml", "(display 0.1)"),
                    eval_ok("b4-e4-b.ml", "(display 1.0)"),
                    eval_ok("b4-e4-c1.ml", "(display 12345.5)"),
                    eval_ok("b4-e4-c2.ml", "(display 1e20)"),
                    eval_ok("b4-e4-c3.ml", "(display 1e-20)"),
                    eval_ok("b4-e4-d1.ml", "(display (/ 1.0 0.0))"),
                    eval_ok("b4-e4-d2.ml", "(display (/ -1.0 0.0))"),
                    eval_ok("b4-e4-d3.ml", "(display (/ 0.0 0.0))"),
                    eval_ok("b4-e4-e1.ml", "(display -0.0)"),
                    eval_ok("b4-e4-e2.ml", "(display 0.0)"),
                ];
            },
        )
        .step("each is displayed", |_w, _text, _| {})
        .step(
            "each prints per the formatting rules: shortest round-trip decimal text; a trailing \".0\" for whole-number floats; plain decimal within the ordinary range and an alternate form outside it; a recognisable dedicated form for +inf/-inf/nan; negative zero distinct from positive zero",
            |w, _text, _| {
                assert_eq!(w.notes[0], "0.1");
                assert_eq!(w.notes[1], "1.0");
                assert_eq!(w.notes[2], "12345.5");
                assert_eq!(w.notes[3], "1e20");
                assert_eq!(w.notes[4], "1e-20");
                assert_eq!(w.notes[5], "+inf.0");
                assert_eq!(w.notes[6], "-inf.0");
                assert_eq!(w.notes[7], "+nan.0");
                assert_eq!(w.notes[8], "-0.0");
                assert_eq!(w.notes[9], "0.0");
            },
        )
        // --- E5 ---
        .step("the maximum representable integer plus one", |_w, _text, _| {})
        .step(
            "the result wraps to the minimum representable integer, not an error and not a bignum",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), &i64::MIN.to_string());
            },
        )
        // --- E6 ---
        .step(
            "zero-argument and single-argument calls to each of +, -, *, /",
            |w, _text, _| {
                w.pending = vec![
                    "(display (+))".to_string(),
                    "(display (*))".to_string(),
                    "(display (- 5))".to_string(),
                    "(display (/ 4))".to_string(),
                ];
            },
        )
        .step(
            "(+) yields 0, (*) yields 1, (-) and (/) with zero arguments are errors, (- x) negates x, and (/ x) inverts x",
            |w, _text, _| {
                assert_eq!(w.notes[0], "0");
                assert_eq!(w.notes[1], "1");
                assert_eq!(w.notes[2], "-5");
                assert_eq!(w.notes[3], "0.25");
                let minus0 = run(&["eval", write_source("b4-e6-minus0.ml", "(display (-))").to_str().unwrap()]);
                assert!(!minus0.status.success());
                let div0 = run(&["eval", write_source("b4-e6-div0.ml", "(display (/))").to_str().unwrap()]);
                assert!(!div0.status.success());
            },
        )
        // --- E7 ---
        .step(
            "exact whole-number division, inexact whole-number division, a whole number divided by a float, an integer divided by exact zero, and a float divided by zero",
            |w, _text, _| {
                w.pending = vec![
                    "(display (/ 6 3))".to_string(),
                    "(display (/ 7 2))".to_string(),
                    "(display (/ 6 3.0))".to_string(),
                    "(display (/ 6.0 0))".to_string(),
                ];
            },
        )
        .step(
            "exact whole-number division yields an integer, inexact division yields a float, any float operand yields a float even when exact, integer-divided-by-zero is a runtime failure with a distinct exit code, and float-divided-by-zero succeeds per IEEE rules",
            |w, _text, _| {
                assert_eq!(w.notes[0], "2");
                assert_eq!(w.notes[1], "3.5");
                assert_eq!(w.notes[2], "2.0");
                assert_eq!(w.notes[3], "+inf.0");
                let div_zero = run(&["eval", write_source("b4-e7-div0.ml", "(display (/ 6 0))").to_str().unwrap()]);
                assert!(!div_zero.status.success());
            },
        )
        // --- E8 ---
        .step("each of the six DEMO programs from the behaviour spec", |w, _text, _| {
            w.pending = vec![
                "(display (/ 6 3)) (newline)".to_string(),
                "(display (/ 7 2)) (newline)".to_string(),
                "(display (/ 6 3.0)) (newline)".to_string(),
                "(display 1.0) (newline)".to_string(),
                "(display -0.0) (newline)".to_string(),
                "(display (do ((i 0 (+ i 1)) (s 0 (+ s i))) ((= i 5) s))) (newline)".to_string(),
            ];
        })
        .step(
            "each produces exactly its prescribed output followed by a trailing newline, and exits 0",
            |w, _text, _| {
                assert_eq!(w.notes[0], "2\n");
                assert_eq!(w.notes[1], "3.5\n");
                assert_eq!(w.notes[2], "2.0\n");
                assert_eq!(w.notes[3], "1.0\n");
                assert_eq!(w.notes[4], "-0.0\n");
                assert_eq!(w.notes[5], "10\n");
            },
        )
}
