//! B6: constant-space tail recursion and bounded-but-deep genuine recursion.

use magiclisp::exitcode::RUNTIME_ERROR;

use super::helpers::{run, run_demo, stderr_of, write_source};

const SELF_TAIL_LOOP: &str =
    "(define (loop n limit) (if (= n limit) n (loop (+ n 1) limit))) (display (loop 0 10000000))";

const MUTUAL_TAIL_EVEN_ODD: &str = "(define (even? n) (if (= n 0) #t (odd? (- n 1)))) \
     (define (odd? n) (if (= n 0) #f (even? (- n 1)))) \
     (display (even? 10000000))";

const NON_TAIL_SUM: &str =
    "(define (sum n) (if (= n 0) 0 (+ n (sum (- n 1))))) (display (sum 100000))";

#[test]
fn b6_e1_self_tail_call_loop_counts_to_ten_million() {
    assert_eq!(run_demo("b6-e1.ml", SELF_TAIL_LOOP), "10000000\n");
}

#[test]
fn b6_e2_mutual_tail_call_even_odd_check_at_depth_ten_million() {
    assert_eq!(run_demo("b6-e2.ml", MUTUAL_TAIL_EVEN_ODD), "#t\n");
}

#[test]
fn b6_e3_non_tail_recursive_sum_to_one_hundred_thousand_completes_correctly() {
    assert_eq!(run_demo("b6-e3.ml", NON_TAIL_SUM), "5000050000\n");
}

#[test]
fn b6_e4_non_tail_recursion_far_deeper_than_e3_fails_cleanly_not_a_crash() {
    // Same shape as E3's successful 100,000-level sum, but driven an order
    // of magnitude deeper -- well past whatever depth the implementation
    // actually supports -- to show the boundary: deep-enough succeeds
    // (E3), too-deep fails cleanly with a reported runtime error and a
    // distinct exit code, not a segfault/abort/hang.
    let file = write_source(
        "b6-e4.ml",
        "(define (sum n) (if (= n 0) 0 (+ n (sum (- n 1))))) (display (sum 10000000))",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(RUNTIME_ERROR),
        "stderr: {}",
        stderr_of(&output)
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("call depth"),
        "expected a call-depth error, got: {stderr}"
    );
}

#[test]
fn b6_e5_all_three_demo_programs_produce_exactly_the_prescribed_output() {
    let demo1 = run_demo("b6-e5-demo1.ml", SELF_TAIL_LOOP);
    assert_eq!(demo1, "10000000\n");

    let demo2 = run_demo("b6-e5-demo2.ml", MUTUAL_TAIL_EVEN_ODD);
    assert_eq!(demo2, "#t\n");

    let demo3 = run_demo("b6-e5-demo3.ml", NON_TAIL_SUM);
    assert_eq!(demo3, "5000050000\n");

    // run_demo already asserts each individual invocation exited SUCCESS;
    // this is the single end-to-end proof all three run together correctly
    // in the same suite, not just in isolated unit demonstrations.
}
