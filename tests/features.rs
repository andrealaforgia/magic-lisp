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
