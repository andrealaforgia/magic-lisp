//! B12: input reading and the write/display output distinction (spec 3.2, 4.8).

use std::io::{Read as _, Write as _};
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
fn read_completes_promptly_for_a_datum_that_lands_exactly_on_a_full_relay_read_boundary() {
    // Regression test for warden security review msg #231/#232: the
    // short-vs-full-read signal is a much better proxy than chunk count,
    // but it's still a guess about the transport, not a fact about the
    // grammar -- when a complete datum's last byte happens to land
    // exactly at the end of a FULL relay read (the relay's fixed 8192-
    // byte buffer, see `run_with_stdin` in src/vm.rs), the old logic
    // concluded "more is probably still coming" and never retried,
    // stalling indefinitely even though nothing more was ever coming.
    // Sends a complete, valid 8192-byte string-literal datum (`"` + 8190
    // `a`s + `"`) in a single write, landing as exactly one full 8192-byte
    // relay read, then holds the stream open with no further data.
    let file = write_source(
        "b12-read-full-boundary.ml",
        "(display (string-length (read)))",
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
    let datum = format!("\"{}\"", "a".repeat(8190));
    assert_eq!(
        datum.len(),
        8192,
        "datum must land exactly on the relay's read buffer size"
    );
    stdin.write_all(datum.as_bytes()).unwrap();

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
                "process did not complete within 5s while stdin stayed open \
                 after a datum landing exactly on the 8192-byte relay read \
                 boundary -- the full-read-boundary stall regression"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
}

#[test]
fn read_completes_promptly_for_a_complete_datum_containing_a_block_comment() {
    // Regression test for warden security review msg #366: B19 added
    // `#| ... |#` block comments to the reader, but `native_read`'s own
    // `DatumBoundaryScan` (a deliberately simplified mirror of that same
    // grammar, used to decide when a real parse attempt is worth trying)
    // went stale -- it had no notion of block comments at all, so an
    // unmatched paren inside one desynced its bracket-depth count and the
    // datum never registered as complete on a stream that stayed open.
    let file = write_source("b12-read-block-comment.ml", "(display (read))");
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"(display #| ( |# 1)").unwrap();

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
                 after a complete datum containing a block comment -- the \
                 stale-boundary-tracker stall regression"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
}

#[test]
fn read_completes_promptly_for_a_complete_datum_containing_a_skip_next_datum_marker() {
    // Sibling regression test for the same root cause (warden security
    // review msg #366): `#;` (skip the next datum) was misrouted into the
    // tracker's unconditional `;`-line-comment handling, which then
    // swallowed the rest of the stream -- including the real, load-bearing
    // closing paren -- since no newline ever arrives on a held-open stream.
    let file = write_source("b12-read-skip-datum.ml", "(display (read))");
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"(display 1 #;(x) 2)").unwrap();

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
                 after a complete datum containing a #; skip-next-datum \
                 marker -- the stale-boundary-tracker stall regression"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
}

#[test]
fn read_does_not_return_a_number_split_by_a_gap_until_all_its_digits_have_arrived() {
    // Regression test for warden security review msg #244: a bare number
    // (or symbol) has no delimiter of its own -- its true end can only be
    // known by seeing what comes after it, or by the stream genuinely
    // ending. Sending only its first digits, with the stream still open
    // and no delimiter yet, must NOT be treated as "done"; the process
    // must still be running when the rest of the number arrives later.
    // Confirmed pre-fix: `(display (read))` fed "123" then, after a delay,
    // "456" returned "123" -- the process had already exited on the first
    // piece alone, instead of waiting for a real delimiter or true EOF.
    let file = write_source("b12-read-number-split-by-a-gap.ml", "(display (read))");
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"123").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        child
            .try_wait()
            .expect("polling child status should not fail")
            .is_none(),
        "process exited on the first piece of a number alone, before the \
         rest of its digits ever arrived -- the number-truncation regression"
    );

    stdin.write_all(b"456").unwrap();
    drop(stdin);
    let output = child
        .wait_with_output()
        .expect("process should exit after the number completes");
    assert!(
        output.status.success(),
        "expected a clean exit, got: {:?}",
        output.status
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "123456");
}

#[test]
fn read_does_not_return_a_quote_wrapped_number_split_by_a_gap_until_all_its_digits_have_arrived() {
    // Regression test for warden security review msg #244: quote-shorthand
    // (`'1`) has no closing delimiter of its own -- its completeness is
    // entirely inherited from whatever it wraps. Sending only the wrapped
    // number's first digit, with the stream still open and no delimiter
    // yet, must NOT be treated as "done"; the process must still be
    // running when the rest of the number arrives later. Confirmed
    // pre-fix: sending "'1" then, after a delay, "23" returned `(quote 1)`
    // instead of `(quote 123)` -- the process had already exited on the
    // first digit alone.
    let file = write_source("b12-read-quoted-number-split-by-a-gap.ml", "(write (read))");
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"'1").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        child
            .try_wait()
            .expect("polling child status should not fail")
            .is_none(),
        "process exited on the first digit of a quote-wrapped number alone, \
         before the rest of its digits ever arrived -- the quote-wrapped \
         number-truncation regression"
    );

    stdin.write_all(b"23").unwrap();
    drop(stdin);
    let output = child
        .wait_with_output()
        .expect("process should exit after the number completes");
    assert!(
        output.status.success(),
        "expected a clean exit, got: {:?}",
        output.status
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "(quote 123)");
}

