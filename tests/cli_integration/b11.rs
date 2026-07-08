//! B11: vectors and hash tables (spec 4.5, 4.6).

use super::helpers::{eval_ok, run, run_demo, stderr_of, write_source};
use magiclisp::exitcode::RUNTIME_ERROR;

#[test]
fn b11_e1_vector_construction_indexing_and_bounds_errors() {
    assert_eq!(
        eval_ok("b11-e1a.ml", "(display (vector-ref (vector 1 2 3) 1))"),
        "2"
    );
    assert_eq!(
        eval_ok(
            "b11-e1b.ml",
            "(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector-ref v 1))"
        ),
        "99"
    );
    assert_eq!(
        eval_ok("b11-e1c.ml", "(display (vector-length (vector 1 2 3)))"),
        "3"
    );
    assert_eq!(
        eval_ok("b11-e1d.ml", "(display (make-vector 3 7))"),
        "#(7 7 7)"
    );

    let read_oob = write_source(
        "b11-e1-read-oob.ml",
        "(display (vector-ref (vector 1 2 3) 3))",
    );
    let output = run(&["eval", read_oob.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());

    let write_oob = write_source("b11-e1-write-oob.ml", "(vector-set! (vector 1 2 3) 3 99)");
    let output = run(&["eval", write_oob.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b11_e2_vector_list_conversion_and_whole_vector_fill() {
    assert_eq!(
        eval_ok(
            "b11-e2a.ml",
            "(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector->list v))"
        ),
        "(1 99 3)"
    );
    assert_eq!(
        eval_ok("b11-e2b.ml", "(display (list->vector (list 1 2)))"),
        "#(1 2)"
    );
    assert_eq!(
        eval_ok(
            "b11-e2c.ml",
            "(define v (vector 1 2 3)) (vector-fill! v 9) (display v)"
        ),
        "#(9 9 9)"
    );
    assert_eq!(
        eval_ok(
            "b11-e2d.ml",
            "(display (vector->list (list->vector (list 1 2 3))))"
        ),
        "(1 2 3)"
    );
}

#[test]
fn b11_e3_vector_literals_read_and_evaluate_correctly() {
    assert_eq!(eval_ok("b11-e3a.ml", "(display #(1 2 3))"), "#(1 2 3)");
    assert_eq!(eval_ok("b11-e3b.ml", "(display (vector? #(1 2 3)))"), "#t");
    assert_eq!(
        eval_ok("b11-e3c.ml", "(display (vector-ref #(1 2 3) 2))"),
        "3"
    );
}

#[test]
fn b11_e4_hash_table_create_store_retrieve_remove() {
    assert_eq!(
        eval_ok(
            "b11-e4a.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
             (display (hash-count h))"
        ),
        "2"
    );
    assert_eq!(
        eval_ok(
            "b11-e4b.ml",
            "(display (hash-ref (make-hash) (quote c) \"nope\"))"
        ),
        "nope"
    );
    assert_eq!(
        eval_ok(
            "b11-e4c.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (display (hash-has-key? h (quote a)))"
        ),
        "#t"
    );
    assert_eq!(
        eval_ok(
            "b11-e4d.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-remove! h (quote a)) \
             (display (hash-has-key? h (quote a)))"
        ),
        "#f"
    );
    // Deep structural equality: a separately-built but structurally
    // identical compound key still finds the stored value.
    assert_eq!(
        eval_ok(
            "b11-e4e.ml",
            "(define h (make-hash)) (hash-set! h (list 1 2) 42) (display (hash-ref h (list 1 2)))"
        ),
        "42"
    );

    let missing_no_fallback = write_source(
        "b11-e4-missing-no-fallback.ml",
        "(hash-ref (make-hash) (quote c))",
    );
    let output = run(&["eval", missing_no_fallback.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b11_e5_hash_keys_come_back_in_deterministic_insertion_order() {
    assert_eq!(
        eval_ok(
            "b11-e5a.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
             (display (hash-keys h))"
        ),
        "(a b)"
    );
    assert_eq!(
        eval_ok(
            "b11-e5b.ml",
            "(define h (make-hash)) \
             (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) (hash-set! h (quote c) 3) \
             (hash-remove! h (quote a)) (hash-set! h (quote a) 99) \
             (display (hash-keys h))"
        ),
        "(b c a)"
    );
}

#[test]
fn b11_e6_all_twelve_demo_expressions_produce_exactly_the_prescribed_output() {
    assert_eq!(
        run_demo(
            "b11-e6-01.ml",
            "(define v (vector 1 2 3)) (display (vector-ref v 1))"
        ),
        "2\n"
    );
    // Continues mutating the same vector `v` across sequential demo lines,
    // exactly as the behaviour spec's DEMO sequence does, so each demo below
    // reconstructs the needed prior state inline rather than depending on
    // shared state across separate `run_demo` process invocations.
    assert_eq!(
        run_demo(
            "b11-e6-02.ml",
            "(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector-ref v 1))"
        ),
        "99\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-03.ml",
            "(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector-length v))"
        ),
        "3\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-04.ml",
            "(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector->list v))"
        ),
        "(1 99 3)\n"
    );
    assert_eq!(
        run_demo("b11-e6-05.ml", "(display (make-vector 3 0))"),
        "#(0 0 0)\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-06.ml",
            "(display (list->vector (cons 1 (cons 2 (quote ())))))"
        ),
        "#(1 2)\n"
    );
    assert_eq!(run_demo("b11-e6-07.ml", "(display #(1 2 3))"), "#(1 2 3)\n");
    assert_eq!(
        run_demo(
            "b11-e6-08.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
             (display (hash-count h))"
        ),
        "2\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-09.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
             (display (hash-keys h))"
        ),
        "(a b)\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-10.ml",
            "(display (hash-ref (make-hash) (quote c) \"nope\"))"
        ),
        "nope\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-11.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (display (hash-has-key? h (quote a)))"
        ),
        "#t\n"
    );
    assert_eq!(
        run_demo(
            "b11-e6-12.ml",
            "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-remove! h (quote a)) \
             (display (hash-has-key? h (quote a)))"
        ),
        "#f\n"
    );

    // The single end-to-end proof running every step in ONE program, in the
    // same order and against the SAME vector/hash state throughout, exactly
    // as the behaviour spec's DEMO sequence does.
    assert_eq!(
        run_demo(
            "b11-e6-integrated.ml",
            "(define v (vector 1 2 3)) (display (vector-ref v 1)) (newline) \
             (vector-set! v 1 99) (display (vector-ref v 1)) (newline) \
             (display (vector-length v)) (newline) \
             (display (vector->list v)) (newline) \
             (display (make-vector 3 0)) (newline) \
             (display (list->vector (cons 1 (cons 2 (quote ()))))) (newline) \
             (display #(1 2 3)) (newline) \
             (define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
             (display (hash-count h)) (newline) \
             (display (hash-keys h)) (newline) \
             (display (hash-ref h (quote c) \"nope\")) (newline) \
             (display (hash-has-key? h (quote a))) (newline) \
             (hash-remove! h (quote a)) (display (hash-has-key? h (quote a)))"
        ),
        "2\n99\n3\n(1 99 3)\n#(0 0 0)\n#(1 2)\n#(1 2 3)\n2\n(a b)\nnope\n#t\n#f\n"
    );
}
