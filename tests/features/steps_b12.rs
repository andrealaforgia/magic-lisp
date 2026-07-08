//! Step definitions for features/B12-io-read-write-display.feature.

use super::registry::Registry;
use super::world::{eval_ok, run_with_stdin, stdout_of, write_source};

fn eval_ok_with_stdin(label: &str, src: &str, stdin_data: &[u8]) -> String {
    let file = write_source(label, src);
    let output = run_with_stdin(&["eval", file.to_str().unwrap()], stdin_data);
    assert!(
        output.status.success(),
        "expected {label} to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_of(&output)
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "stdin containing the text \"(+ 1 2)\", stdin containing two data units, and stdin exhausted after one value",
            |_w, _text, _| {},
        )
        .step(
            "read is called and its result is written or checked with eof-object?",
            |w, _text, _| {
                w.notes.push(eval_ok_with_stdin(
                    "b12-e1a.ml",
                    "(define d (read)) (write d) (newline) (display (+ 1 2))",
                    b"(+ 1 2)",
                ));
                w.notes.push(eval_ok_with_stdin(
                    "b12-e1b.ml",
                    "(display (read)) (display (read))",
                    b"1 2",
                ));
                w.notes.push(eval_ok_with_stdin(
                    "b12-e1c.ml",
                    "(display (eof-object? (read))) (display (eof-object? (read)))",
                    b"1",
                ));
            },
        )
        .step(
            "it returns the literal unevaluated data (confirmed distinct from separately computing the same expression), advances correctly across repeated calls, and eof-object? is #f for an ordinary value and #t for the end-of-input marker",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(+ 1 2)\n3");
                assert_eq!(w.notes[1], "12");
                assert_eq!(w.notes[2], "#f#t");
            },
        )
        // --- E2 ---
        .step(
            "stdin with two lines \"hello\" and \"world\", and a single line \"hello\\n\"",
            |_w, _text, _| {},
        )
        .step(
            "read-line is called repeatedly, and the returned string's length is measured",
            |w, _text, _| {
                w.notes.push(eval_ok_with_stdin(
                    "b12-e2a.ml",
                    "(display (read-line)) (newline) (display (read-line)) (newline) \
                     (display (eof-object? (read-line))) (newline)",
                    b"hello\nworld\n",
                ));
                w.notes.push(eval_ok_with_stdin(
                    "b12-e2b.ml",
                    "(display (string-length (read-line)))",
                    b"hello\n",
                ));
            },
        )
        .step(
            "it returns each line without its terminator, the third call at end-of-input satisfies eof-object?, and the returned string's length proves the terminator was actually removed, not just invisible",
            |w, _text, _| {
                assert_eq!(w.notes[0], "hello\nworld\n#t\n");
                assert_eq!(w.notes[1], "5");
            },
        )
        // --- E3 ---
        .step(
            "a string with an embedded newline and a character value",
            |_w, _text, _| {},
        )
        .step("each is displayed", |w, _text, _| {
            let out = eval_ok(
                "b12-e3.ml",
                "(display \"a\\nb\") (newline) (display #\\a)",
            );
            w.notes.push(out);
        })
        .step(
            "the embedded newline produces a real line break (not literal backslash-n), and the character shows as itself, bare",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "a\nb\na");
            },
        )
        // --- E4 ---
        .step(
            "the same embedded-newline string, a symbol, a non-printing character, a number, and a list",
            |_w, _text, _| {},
        )
        .step(
            "each is written and, for the number/list/character, also displayed",
            |w, _text, _| {
                let out = eval_ok(
                    "b12-e4.ml",
                    "(write \"a\\nb\") (newline) (write (quote sym)) (newline) \
                     (write #\\space) (newline) (display #\\space) (newline) \
                     (write 42) (newline) (display 42) (newline) \
                     (write (list 1 2 3)) (newline) (display (list 1 2 3))",
                );
                w.notes.push(out);
            },
        )
        .step(
            "the embedded newline prints as literal backslash-n under write, the character prints in its named form under write versus bare under display, and the number and list print byte-for-byte identically under both styles",
            |w, _text, _| {
                assert_eq!(
                    w.notes.last().unwrap(),
                    "\"a\\nb\"\nsym\n#\\space\n \n42\n42\n(1 2 3)\n(1 2 3)"
                );
            },
        )
        // --- E5 ---
        .step(
            "a program that displays, reads a line, and displays again, ending on output with no trailing newline",
            |_w, _text, _| {},
        )
        .step("it runs to completion", |w, _text, _| {
            let out = eval_ok_with_stdin(
                "b12-e5.ml",
                "(display \"start\") (newline) (display (read-line)) (newline) (display \"end\")",
                b"middle\n",
            );
            w.notes.push(out);
        })
        .step(
            "every piece of output appears in stdout in the correct order, including the final unflushed-looking output before the process exits",
            |w, _text, _| {
                assert_eq!(w.notes.last().unwrap(), "start\nmiddle\nend");
            },
        )
        // --- E6 ---
        .step(
            "each of the three DEMO scenarios from the behaviour spec, with their specified stdin",
            |_w, _text, _| {},
        )
        .step("each is run", |w, _text, _| {
            w.notes.push(eval_ok_with_stdin(
                "b12-e6-case1.ml",
                "(define d (read)) (write d) (newline) (display (+ 1 2)) (newline)",
                b"(+ 1 2)\n",
            ));
            w.notes.push(eval_ok_with_stdin(
                "b12-e6-case2.ml",
                "(display (read-line)) (newline) (display (read-line)) (newline) \
                 (display (eof-object? (read-line))) (newline)",
                b"hello\nworld\n",
            ));
            w.notes.push(eval_ok(
                "b12-e6-case3.ml",
                "(write \"a\\nb\") (newline) (display \"a\\nb\") (newline) \
                 (write (quote sym)) (newline)",
            ));
        })
        .step(
            "each produces exactly its prescribed output and exits 0",
            |w, _text, _| {
                assert_eq!(w.notes[0], "(+ 1 2)\n3\n");
                assert_eq!(w.notes[1], "hello\nworld\n#t\n");
                assert_eq!(w.notes[2], "\"a\\nb\"\na\nb\nsym\n");
            },
        )
}
