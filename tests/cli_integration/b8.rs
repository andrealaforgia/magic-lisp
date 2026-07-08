//! B8: type predicates and the three equality relations (spec 3.7, 4.2).

use super::helpers::{eval_ok, run_demo};

#[test]
fn b8_e1_eq_identity_semantics() {
    assert_eq!(
        eval_ok("b8-e1a.ml", "(display (eq? (quote a) (quote a)))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b8-e1b.ml", "(display (eq? (cons 1 2) (cons 1 2)))"),
        "#f"
    );
    assert_eq!(
        eval_ok(
            "b8-e1c.ml",
            "(define p (cons 1 2)) (define q p) (display (eq? p q))"
        ),
        "#t"
    );
    assert_eq!(eval_ok("b8-e1d.ml", "(display (eq? \"ab\" \"ab\"))"), "#f");
    assert_eq!(
        eval_ok(
            "b8-e1e.ml",
            "(define s \"ab\") (define t s) (display (eq? s t))"
        ),
        "#t"
    );
}

#[test]
fn b8_e2_eqv_float_semantics() {
    assert_eq!(eval_ok("b8-e2a.ml", "(display (eqv? 1 1.0))"), "#f");
    assert_eq!(eval_ok("b8-e2b.ml", "(display (eqv? 0.0 -0.0))"), "#f");
    assert_eq!(
        eval_ok("b8-e2c.ml", "(display (eqv? (+ 0.5 0.5) 1.0))"),
        "#t"
    );
    assert_eq!(
        eval_ok("b8-e2d.ml", "(display (eqv? (/ 0.0 0.0) (/ 0.0 0.0)))"),
        "#t"
    );
}

#[test]
fn b8_e3_equal_structural_recursion() {
    assert_eq!(
        eval_ok(
            "b8-e3a.ml",
            "(display (equal? (cons 1 (cons 2 (quote ()))) (cons 1 (cons 2 (quote ())))))"
        ),
        "#t"
    );
    assert_eq!(
        eval_ok("b8-e3b.ml", "(display (equal? \"ab\" \"ab\"))"),
        "#t"
    );
    assert_eq!(
        eval_ok(
            "b8-e3c.ml",
            "(display (equal? (cons 1 (cons (cons 2 (quote ())) (quote ()))) \
                              (cons 1 (cons (cons 2 (quote ())) (quote ())))))"
        ),
        "#t"
    );
    assert_eq!(eval_ok("b8-e3d.ml", "(display (equal? 1 1.0))"), "#f");
}

#[test]
fn b8_e4_not_only_false_is_falsy() {
    assert_eq!(eval_ok("b8-e4a.ml", "(display (not #f))"), "#t");
    assert_eq!(eval_ok("b8-e4b.ml", "(display (not 0))"), "#f");
    assert_eq!(eval_ok("b8-e4c.ml", "(display (not (quote ())))"), "#f");
}

#[test]
fn b8_e5_type_predicates_shown_both_ways() {
    assert_eq!(
        eval_ok(
            "b8-e5a.ml",
            "(display (list? (cons 1 (cons 2 (cons 3 (quote ()))))))"
        ),
        "#t"
    );
    assert_eq!(eval_ok("b8-e5b.ml", "(display (list? (cons 1 2)))"), "#f");
    assert_eq!(eval_ok("b8-e5c.ml", "(display (null? (quote ())))"), "#t");
    assert_eq!(eval_ok("b8-e5d.ml", "(display (pair? (quote ())))"), "#f");
    assert_eq!(eval_ok("b8-e5e.ml", "(display (procedure? +))"), "#t");
    assert_eq!(eval_ok("b8-e5f.ml", "(display (symbol? (quote a)))"), "#t");
    assert_eq!(eval_ok("b8-e5g.ml", "(display (symbol? 5))"), "#f");
    assert_eq!(eval_ok("b8-e5h.ml", "(display (string? \"x\"))"), "#t");
    assert_eq!(eval_ok("b8-e5i.ml", "(display (string? 5))"), "#f");
    assert_eq!(eval_ok("b8-e5j.ml", "(display (char? #\\a))"), "#t");
    assert_eq!(eval_ok("b8-e5k.ml", "(display (char? 5))"), "#f");
    assert_eq!(eval_ok("b8-e5l.ml", "(display (boolean? #t))"), "#t");
    assert_eq!(eval_ok("b8-e5m.ml", "(display (boolean? 5))"), "#f");
    assert_eq!(eval_ok("b8-e5n.ml", "(display (vector? #(1 2)))"), "#t");
    assert_eq!(eval_ok("b8-e5o.ml", "(display (vector? 5))"), "#f");
    assert_eq!(eval_ok("b8-e5p.ml", "(display (hash? (make-hash)))"), "#t");
    assert_eq!(eval_ok("b8-e5q.ml", "(display (hash? 5))"), "#f");
}

#[test]
fn b8_e6_all_twelve_demo_expressions_produce_exactly_the_prescribed_output() {
    assert_eq!(
        run_demo("b8-e6-01.ml", "(display (eq? (quote a) (quote a)))"),
        "#t\n"
    );
    assert_eq!(run_demo("b8-e6-02.ml", "(display (eqv? 1 1.0))"), "#f\n");
    assert_eq!(run_demo("b8-e6-03.ml", "(display (eqv? 0.0 -0.0))"), "#f\n");
    assert_eq!(
        run_demo(
            "b8-e6-04.ml",
            "(display (equal? (cons 1 (cons 2 (quote ()))) (cons 1 (cons 2 (quote ())))))"
        ),
        "#t\n"
    );
    assert_eq!(
        run_demo("b8-e6-05.ml", "(display (equal? \"ab\" \"ab\"))"),
        "#t\n"
    );
    assert_eq!(run_demo("b8-e6-06.ml", "(display (not #f))"), "#t\n");
    assert_eq!(run_demo("b8-e6-07.ml", "(display (not 0))"), "#f\n");
    assert_eq!(
        run_demo(
            "b8-e6-08.ml",
            "(display (list? (cons 1 (cons 2 (cons 3 (quote ()))))))"
        ),
        "#t\n"
    );
    assert_eq!(
        run_demo("b8-e6-09.ml", "(display (list? (cons 1 2)))"),
        "#f\n"
    );
    assert_eq!(
        run_demo("b8-e6-10.ml", "(display (null? (quote ())))"),
        "#t\n"
    );
    assert_eq!(
        run_demo("b8-e6-11.ml", "(display (pair? (quote ())))"),
        "#f\n"
    );
    assert_eq!(run_demo("b8-e6-12.ml", "(display (procedure? +))"), "#t\n");
}
