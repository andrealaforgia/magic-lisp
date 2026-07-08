//! Step definitions for features/B7-numeric-library.feature.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::registry::Registry;
use super::world::{eval_ok, run, run_pending, stderr_of, write_source};

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "the same negative-dividend and negative-divisor inputs applied to remainder and modulo",
            |_w, _text, _| {},
        )
        .step("quotient, remainder, and modulo are evaluated", |w, _text, _| {
            let out = eval_ok(
                "b7-e1.ml",
                "(display (quotient 7 2)) (newline) \
                 (display (remainder 7 2)) (newline) \
                 (display (modulo -7 2)) (newline) \
                 (display (remainder -7 2)) (newline) \
                 (display (remainder 7 -2)) (newline) \
                 (display (modulo 7 -2)) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "quotient and remainder truncate toward zero while modulo floors, giving a different sign on negative inputs",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "3\n1\n1\n-1\n1\n-1\n");
            },
        )
        .step(
            "dividing by zero with any of the three is a clean runtime error",
            |_w, _text, _| {
                for op in ["quotient", "remainder", "modulo"] {
                    let file = write_source(
                        &format!("b7-e1-{op}-zero.ml"),
                        &format!("(display ({op} 7 0))"),
                    );
                    let output = run(&["eval", file.to_str().unwrap()]);
                    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                    assert!(!stderr_of(&output).is_empty());
                }
            },
        )
        // --- shared: E2/E3/E4/E5 all use this exact When wording, with a
        // different Given queueing up what it should run.
        .step("each is evaluated", |w, _text, _| {
            run_pending(w, "b7-each");
        })
        // --- E2 ---
        .step(
            "abs of a negative number, min/max over 2 and over 4+ arguments, and each predicate on a satisfying and a non-satisfying input",
            |w, _text, _| {
                w.pending = vec![
                    "(display (abs -5)) (newline) \
                     (display (max 1 5 3)) (newline) \
                     (display (min 3 1)) (newline) \
                     (display (min 5 1 3 2)) (newline) \
                     (display (zero? 0)) (newline) \
                     (display (zero? 1)) (newline) \
                     (display (positive? 1)) (newline) \
                     (display (positive? -1)) (newline) \
                     (display (positive? 0)) (newline) \
                     (display (negative? -1)) (newline) \
                     (display (negative? 1)) (newline) \
                     (display (negative? 0)) (newline) \
                     (display (even? 10)) (newline) \
                     (display (even? 3)) (newline) \
                     (display (odd? 3)) (newline) \
                     (display (odd? 10)) (newline)"
                        .to_string(),
                ];
            },
        )
        .step(
            "abs/min/max compute correctly and every predicate returns #t on its satisfying input and #f otherwise",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "5\n5\n1\n1\n#t\n#f\n#t\n#f\n#f\n#t\n#f\n#f\n#t\n#f\n#t\n#f\n"
                );
            },
        )
        // --- E3 ---
        .step(
            "a positive and a negative non-integer float, and a whole-number integer, applied to floor/ceiling/round/truncate",
            |w, _text, _| {
                w.pending = vec![
                    "(display (floor 2.7)) (newline) \
                     (display (round 2.5)) (newline) \
                     (display (round 3.5)) (newline) \
                     (display (floor -2.7)) (newline) \
                     (display (ceiling -2.7)) (newline) \
                     (display (truncate -2.7)) (newline) \
                     (display (round -2.7)) (newline) \
                     (display (floor 5)) (newline) \
                     (display (ceiling 5)) (newline) \
                     (display (round 5)) (newline) \
                     (display (truncate 5)) (newline)"
                        .to_string(),
                ];
            },
        )
        .step(
            "floor/ceiling/truncate/round on the negative float show all four can differ, round-to-even holds at exact halfway points, and the integer input comes back unchanged (not promoted to float)",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "2.0\n2.0\n4.0\n-3.0\n-2.0\n-2.0\n-3.0\n5\n5\n5\n5\n"
                );
            },
        )
        // --- E4 ---
        .step(
            "a whole number raised to a non-negative whole-number power, a perfect-square whole number's square root, and known-value spot-checks of exp/log/sin/cos/tan/atan",
            |w, _text, _| {
                w.pending = vec![
                    "(display (expt 2 10)) (newline) \
                     (display (sqrt 4)) (newline) \
                     (display (exp 0)) (newline) \
                     (display (log 1)) (newline) \
                     (display (sin 0)) (newline) \
                     (display (cos 0)) (newline) \
                     (display (tan 0)) (newline) \
                     (display (atan 0)) (newline)"
                        .to_string(),
                ];
            },
        )
        .step(
            "the integer power is an exact whole number, the perfect-square square root is still a float, and each transcendental function produces the mathematically correct float",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "1024\n2.0\n1.0\n0.0\n0.0\n1.0\n0.0\n0.0\n"
                );
            },
        )
        // --- E5 ---
        .step(
            "values for number?/integer?/float?, an integer converted to float and a float converted to exact, and +inf/-inf/nan converted to exact",
            |w, _text, _| {
                w.pending = vec![
                    "(display (number? 5)) (newline) \
                     (display (number? \"x\")) (newline) \
                     (display (integer? 5)) (newline) \
                     (display (integer? 5.0)) (newline) \
                     (display (float? 5.0)) (newline) \
                     (display (float? 5)) (newline) \
                     (display (exact->inexact 5)) (newline) \
                     (display (inexact->exact 5.7)) (newline)"
                        .to_string(),
                ];
            },
        )
        .step(
            "each predicate is type-based and correct both ways, the conversions produce the correct value (truncating toward zero for float-to-exact), and converting a non-finite float to exact is a clean runtime error naming the specific non-finite value",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "#t\n#f\n#t\n#f\n#t\n#f\n5.0\n5\n"
                );
                for (expr, label) in [
                    ("(/ 1.0 0.0)", "+inf.0"),
                    ("(/ -1.0 0.0)", "-inf.0"),
                    ("(/ 0.0 0.0)", "+nan.0"),
                ] {
                    let file = write_source(
                        &format!("b7-e5-nonfinite-{}.ml", label.trim_start_matches(['+', '-'])),
                        &format!("(display (inexact->exact {expr}))"),
                    );
                    let output = run(&["eval", file.to_str().unwrap()]);
                    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
                    assert!(
                        stderr_of(&output).contains(label),
                        "expected the error to name {label}, got: {}",
                        stderr_of(&output)
                    );
                }
            },
        )
        // --- E6 ---
        .step(
            "a numeric string, a non-numeric string, and a round trip through number->string and back",
            |_w, _text, _| {},
        )
        .step("each is parsed or converted", |w, _text, _| {
            let out = eval_ok(
                "b7-e6.ml",
                "(display (string->number \"3.5\")) (newline) \
                 (display (string->number \"xyz\")) (newline) \
                 (display (string->number (number->string 42))) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "a valid numeric string parses to the correct number, an invalid one yields #f (not an error), and the round trip reproduces the original value",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "3.5\n#f\n42\n");
            },
        )
        // --- E7 ---
        .step(
            "a user-defined higher-order function that calls its procedure argument, tried with a representative operation from each category (division family, predicate, rounding, conversion, abs)",
            |_w, _text, _| {},
        )
        .step("each is passed as a plain argument and invoked indirectly", |w, _text, _| {
            let out = eval_ok(
                "b7-e7.ml",
                "(define (apply-to-5 f) (f 5)) \
                 (display (apply-to-5 abs)) (newline) \
                 (display (apply-to-5 even?)) (newline) \
                 (display (apply-to-5 floor)) (newline) \
                 (display (apply-to-5 exact->inexact)) (newline) \
                 (define (apply-to-2-and-3 f) (f 2 3)) \
                 (display (apply-to-2-and-3 quotient)) (newline)",
            );
            w.notes.push(out);
        })
        .step("it produces exactly what calling it directly would", |w, _text, _| {
            assert_eq!(w.notes.last().unwrap(), "5\n#f\n5\n5.0\n0\n");
        })
        // --- E8 ---
        .step(
            "all thirteen DEMO expressions from the behaviour spec run together in one program",
            |_w, _text, _| {},
        )
        .step("it is run", |w, _text, _| {
            let out = eval_ok(
                "b7-e8.ml",
                "(display (quotient 7 2)) (newline) \
                 (display (remainder 7 2)) (newline) \
                 (display (modulo -7 2)) (newline) \
                 (display (abs -5)) (newline) \
                 (display (max 1 5 3)) (newline) \
                 (display (even? 10)) (newline) \
                 (display (expt 2 10)) (newline) \
                 (display (sqrt 4)) (newline) \
                 (display (floor 2.7)) (newline) \
                 (display (round 2.5)) (newline) \
                 (display (round 3.5)) (newline) \
                 (display (string->number \"3.5\")) (newline) \
                 (display (string->number \"xyz\")) (newline)",
            );
            w.notes.push(out);
        })
        .step(
            "each line of output matches its prescribed value exactly, and the process exits 0",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "3\n1\n1\n5\n5\n#t\n1024\n2.0\n2.0\n2.0\n4.0\n3.5\n#f\n"
                );
            },
        )
}
