//! B19: reader edge cases and the full conformance-sample pass.

use magiclisp::exitcode::{RUNTIME_ERROR, SOURCE_ERROR, SUCCESS};

use super::helpers::{eval_ok, run, run_with_stdin, stderr_of, stdout_of, write_source};

// --- E1: line comments (already established since B1) -- brief reconfirmation. ---

#[test]
fn e1_a_line_comment_runs_to_end_of_line() {
    let out = eval_ok("b19-e1.ml", "; a leading comment\n(display 1)");
    assert_eq!(out, "1");
}

// --- E2: block comments, including nesting. ---

#[test]
fn e2_demo2_a_single_block_comment_precedes_a_display() {
    let out = eval_ok("b19-e2-demo2.ml", "#| a block comment |# (display 1) (newline)");
    assert_eq!(out, "1\n");
}

#[test]
fn e2_demo3_a_block_comment_containing_a_complete_nested_one_is_treated_as_one_outer_comment() {
    let out = eval_ok(
        "b19-e2-demo3.ml",
        "#| outer #| nested |# still outer |# (display 2) (newline)",
    );
    assert_eq!(out, "2\n");
}

// --- E3: the "skip the next datum" marker. ---

#[test]
fn e3_demo1_a_stray_datum_between_two_numbers_in_a_sum_is_never_evaluated_or_included() {
    let out = eval_ok("b19-e3-demo1.ml", "(display (+ 1 #;99 2)) (newline)");
    assert_eq!(out, "3\n");
}

#[test]
fn e3_the_skipped_datum_may_itself_be_a_compound_structure() {
    // Proves the marker removes one WHOLE datum regardless of how many
    // tokens it spans, not just the next single token.
    let out = eval_ok("b19-e3-compound.ml", "(display (+ 1 #;(a b c) 2)) (newline)");
    assert_eq!(out, "3\n");
}

// --- E4: alternate-radix integers and dotted pairs (reconfirming B4/B9). ---

#[test]
fn e4_demo4_displaying_a_dotted_pair_of_1_and_2() {
    let out = eval_ok("b19-e4-demo4.ml", "(display (quote (1 . 2))) (newline)");
    assert_eq!(out, "(1 . 2)\n");
}

#[test]
fn e4_alternate_base_integer_literals_read_to_the_correct_value() {
    let out = eval_ok(
        "b19-e4-radix.ml",
        "(display #x1A) (newline) (display #b101) (newline) (display #o17) (newline)",
    );
    assert_eq!(out, "26\n5\n15\n");
}

// --- E5: oversized literal is still a read error; overflow still wraps. ---

#[test]
fn e5_an_oversized_integer_literal_is_still_a_read_error() {
    let file = write_source("b19-e5-oversized.ml", "(display 99999999999999999999999999999)");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(!stderr_of(&out).is_empty());
}

#[test]
fn e5_integer_arithmetic_overflow_still_wraps() {
    let out = eval_ok(
        "b19-e5-wrap.ml",
        "(display (+ 9223372036854775807 1))",
    );
    assert_eq!(out, "-9223372036854775808");
}

// --- E6: all ten officially published SPEC.md 9.5 sample programs. ---

