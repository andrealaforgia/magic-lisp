//! B23: dotted-list literals past the const-nesting cap round-trip
//! correctly, without weakening the cap's protection against genuine
//! (car-side) pathological nesting.

use magiclisp::bytecode::{Chunk, Const, Module, encode};
use magiclisp::exitcode::SUCCESS;

use super::helpers::{
    assert_rejected_as_bad_artifact, run, stderr_of, stdout_of, temp_path, write_source,
};

/// Mirrors `bytecode::MAX_CONST_NESTING_DEPTH` (private to that module, so
/// restated here) -- 513 elements is one past it, comfortably confirming
/// the fix doesn't merely shift the cap rather than removing it for cdr
/// chains specifically.
const PAST_THE_OLD_CDR_CAP: usize = 513;
const WELL_PAST_THE_OLD_CDR_CAP: usize = 5_000;

fn dotted_list_source(count: usize) -> String {
    let items: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
    format!("(display (quote ({} . 99999)))", items.join(" "))
}

fn expected_display(count: usize) -> String {
    let items: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
    format!("({} . 99999)", items.join(" "))
}

// --- E1/E2/E3: a long dotted-list literal compiles, round-trips through a
// real .mlbc file, and runs correctly -- exercised together per element
// count, since each count's compile/run pair is the meaningful unit here.

fn assert_dotted_list_round_trips_via_a_real_artifact(label: &str, count: usize) {
    let source_file = write_source(&format!("{label}.ml"), &dotted_list_source(count));
    let artifact = temp_path(&format!("{label}.mlbc"));

    // E1: compiling succeeds and produces an artifact -- no spurious
    // rejection at compile/encode time for a dotted list this long.
    let compile_out = run(&[
        "compile",
        source_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(
        compile_out.status.code(),
        Some(SUCCESS),
        "compile should succeed for a {count}-element dotted list, stderr: {}",
        stderr_of(&compile_out)
    );

    // E2/E3: running the freshly-written artifact (a real decode, not an
    // in-process shortcut) succeeds and produces the exact original value.
    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(
        run_out.status.code(),
        Some(SUCCESS),
        "run should succeed decoding a {count}-element dotted list, stderr: {}",
        stderr_of(&run_out)
    );
    assert_eq!(stdout_of(&run_out), expected_display(count));
}

#[test]
fn e1_e2_e3_a_dotted_list_one_element_past_the_old_cdr_cap_compiles_round_trips_and_runs_correctly()
{
    assert_dotted_list_round_trips_via_a_real_artifact("b23-past-cap", PAST_THE_OLD_CDR_CAP);
}

#[test]
fn e1_e2_e3_a_dotted_list_several_thousand_elements_long_compiles_round_trips_and_runs_correctly() {
    assert_dotted_list_round_trips_via_a_real_artifact(
        "b23-well-past-cap",
        WELL_PAST_THE_OLD_CDR_CAP,
    );
}

// --- E4: genuine car-side/List/Vector nesting past the cap must still be
// rejected -- the fix must not open a hole in the existing protection.

fn nested_list_const(depth: usize) -> Const {
    let mut c = Const::Int(0);
    for _ in 0..depth {
        c = Const::List(vec![c]);
    }
    c
}

fn module_with_const(c: Const) -> Module {
    let mut chunk = Chunk::new();
    let idx = chunk.add_const(c);
    chunk.emit_const(idx);
    chunk.emit_pop();
    chunk.emit_halt();
    Module {
        entry_index: 0,
        functions: vec![chunk],
    }
}

#[test]
fn e4_a_hand_crafted_artifact_with_car_side_list_nesting_past_the_cap_is_still_rejected() {
    let module = module_with_const(nested_list_const(PAST_THE_OLD_CDR_CAP));
    let bytes = encode(&module);
    assert_rejected_as_bad_artifact(&bytes, "b23-e4-nested-list");
}

// --- E5: integration -- both properties hold together in one review pass. ---

#[test]
fn e5_the_long_dotted_list_and_the_rejected_pathological_nesting_both_hold_together() {
    assert_dotted_list_round_trips_via_a_real_artifact(
        "b23-e5-long-dotted-list",
        WELL_PAST_THE_OLD_CDR_CAP,
    );

    let module = module_with_const(nested_list_const(PAST_THE_OLD_CDR_CAP));
    let bytes = encode(&module);
    assert_rejected_as_bad_artifact(&bytes, "b23-e5-nested-list");
}
