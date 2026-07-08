//! B13: quasiquotation (spec 3.4).

use super::helpers::{eval_ok, run_demo};

#[test]
fn b13_e1_a_template_with_no_markers_is_literal_data_not_evaluated_as_code() {
    assert_eq!(eval_ok("b13-e1.ml", "(display `(+ 1 2))"), "(+ 1 2)");
}

#[test]
fn b13_e2_unquote_inserts_a_single_evaluated_value_in_place() {
    assert_eq!(
        eval_ok("b13-e2a.ml", "(define x 10) (display `(a ,x c))"),
        "(a 10 c)"
    );
    assert_eq!(
        eval_ok(
            "b13-e2b.ml",
            "(define x 1) (define y 2) (display `(,x mid ,y))"
        ),
        "(1 mid 2)"
    );
    // The critical distinguishing case versus E3: a list value is inserted
    // as one nested element, not flattened in.
    assert_eq!(
        eval_ok(
            "b13-e2c.ml",
            "(define mid (list 2 3 4)) (display `(1 ,mid 5))"
        ),
        "(1 (2 3 4) 5)"
    );
}

#[test]
fn b13_e3_unquote_splicing_flattens_a_list_values_elements_in() {
    assert_eq!(
        eval_ok(
            "b13-e3a.ml",
            "(define mid (list 2 3 4)) (display `(1 ,@mid 5))"
        ),
        "(1 2 3 4 5)"
    );
    assert_eq!(
        eval_ok("b13-e3b.ml", "(display `(1 ,@(list 2 3) 4))"),
        "(1 2 3 4)"
    );
    assert_eq!(eval_ok("b13-e3c.ml", "(display `(1 ,@(list) 2))"), "(1 2)");
    assert_eq!(
        eval_ok("b13-e3d.ml", "(display `(0 1 ,@(list 2 3) 4 5))"),
        "(0 1 2 3 4 5)"
    );
}

#[test]
fn b13_e4_nested_quasiquote_levels_only_a_marker_reaching_zero_is_evaluated() {
    assert_eq!(
        eval_ok("b13-e4a.ml", "(define y 5) (display `(a `(b ,,y)))"),
        "(a (quasiquote (b (unquote 5))))"
    );
    assert_eq!(
        eval_ok("b13-e4b.ml", "(define y 5) (display `(a `(b ,y)))"),
        "(a (quasiquote (b (unquote y))))"
    );
}

#[test]
fn b13_e5_both_markers_work_inside_a_vector_template() {
    assert_eq!(
        eval_ok("b13-e5a.ml", "(define x 10) (display `#(1 ,x 3))"),
        "#(1 10 3)"
    );
    assert_eq!(
        eval_ok("b13-e5b.ml", "(display `#(1 ,@(list 2 3) 4))"),
        "#(1 2 3 4)"
    );
}

#[test]
fn b13_e6_all_five_demo_expressions_produce_exactly_the_prescribed_output() {
    assert_eq!(
        run_demo(
            "b13-e6-01.ml",
            "(define mid (list 2 3 4)) (write `(1 ,@mid 5))"
        ),
        "(1 2 3 4 5)\n"
    );
    assert_eq!(
        run_demo("b13-e6-02.ml", "(define x 10) (display `(a ,x c))"),
        "(a 10 c)\n"
    );
    assert_eq!(
        run_demo("b13-e6-03.ml", "(display `(1 ,@(list 2 3) 4))"),
        "(1 2 3 4)\n"
    );
    assert_eq!(
        run_demo("b13-e6-04.ml", "(define x 10) (display `#(1 ,x 3))"),
        "#(1 10 3)\n"
    );
    assert_eq!(
        run_demo("b13-e6-05.ml", "(define y 5) (display `(a `(b ,,y)))"),
        "(a (quasiquote (b (unquote 5))))\n"
    );
}
