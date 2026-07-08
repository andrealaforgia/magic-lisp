//! B12: input reading and the write/display output distinction (spec 3.2, 4.8).

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::helpers::{eval_ok, run_with_stdin, stdout_of, write_source};
use magiclisp::exitcode::SUCCESS;

fn eval_ok_with_stdin(label: &str, src: &str, stdin_data: &[u8]) -> String {
    let file = write_source(label, src);
    let output = run_with_stdin(&["eval", file.to_str().unwrap()], stdin_data);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_of(&output)
}

#[test]
fn b12_e1_read_returns_data_unevaluated_advances_and_eof_shown_both_ways() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e1a.ml",
            "(define d (read)) (write d) (newline) (display (+ 1 2))",
            b"(+ 1 2)"
        ),
        "(+ 1 2)\n3"
    );
    assert_eq!(
        eval_ok_with_stdin("b12-e1b.ml", "(display (read)) (display (read))", b"1 2"),
        "12"
    );
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e1c.ml",
            "(display (eof-object? (read))) (display (eof-object? (read)))",
            b"1"
        ),
        "#f#t"
    );
}

#[test]
fn b12_e2_read_line_reads_successive_lines_then_eof_with_terminator_stripped() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e2a.ml",
            "(display (read-line)) (newline) (display (read-line)) (newline) \
             (display (eof-object? (read-line))) (newline)",
            b"hello\nworld\n"
        ),
        "hello\nworld\n#t\n"
    );
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e2b.ml",
            "(display (string-length (read-line)))",
            b"hello\n"
        ),
        "5"
    );
}

#[test]
fn b12_e3_display_prints_raw_strings_and_bare_characters() {
    assert_eq!(eval_ok("b12-e3a.ml", "(display \"a\\nb\")"), "a\nb");
    assert_eq!(eval_ok("b12-e3b.ml", "(display #\\a)"), "a");
}

#[test]
fn b12_e4_write_prints_escaped_strings_named_characters_and_matches_display_for_ordinary_values() {
    assert_eq!(eval_ok("b12-e4a.ml", "(write \"a\\nb\")"), "\"a\\nb\"");
    assert_eq!(eval_ok("b12-e4b.ml", "(write (quote sym))"), "sym");
    assert_eq!(eval_ok("b12-e4c.ml", "(write #\\space)"), "#\\space");
    assert_eq!(eval_ok("b12-e4d.ml", "(display #\\space)"), " ");
    assert_eq!(
        eval_ok("b12-e4e.ml", "(write 42)"),
        eval_ok("b12-e4f.ml", "(display 42)")
    );
    assert_eq!(
        eval_ok("b12-e4g.ml", "(write (list 1 2 3))"),
        eval_ok("b12-e4h.ml", "(display (list 1 2 3))")
    );
}

#[test]
fn b12_e5_all_output_present_and_in_order_with_interleaved_reads_and_writes() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e5.ml",
            "(display \"start\") (newline) (display (read-line)) (newline) (display \"end\")",
            b"middle\n"
        ),
        "start\nmiddle\nend"
    );
}

#[test]
fn b12_e6_all_three_demo_scenarios_produce_exactly_the_prescribed_output() {
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e6a.ml",
            "(define d (read)) (write d) (newline) (display (+ 1 2)) (newline)",
            b"(+ 1 2)\n"
        ),
        "(+ 1 2)\n3\n"
    );
    assert_eq!(
        eval_ok_with_stdin(
            "b12-e6b.ml",
            "(display (read-line)) (newline) (display (read-line)) (newline) \
             (display (eof-object? (read-line))) (newline)",
            b"hello\nworld\n"
        ),
        "hello\nworld\n#t\n"
    );
    assert_eq!(
        eval_ok(
            "b12-e6c.ml",
            "(write \"a\\nb\") (newline) (display \"a\\nb\") (newline) \
             (write (quote sym)) (newline)"
        ),
        "\"a\\nb\"\na\nb\nsym\n"
    );
}

#[test]
fn read_completes_promptly_even_when_stdin_stays_open_after_a_complete_datum() {
    // Regression test for warden security review msg #218: native_read's
    // O(n^2) fix (msg #208) could leave a datum that had ALREADY fully
    // arrived unread, indefinitely, if the stream stayed open afterward --
    // exactly how an interactive terminal or a persistent request/response
    // pipe behaves. Confirmed pre-fix via a live FIFO test with a 3+
    // second stall. This drives the same shape through a real spawned
    // process: writes a complete, valid 20-line datum, then deliberately
    // keeps stdin OPEN well past when the read should have completed, and
    // asserts the process finishes anyway -- proving completion doesn't
    // depend on the stream closing or on more (irrelevant) data arriving.
    let file = write_source("b12-read-stream-stays-open.ml", "(display (length (read)))");
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    let datum: String = (0..20).map(|i| format!("{i}\n")).collect();
    stdin.write_all(format!("(\n{datum})").as_bytes()).unwrap();

    // Poll for up to 2s -- comfortably more than the read should ever
    // need, comfortably less than the 3+ second stall the pre-fix bug
    // exhibited -- while STILL holding stdin open the whole time.
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .expect("polling child status should not fail")
        {
            break status;
        }
        if Instant::now() >= deadline {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "process did not complete within 2s while stdin stayed open \
                 -- likely the interactive-stream stall regression"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
}

#[test]
fn read_completes_promptly_for_a_datum_delivered_in_far_more_than_sixty_four_small_chunks() {
    // Regression test for warden security review msg #226 and qa msg #227:
    // the previous fix for the test above only retried eagerly for the
    // first 64 chunks of a read, then fell back to size-based backoff --
    // which merely moved the same interactive stall to any datum spread
    // across MORE than 64 chunks, independently reproduced by both
    // reviewers (152 and 100 one-byte-at-a-time writes respectively).
    // Unlike that test's single `write_all` call (which the OS typically
    // delivers to the reader as one chunk, so it could never have caught
    // this), this one writes one byte at a time with a small delay after
    // each -- forcing the child's underlying reads to actually happen in
    // many separate small pieces -- for a datum comfortably past the old
    // 64-chunk threshold, then keeps stdin open afterward and asserts
    // completion anyway.
    let file = write_source(
        "b12-read-many-small-chunks.ml",
        "(display (length (read)))",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    let datum: String = (0..150).map(|i| format!(" {i}")).collect();
    let bytes = format!("({datum})");
    assert!(
        bytes.len() > 150,
        "datum must span well over 64 single-byte chunks"
    );
    for byte in bytes.as_bytes() {
        stdin
            .write_all(std::slice::from_ref(byte))
            .and_then(|()| stdin.flush())
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .expect("polling child status should not fail")
        {
            break status;
        }
        if Instant::now() >= deadline {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "process did not complete within 5s of its datum finishing \
                 while stdin stayed open, chunked into far more than 64 \
                 pieces -- the interactive-stream stall regression"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
}
