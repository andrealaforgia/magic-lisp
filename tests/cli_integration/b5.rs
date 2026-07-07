//! B5: closures that remember and share their surroundings, plus basic pairs.

use super::helpers::eval_ok;

#[test]
fn b5_e1_a_closure_survives_and_uses_its_captured_value_after_the_factory_returns() {
    let out = eval_ok(
        "b5-e1.ml",
        "(define (make-adder n) (lambda (x) (+ x n))) (display ((make-adder 3) 4))",
    );
    assert_eq!(out, "7");
}

#[test]
fn b5_e2_mutating_a_captured_variable_through_one_closure_is_visible_through_another() {
    let out = eval_ok(
        "b5-e2.ml",
        "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v))))) \
         (define p (pairf)) \
         ((cdr p) 10) \
         (display ((car p)))",
    );
    assert_eq!(out, "10");
}

#[test]
fn b5_e3_two_calls_to_the_same_factory_produce_independent_closures() {
    let out = eval_ok(
        "b5-e3.ml",
        "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
         (define a (counter)) \
         (define b (counter)) \
         (display (a)) (newline) \
         (display (a)) (newline) \
         (display (b)) (newline)",
    );
    assert_eq!(out, "1\n2\n1\n");
}

#[test]
fn b5_e4_a_pair_is_constructed_from_two_values_and_each_half_retrieves_correctly() {
    // Distinguishable values, not swapped: displaying car then cdr in order
    // proves each half was retrieved from the correct position.
    let out = eval_ok(
        "b5-e4.ml",
        "(define pr (cons \"first\" \"second\")) (display (car pr)) (display (cdr pr))",
    );
    assert_eq!(out, "firstsecond");
}

#[test]
fn b5_e5_both_demo_programs_produce_exactly_the_prescribed_output() {
    let counter_demo = eval_ok(
        "b5-e5-demo1.ml",
        "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
         (define a (counter)) \
         (define b (counter)) \
         (display (a)) (newline) \
         (display (a)) (newline) \
         (display (b)) (newline)",
    );
    assert_eq!(counter_demo, "1\n2\n1\n");

    let pair_demo = eval_ok(
        "b5-e5-demo2.ml",
        "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v))))) \
         (define p (pairf)) \
         ((cdr p) 10) \
         (display ((car p))) \
         (newline)",
    );
    assert_eq!(pair_demo, "10\n");
}
