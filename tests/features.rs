//! Executes the project's `features/*.feature` files for real: each
//! scenario's Given/When/Then steps are parsed from the file itself and
//! dispatched to a registered Rust step definition that spawns the real
//! compiled `magiclisp` binary and asserts on its actual stdout/stderr/exit
//! code — the same process-level rigor `tests/cli_integration.rs` already
//! established, just driven by the `.feature` files instead of duplicated
//! by hand alongside them.

#[path = "features/gherkin.rs"]
mod gherkin;
#[path = "features/registry.rs"]
mod registry;
#[path = "features/world.rs"]
mod world;

#[path = "features/steps_b1.rs"]
mod steps_b1;
#[path = "features/steps_b10.rs"]
mod steps_b10;
#[path = "features/steps_b11.rs"]
mod steps_b11;
#[path = "features/steps_b12.rs"]
mod steps_b12;
#[path = "features/steps_b13.rs"]
mod steps_b13;
#[path = "features/steps_b14.rs"]
mod steps_b14;
#[path = "features/steps_b15.rs"]
mod steps_b15;
#[path = "features/steps_b16.rs"]
mod steps_b16;
#[path = "features/steps_b17.rs"]
mod steps_b17;
#[path = "features/steps_b18.rs"]
mod steps_b18;
#[path = "features/steps_b19.rs"]
mod steps_b19;
#[path = "features/steps_b2.rs"]
mod steps_b2;
#[path = "features/steps_b20.rs"]
mod steps_b20;
#[path = "features/steps_b3.rs"]
mod steps_b3;
#[path = "features/steps_b4.rs"]
mod steps_b4;
#[path = "features/steps_b5.rs"]
mod steps_b5;
#[path = "features/steps_b6.rs"]
mod steps_b6;
#[path = "features/steps_b7.rs"]
mod steps_b7;
#[path = "features/steps_b8.rs"]
mod steps_b8;
#[path = "features/steps_b9.rs"]
mod steps_b9;

#[test]
fn b1_walking_skeleton() {
    let src = include_str!("../features/B1-walking-skeleton.feature");
    registry::run_feature("B1-walking-skeleton", src, &steps_b1::registry());
}

#[test]
fn b2_functions_recursion_conditionals() {
    let src = include_str!("../features/B2-functions-recursion-conditionals.feature");
    registry::run_feature(
        "B2-functions-recursion-conditionals",
        src,
        &steps_b2::registry(),
    );
}

#[test]
fn b3_local_bindings_mutation_conditionals() {
    let src = include_str!("../features/B3-local-bindings-mutation-conditionals.feature");
    registry::run_feature(
        "B3-local-bindings-mutation-conditionals",
        src,
        &steps_b3::registry(),
    );
}

#[test]
fn b4_iteration_and_numeric_semantics() {
    let src = include_str!("../features/B4-iteration-and-numeric-semantics.feature");
    registry::run_feature(
        "B4-iteration-and-numeric-semantics",
        src,
        &steps_b4::registry(),
    );
}

#[test]
fn b5_closures() {
    let src = include_str!("../features/B5-closures.feature");
    registry::run_feature("B5-closures", src, &steps_b5::registry());
}

#[test]
fn b6_tail_and_deep_recursion() {
    let src = include_str!("../features/B6-tail-and-deep-recursion.feature");
    registry::run_feature("B6-tail-and-deep-recursion", src, &steps_b6::registry());
}

#[test]
fn b7_numeric_library() {
    let src = include_str!("../features/B7-numeric-library.feature");
    registry::run_feature("B7-numeric-library", src, &steps_b7::registry());
}

#[test]
fn b8_type_predicates_and_equality() {
    let src = include_str!("../features/B8-type-predicates-and-equality.feature");
    registry::run_feature(
        "B8-type-predicates-and-equality",
        src,
        &steps_b8::registry(),
    );
}

#[test]
fn b9_pairs_and_lists() {
    let src = include_str!("../features/B9-pairs-and-lists.feature");
    registry::run_feature("B9-pairs-and-lists", src, &steps_b9::registry());
}

#[test]
fn b10_strings_and_characters() {
    let src = include_str!("../features/B10-strings-and-characters.feature");
    registry::run_feature("B10-strings-and-characters", src, &steps_b10::registry());
}

#[test]
fn b11_vectors_and_hash_tables() {
    let src = include_str!("../features/B11-vectors-and-hash-tables.feature");
    registry::run_feature("B11-vectors-and-hash-tables", src, &steps_b11::registry());
}

#[test]
fn b12_io_read_write_display() {
    let src = include_str!("../features/B12-io-read-write-display.feature");
    registry::run_feature("B12-io-read-write-display", src, &steps_b12::registry());
}

#[test]
fn b13_quasiquotation() {
    let src = include_str!("../features/B13-quasiquotation.feature");
    registry::run_feature("B13-quasiquotation", src, &steps_b13::registry());
}

#[test]
fn b14_macros_and_gensym() {
    let src = include_str!("../features/B14-macros-and-gensym.feature");
    registry::run_feature("B14-macros-and-gensym", src, &steps_b14::registry());
}

#[test]
fn b15_errors_and_exit() {
    let src = include_str!("../features/B15-errors-and-exit.feature");
    registry::run_feature("B15-errors-and-exit", src, &steps_b15::registry());
}

#[test]
fn b16_disassembler() {
    let src = include_str!("../features/B16-disassembler.feature");
    registry::run_feature("B16-disassembler", src, &steps_b16::registry());
}

#[test]
fn b17_repl() {
    let src = include_str!("../features/B17-repl.feature");
    registry::run_feature("B17-repl", src, &steps_b17::registry());
}

#[test]
fn b18_robustness() {
    let src = include_str!("../features/B18-robustness.feature");
    registry::run_feature("B18-robustness", src, &steps_b18::registry());
}

#[test]
fn b19_reader_edge_cases_and_conformance() {
    let src = include_str!("../features/B19-reader-edge-cases-and-conformance.feature");
    registry::run_feature(
        "B19-reader-edge-cases-and-conformance",
        src,
        &steps_b19::registry(),
    );
}

#[test]
#[ignore = "self-verifying: its own 'test' check spawns a full nested `cargo test --all`, \
            so this one test alone costs several minutes wall-clock and a rebuilt isolated \
            target directory -- three to four orders of magnitude slower than the rest of \
            this binary combined (qa test-design warning on e3d5e43). Not run by default; \
            invoke explicitly (`cargo test --test features -- --ignored b20_self_test`) \
            before a release or in a dedicated CI job to re-confirm the documented quality \
            gates still hold end to end."]
fn b20_self_test_and_quality_gates() {
    let src = include_str!("../features/B20-self-test-and-quality-gates.feature");
    registry::run_feature(
        "B20-self-test-and-quality-gates",
        src,
        &steps_b20::registry(),
    );
}