#[test]
fn e6_sample_010_factorial_and_top_level_redefinition() {
    let src = "(define (fact n) (error \"should have been redefined\"))\n\
               (define (fact n) (if (< n 2) 1 (* n (fact (- n 1)))))\n\
               (display (fact 10)) (newline)";
    let file = write_source("b19-e6-010.ml", src);
    let out = run(&["eval", file.to_str().unwrap()]);
    // SPEC.md 9.5 '010-factorial' officially specifies: stdout '3628800\n', exit 0.
    assert_eq!(stdout_of(&out), "3628800\n");
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e6_sample_020_tco_self_tail_recursive_loop_to_ten_million() {
    let src = "(define (loop i) (if (= i 10000000) i (loop (+ i 1)))) (display (loop 0)) (newline)";
    // SPEC.md 9.5 '020-tco' officially specifies: '10000000\n'.
    assert_eq!(eval_ok("b19-e6-020.ml", src), "10000000\n");
}

#[test]
fn e6_sample_030_closures_independent_counters() {
    let src = "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n)))\n\
               (define a (counter)) (define b (counter))\n\
               (display (a)) (newline) (display (a)) (newline) (display (b)) (newline)";
    // SPEC.md 9.5 '030-closures' officially specifies: '1\n2\n1\n'.
    assert_eq!(eval_ok("b19-e6-030.ml", src), "1\n2\n1\n");
}

#[test]
fn e6_sample_040_shared_upvalue_between_two_closures() {
    let src = "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v)))))\n\
               (define p (pairf))\n\
               ((cdr p) 10)\n\
               (display ((car p))) (newline)";
    // SPEC.md 9.5 '040-shared-upvalue' officially specifies: '10\n'.
    assert_eq!(eval_ok("b19-e6-040.ml", src), "10\n");
}

#[test]
fn e6_sample_050_macro_swap_via_gensym() {
    let src = "(define-macro (swap! a b) (let ((tmp (gensym))) `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp))))\n\
               (define x 1) (define y 2) (swap! x y) (write (list x y)) (newline)";
    // SPEC.md 9.5 '050-macro' officially specifies: '(2 1)\n'.
    assert_eq!(eval_ok("b19-e6-050.ml", src), "(2 1)\n");
}

#[test]
fn e6_sample_060_quasi_unquote_splicing() {
    let src = "(define mid (quote (2 3 4))) (write `(1 ,@mid 5)) (newline)";
    // SPEC.md 9.5 '060-quasi' officially specifies: '(1 2 3 4 5)\n'.
    assert_eq!(eval_ok("b19-e6-060.ml", src), "(1 2 3 4 5)\n");
}

#[test]
fn e6_sample_070_division_semantics() {
    let src = "(display (/ 6 3)) (newline)\n\
               (display (/ 7 2)) (newline)\n\
               (display (/ 6 3.0)) (newline)\n\
               (display 1.0) (newline)";
    // SPEC.md 9.5 '070-division' officially specifies: '2\n', '3.5\n', '2.0\n', '1.0\n'.
    assert_eq!(eval_ok("b19-e6-070.ml", src), "2\n3.5\n2.0\n1.0\n");
}

#[test]
fn e6_sample_080_error_deliberate_signalling() {
    let file = write_source("b19-e6-080.ml", "(error \"boom\" 42)");
    let out = run(&["eval", file.to_str().unwrap()]);
    // SPEC.md 9.5 '080-error' officially specifies: stdout empty; exit code 70;
    // first stderr line begins 'Error: boom 42'.
    assert_eq!(stdout_of(&out), "");
    assert_eq!(out.status.code(), Some(RUNTIME_ERROR));
    assert_eq!(stderr_of(&out).lines().next().unwrap(), "Error: boom 42");
}

#[test]
fn e6_sample_090_read_returns_data_unevaluated() {
    let src = "(define d (read)) (write d) (newline) (display (+ 1 2)) (newline)";
    let file = write_source("b19-e6-090.ml", src);
    let out = run_with_stdin(&["eval", file.to_str().unwrap()], b"(+ 1 2)\n");
    // SPEC.md 9.5 '090-read' officially specifies: stdout '(+ 1 2)\n3\n'.
    assert_eq!(stdout_of(&out), "(+ 1 2)\n3\n");
    assert_eq!(out.status.code(), Some(SUCCESS));
}

#[test]
fn e6_sample_100_hash_deterministic_insertion_order_keys() {
    let src = "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2)\n\
               (display (hash-count h)) (newline)\n\
               (write (hash-keys h)) (newline)\n\
               (display (hash-ref h (quote c) \"nope\")) (newline)\n\
               (display (hash-has-key? h (quote a))) (newline)\n\
               (hash-remove! h (quote a)) (display (hash-has-key? h (quote a))) (newline)";
    // SPEC.md 9.5 '100-hash' officially specifies (keys shown via write, per
    // B11's own note: identical to display for a list of symbols):
    // '2\n(a b)\nnope\n#t\n#f\n'.
    assert_eq!(
        eval_ok("b19-e6-100.ml", src),
        "2\n(a b)\nnope\n#t\n#f\n"
    );
}

// --- E7 (integration): the four named DEMOs verbatim. ---

#[test]
fn e7_demo1_skip_datum_marker_before_a_stray_value_between_a_sum() {
    let out = eval_ok("b19-e7-demo1.ml", "(display (+ 1 #;99 2)) (newline)");
    assert_eq!(out, "3\n");
}

#[test]
fn e7_demo2_a_single_block_comment_then_displaying_1() {
    let out = eval_ok("b19-e7-demo2.ml", "#| a block comment |# (display 1) (newline)");
    assert_eq!(out, "1\n");
}

#[test]
fn e7_demo3_a_block_comment_containing_a_nested_complete_one_then_displaying_2() {
    let out = eval_ok(
        "b19-e7-demo3.ml",
        "#| outer #| nested |# still outer |# (display 2) (newline)",
    );
    assert_eq!(out, "2\n");
}

#[test]
fn e7_demo4_displaying_a_dotted_pair_of_1_and_2() {
    let out = eval_ok("b19-e7-demo4.ml", "(display (quote (1 . 2))) (newline)");
    assert_eq!(out, "(1 . 2)\n");
}
