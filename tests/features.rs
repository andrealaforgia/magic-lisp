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
#[path = "features/steps_b2.rs"]
mod steps_b2;
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