#[test]
fn read_does_not_return_a_boolean_split_by_a_gap_until_it_is_complete() {
    // Regression test for warden security review msg #249: `#t`/`#f` go
    // through the exact same delimiter-bounded tokenizer as a bare
    // number/symbol, so they're exactly as ambiguous when the stream
    // hasn't produced a real delimiter yet. Sends `#t`, holds the stream
    // open with no delimiter, then sends `ally` -- together spelling the
    // symbol `#tally`, not the boolean `#t`. Confirmed pre-fix: the
    // process had already exited returning `#t` before `ally` ever
    // arrived.
    let file = write_source("b12-read-boolean-split-by-a-gap.ml", "(display (read))");
    let mut child = Command::new(env!("CARGO_BIN_EXE_magiclisp"))
        .arg("eval")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should spawn");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"#t").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        child
            .try_wait()
            .expect("polling child status should not fail")
            .is_none(),
        "process exited on '#t' alone, before the rest of the symbol '#tally' \
         ever arrived -- the boolean-truncation regression"
    );

    stdin.write_all(b"ally").unwrap();
    drop(stdin);
    let output = child
        .wait_with_output()
        .expect("process should exit once the symbol completes");
    assert!(
        output.status.success(),
        "expected a clean exit, got: {:?}",
        output.status
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "#tally");
}

#[test]
fn read_does_not_return_a_named_character_literal_split_by_a_gap_until_it_is_complete() {
    // Regression test for warden security review msg #249: a named
    // character literal like `#\space` is read by its own function
    // (`read_character`), separate from the ordinary tokenizer, but has
    // the identical ambiguity once its name starts with a letter: it
    // keeps consuming until a real delimiter or true EOF. Sends `#\s`,
    // holds the stream open with no delimiter, then sends `pace` --
    // together spelling the named literal `#\space`, not the single
    // character `#\s`. Confirmed pre-fix: the process had already exited
    // returning the character `s` before `pace` ever arrived.
    let file = write_source(
        "b12-read-named-char-literal-split-by-a-gap.ml",
        "(display (read))",
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
    stdin.write_all(b"#\\s").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        child
            .try_wait()
            .expect("polling child status should not fail")
            .is_none(),
        "process exited on '#\\s' alone, before the rest of the named literal \
         '#\\space' ever arrived -- the character-literal-truncation regression"
    );

    stdin.write_all(b"pace").unwrap();
    drop(stdin);
    let output = child
        .wait_with_output()
        .expect("process should exit once the named literal completes");
    assert!(
        output.status.success(),
        "expected a clean exit, got: {:?}",
        output.status
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), " ");
}

#[test]
fn a_second_read_after_a_first_bare_atom_completed_only_at_true_eof_sees_eof_not_the_first_value() {
    // Regression test for qa test-design review msg #250: the fix for
    // warden msg #244 also fixed a second, distinct bug in the same area
    // -- `native_read`'s final true-EOF branch never drained the buffer
    // on a successful atom parse (unreachable for atoms before that fix,
    // since a successful parse always drained during the incremental
    // loop). Left undrained, a second `read` call would see the same
    // already-returned text still sitting in the buffer and return it
    // again. `read_does_not_return_a_number_split_by_a_gap...` above
    // covers the truncation half of that commit; this covers the drain
    // half specifically, decoupled from `b12_e1c`'s incidental coverage
    // of the same shape via a different assertion (`eof-object?`).
    assert_eq!(
        eval_ok_with_stdin(
            "b12-second-read-after-eof-completed-atom.ml",
            "(display (read)) (write (read))",
            b"42"
        ),
        "42#<eof>"
    );
}

#[test]
fn read_completes_promptly_for_a_closed_two_element_list_that_is_not_a_quote_wrapper() {
    // Regression test for warden security review msg #244's fix: only a
    // quote/quasiquote/unquote/unquote-splicing wrapper around a bare atom
    // inherits that atom's ambiguous, possibly-still-growing completeness.
    // An ordinary two-element list like `(foo 1)` is closed by its own
    // explicit, unambiguous `)` -- it must complete promptly the moment
    // that paren arrives, not be mistaken for a quote wrapper and made to
    // wait for more input that was never coming. Sends the complete,
    // already-closed list in one write, then holds the stream open with
    // no further data.
    let file = write_source(
        "b12-read-closed-list-not-a-quote-wrapper.ml",
        "(display (read))",
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
    stdin.write_all(b"(foo 1)").unwrap();

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
                "process did not complete within 5s while stdin stayed open \
                 after a complete, already-closed two-element list -- it was \
                 mistaken for a quote wrapper and left waiting for more input"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
}

#[test]
fn read_completes_promptly_when_a_complete_datum_is_immediately_followed_by_an_incomplete_second_construct()
 {
    // Regression test for warden security review msg #237: checking for a
    // possible boundary only once, after a whole newly-arrived chunk was
    // fed, misses a complete datum that's immediately followed, within
    // that SAME chunk, by the start of an unrelated incomplete second
    // construct -- bracket depth has already left zero again by the time
    // anyone looks, masking a datum that was genuinely complete and ready
    // to return. Sends `1(display` in a single write (a complete `1`
    // immediately followed by the unclosed start of a second, unrelated
    // list) then holds the stream open with no more data -- `(read)`
    // should still return `1` promptly, never waiting on the second,
    // irrelevant construct to ever complete.
    let file = write_source(
        "b12-read-masked-by-incomplete-second-construct.ml",
        "(display (read))",
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
    stdin.write_all(b"1(display").unwrap();

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
                "process did not complete within 5s while stdin stayed open \
                 after a complete datum immediately followed by an incomplete \
                 second construct -- the masked-boundary stall regression"
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    assert!(status.success(), "expected a clean exit, got: {status:?}");
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    assert_eq!(stdout, "1");
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
    let file = write_source("b12-read-many-small-chunks.ml", "(display (length (read)))");
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
