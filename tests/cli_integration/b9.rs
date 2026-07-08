//! B9: pairs and lists (spec 5.1).

use super::helpers::{eval_ok, run, run_demo, stderr_of, write_source};
use magiclisp::exitcode::{RUNTIME_ERROR, SUCCESS};

#[test]
fn b9_e1_pair_mutation_and_cxr_accessors() {
    assert_eq!(
        eval_ok(
            "b9-e1a.ml",
            "(define p (cons 1 2)) (set-car! p 99) (display (car p))"
        ),
        "99"
    );
    assert_eq!(
        eval_ok(
            "b9-e1b.ml",
            "(define p (cons 1 2)) (set-cdr! p 99) (display (cdr p))"
        ),
        "99"
    );
    assert_eq!(
        eval_ok("b9-e1c.ml", "(display (cadr (cons 1 (cons 2 3))))"),
        "2"
    );
    assert_eq!(
        eval_ok("b9-e1d.ml", "(display (caar (cons (cons 1 2) 3)))"),
        "1"
    );
    assert_eq!(
        eval_ok(
            "b9-e1e.ml",
            "(display (caddr (cons 1 (cons 2 (cons 3 4)))))"
        ),
        "3"
    );
}

#[test]
fn b9_e2_list_construction_and_inspection() {
    assert_eq!(
        eval_ok("b9-e2a.ml", "(display (length (quote (a b c))))"),
        "3"
    );
    assert_eq!(
        eval_ok("b9-e2b.ml", "(display (append (list 1 2) (list 3 4)))"),
        "(1 2 3 4)"
    );
    assert_eq!(
        eval_ok("b9-e2c.ml", "(display (reverse (list 1 2 3)))"),
        "(3 2 1)"
    );
    assert_eq!(
        eval_ok("b9-e2d.ml", "(display (list-ref (list 10 20 30) 2))"),
        "30"
    );
    assert_eq!(
        eval_ok("b9-e2e.ml", "(display (list-tail (list 1 2 3) 0))"),
        "(1 2 3)"
    );
    assert_eq!(
        eval_ok("b9-e2f.ml", "(display (last-pair (list 1 2 3)))"),
        "(3)"
    );
}

#[test]
fn b9_e3_member_at_three_strictness_levels() {
    assert_eq!(
        eval_ok("b9-e3a.ml", "(display (member 2 (list 1 2 3)))"),
        "(2 3)"
    );
    assert_eq!(
        eval_ok(
            "b9-e3b.ml",
            "(display (member (list 1 2) (list (list 1 2) 3)))"
        ),
        "((1 2) 3)"
    );
    assert_eq!(
        eval_ok(
            "b9-e3c.ml",
            "(display (memq (list 1 2) (list (list 1 2) 3)))"
        ),
        "#f"
    );
}

#[test]
fn b9_e4_assoc_at_three_strictness_levels() {
    assert_eq!(
        eval_ok(
            "b9-e4a.ml",
            "(display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))"
        ),
        "(2 . b)"
    );
    assert_eq!(
        eval_ok(
            "b9-e4b.ml",
            "(display (assq (list 1 2) (list (cons (list 1 2) (quote a)))))"
        ),
        "#f"
    );
}

#[test]
fn b9_e5_map_for_each_filter() {
    assert_eq!(
        eval_ok(
            "b9-e5a.ml",
            "(display (map (lambda (x) (* x x)) (list 1 2 3)))"
        ),
        "(1 4 9)"
    );
    assert_eq!(
        eval_ok(
            "b9-e5b.ml",
            "(display (map + (list 1 2 3) (list 10 20 30)))"
        ),
        "(11 22 33)"
    );
    assert_eq!(
        eval_ok("b9-e5c.ml", "(display (filter odd? (list 1 2 3 4 5)))"),
        "(1 3 5)"
    );
    assert_eq!(
        eval_ok(
            "b9-e5d.ml",
            "(for-each (lambda (x) (display x)) (list 1 2 3))"
        ),
        "123"
    );
}

#[test]
fn b9_e6_fold_left_fold_right_reduce() {
    assert_eq!(
        eval_ok("b9-e6a.ml", "(display (fold-left + 0 (list 1 2 3 4)))"),
        "10"
    );
    assert_eq!(
        eval_ok(
            "b9-e6b.ml",
            "(display (fold-right cons (quote ()) (list 1 2 3)))"
        ),
        "(1 2 3)"
    );
    assert_eq!(
        eval_ok("b9-e6c.ml", "(display (fold-left - 0 (list 1 2 3)))"),
        "-6"
    );
    assert_eq!(
        eval_ok("b9-e6d.ml", "(display (fold-right - 0 (list 1 2 3)))"),
        "2"
    );
    assert_eq!(
        eval_ok("b9-e6e.ml", "(display (reduce + 99 (quote ())))"),
        "99"
    );
}

#[test]
fn b9_e7_apply_flattens_trailing_list() {
    assert_eq!(
        eval_ok("b9-e7a.ml", "(display (apply + 1 2 (list 3 4)))"),
        "10"
    );
    assert_eq!(
        eval_ok("b9-e7b.ml", "(display (apply + (list 1 2 3)))"),
        "6"
    );
    assert_eq!(eval_ok("b9-e7c.ml", "(display (apply + 1 2 (list)))"), "3");
}

