//! B15: error signalling and the exit procedure.

use super::helpers::{run, stderr_of, stdout_of, write_source};
use magiclisp::exitcode::{RUNTIME_ERROR, SOURCE_ERROR, SUCCESS};

#[test]
fn b15_e1_error_shows_message_display_style_and_irritant_write_style() {
    let file = write_source("b15-e1a.ml", "(error \"boom\" 42)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert_eq!(stdout_of(&output), "");
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().next().unwrap(), "Error: boom 42");
}

#[test]
fn b15_e1_error_irritants_of_different_types_including_a_string_are_shown_write_style() {
    // A bare integer irritant looks identical under display and write, so
    // this specifically exercises a string irritant, which must appear
    // QUOTED (write style) to actually prove irritants aren't just shown
    // in display form.
    let file = write_source(
        "b15-e1b.ml",
        "(error \"bad value\" 1 \"two\" (quote three))",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    let stderr = stderr_of(&output);
    assert_eq!(stderr.lines().next().unwrap(), "Error: bad value 1 \"two\" three");
}

#[test]
fn b15_e2_a_builtin_misuse_stops_before_later_output_with_the_uniform_error_line() {
    let file = write_source(
        "b15-e2-demo.ml",
        "(display \"before\") (newline) (display (car 5)) (display \"after\")",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert_eq!(stdout_of(&output), "before\n");
    assert!(stderr_of(&output).starts_with("Error: "));
}

#[test]
fn b15_e2_division_by_exact_zero_reports_the_same_uniform_error_and_exit_code() {
    let file = write_source("b15-e2-divzero.ml", "(display (/ 1 0))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(stderr_of(&output).starts_with("Error: "));
}

#[test]
fn b15_e2_wrong_argument_count_reports_the_same_uniform_error_and_exit_code() {
    let file = write_source(
        "b15-e2-argcount.ml",
        "(define (f a b) (+ a b)) (display (f 1))",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(stderr_of(&output).starts_with("Error: "));
}

#[test]
fn b15_e2_referencing_an_undefined_name_reports_the_same_uniform_error_and_exit_code() {
    let file = write_source(
        "b15-e2-undefined.ml",
        "(display this-name-does-not-exist)",
    );
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(stderr_of(&output).starts_with("Error: "));
}

#[test]
fn b15_e2_applying_an_operation_to_the_wrong_kind_of_value_reports_the_same_uniform_error_and_exit_code(
) {
    let file = write_source("b15-e2-wrongtype.ml", "(display (+ 1 \"a\"))");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(RUNTIME_ERROR));
    assert!(stderr_of(&output).starts_with("Error: "));
}

#[test]
fn b15_e3_a_read_or_compile_error_exits_with_its_own_code_distinct_from_the_runtime_error_code() {
    let file = write_source("b15-e3.ml", "(display (+ 1");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
    assert_ne!(SOURCE_ERROR, RUNTIME_ERROR);
    assert!(!stderr_of(&output).is_empty());
}

#[test]
fn b15_e4_exit_with_a_chosen_code_terminates_with_that_code_and_no_error_output() {
    let file = write_source("b15-e4a.ml", "(exit 3)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn b15_e4_exit_with_no_argument_means_success() {
    let file = write_source("b15-e4b.ml", "(exit)");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn b15_e4_nothing_after_an_exit_call_executes() {
    let file = write_source("b15-e4c.ml", "(exit 0) (display \"should never appear\")");
    let output = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&output), "");
}

#[test]
fn b15_e5_float_division_by_zero_is_not_an_error_unlike_integer_division_by_zero() {
    let float_file = write_source("b15-e5-float.ml", "(display (/ 1.0 0.0))");
    let float_output = run(&["eval", float_file.to_str().unwrap()]);
    assert_eq!(float_output.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&float_output), "+inf.0");

    // Same operator, same "zero" divisor, but the whole-number case is an
    // error -- the type-dependent distinction this scenario must prove.
    let int_file = write_source("b15-e5-int.ml", "(display (/ 1 0))");
    let int_output = run(&["eval", int_file.to_str().unwrap()]);
    assert_eq!(int_output.status.code(), Some(RUNTIME_ERROR));
}

#[test]
fn b15_e6_all_four_demos_produce_exactly_the_prescribed_output() {
    let f1 = write_source("b15-e6-1.ml", "(error \"boom\" 42)");
    let o1 = run(&["eval", f1.to_str().unwrap()]);
    assert_eq!(o1.status.code(), Some(RUNTIME_ERROR));
    assert_eq!(stdout_of(&o1), "");
    assert_eq!(stderr_of(&o1).lines().next().unwrap(), "Error: boom 42");

    let f2 = write_source(
        "b15-e6-2.ml",
        "(display \"before\") (newline) (display (car 5)) (display \"after\")",
    );
    let o2 = run(&["eval", f2.to_str().unwrap()]);
    assert_eq!(o2.status.code(), Some(RUNTIME_ERROR));
    assert_eq!(stdout_of(&o2), "before\n");
    assert!(stderr_of(&o2).starts_with("Error: "));

    let f3 = write_source("b15-e6-3.ml", "(exit 3)");
    let o3 = run(&["eval", f3.to_str().unwrap()]);
    assert_eq!(o3.status.code(), Some(3));
    assert!(stderr_of(&o3).is_empty());

    let f4 = write_source("b15-e6-4.ml", "(display (/ 1.0 0.0))");
    let o4 = run(&["eval", f4.to_str().unwrap()]);
    assert_eq!(o4.status.code(), Some(SUCCESS));
    assert_eq!(stdout_of(&o4), "+inf.0");
}