#[test]
fn b9_e8_nested_and_dotted_quoted_list_literals() {
    assert_eq!(
        eval_ok("b9-e8a.ml", "(display (car (cadr (quote (1 (2 3) 4)))))"),
        "2"
    );
    assert_eq!(eval_ok("b9-e8b.ml", "(display (quote (a . b)))"), "(a . b)");
    assert_eq!(
        eval_ok("b9-e8c.ml", "(display (quote (1 2 . 3)))"),
        "(1 2 . 3)"
    );
    assert_eq!(
        eval_ok("b9-e8d.ml", "(display (list? (quote (1 2 . 3))))"),
        "#f"
    );
}

#[test]
fn b9_e9_all_fourteen_demo_expressions_produce_exactly_the_prescribed_output() {
    assert_eq!(
        run_demo("b9-e9-01.ml", "(display (car (quote (1 2 3))))"),
        "1\n"
    );
    assert_eq!(
        run_demo("b9-e9-02.ml", "(display (cadr (quote (1 2 3))))"),
        "2\n"
    );
    assert_eq!(
        run_demo("b9-e9-03.ml", "(display (length (quote (a b c))))"),
        "3\n"
    );
    assert_eq!(
        run_demo("b9-e9-04.ml", "(display (append (list 1 2) (list 3 4)))"),
        "(1 2 3 4)\n"
    );
    assert_eq!(
        run_demo("b9-e9-05.ml", "(display (reverse (list 1 2 3)))"),
        "(3 2 1)\n"
    );
    assert_eq!(
        run_demo(
            "b9-e9-06.ml",
            "(display (map (lambda (x) (* x x)) (list 1 2 3)))"
        ),
        "(1 4 9)\n"
    );
    assert_eq!(
        run_demo(
            "b9-e9-07.ml",
            "(display (map + (list 1 2 3) (list 10 20 30)))"
        ),
        "(11 22 33)\n"
    );
    assert_eq!(
        run_demo("b9-e9-08.ml", "(display (filter odd? (list 1 2 3 4 5)))"),
        "(1 3 5)\n"
    );
    assert_eq!(
        run_demo("b9-e9-09.ml", "(display (fold-left + 0 (list 1 2 3 4)))"),
        "10\n"
    );
    assert_eq!(
        run_demo(
            "b9-e9-10.ml",
            "(display (fold-right cons (quote ()) (list 1 2 3)))"
        ),
        "(1 2 3)\n"
    );
    assert_eq!(
        run_demo("b9-e9-11.ml", "(display (reduce + 0 (list 1 2 3 4)))"),
        "10\n"
    );
    assert_eq!(
        run_demo("b9-e9-12.ml", "(display (apply + 1 2 (list 3 4)))"),
        "10\n"
    );
    assert_eq!(
        run_demo(
            "b9-e9-13.ml",
            "(display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))"
        ),
        "(2 . b)\n"
    );
    assert_eq!(
        run_demo("b9-e9-14.ml", "(display (member 2 (list 1 2 3)))"),
        "(2 3)\n"
    );
}

#[test]
fn a_large_dotted_list_literal_evaluates_cleanly_instead_of_crashing_the_process() {
    // Regression test for warden security review msg #146: a dotted-list
    // literal `(1 2 3 ... N . tail)` is one flat pair of parens (nesting
    // depth 1), so it never hits the reader's own nesting-depth guard, but
    // its element count has no bound -- large enough, this used to abort
    // the whole process (a real crash, not a clean error) with no set-cdr!,
    // no loop, and no runtime construction at all: just this literal.
    let n = 1_000_000;
    let items: String = (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
    let src = format!("(display (car (quote ({items} . 0))))");
    let file = write_source("b9-large-dotted.ml", &src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "expected a clean exit, got: {:?} (stderr: {})",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0");
}

#[test]
fn list_operations_on_a_circular_list_terminate_cleanly_through_the_real_cli() {
    // Acceptance-level counterpart to the unit tests in src/vm.rs proving
    // equal?/list?/last-pair/length/member/display all terminate on a
    // set-cdr!-induced circular structure (warden msgs #143/#144/#146/#147)
    // -- those were previously only verified by calling eval() directly in
    // the same binary as the code under test, never through the actual CLI
    // argument-parsing/process-exit path a real user would hit (qa
    // test-design review, msg #165).
    let src = "(define p (list 1 2 3)) (set-cdr! (last-pair p) p) \
               (display (list? p)) (newline) \
               (display (member 99 p)) (newline) \
               (display p)";
    let file = write_source("b9-circular-list.ml", src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(SUCCESS),
        "expected a clean exit, got: {:?} (stderr: {})",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "#f\n#f\n(1 2 3 ...)"
    );
}

#[test]
fn length_on_a_circular_list_is_a_clean_runtime_error_through_the_real_cli_not_a_hang() {
    let src = "(define p (list 1 2 3)) (set-cdr! (last-pair p) p) (display (length p))";
    let file = write_source("b9-circular-length.ml", src);
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}
