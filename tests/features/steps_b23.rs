//! Step definitions for features/B23-dotted-list-round-trip.feature.

use magiclisp::bytecode::{Chunk, Const, Module, encode};
use magiclisp::exitcode::{BAD_ARTIFACT, SUCCESS};

use super::registry::Registry;
use super::world::{World, run, stderr_of, stdout_of, temp_path, write_source};

/// Mirrors `bytecode::MAX_CONST_NESTING_DEPTH` (private to that module, so
/// restated here) -- several thousand is comfortably past it, matching the
/// expectation's own "several thousand elements" framing.
const WELL_PAST_THE_OLD_CDR_CAP: usize = 5_000;
const PAST_THE_OLD_CDR_CAP: usize = 513;

fn dotted_list_source(count: usize) -> String {
    let items: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
    format!("(display (quote ({} . 99999)))", items.join(" "))
}

fn expected_display(count: usize) -> String {
    let items: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
    format!("({} . 99999)", items.join(" "))
}

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

/// Compiles the long dotted-list source to a real `.mlbc` artifact,
/// asserting compilation succeeds, and records the artifact path.
fn compile_long_dotted_list(world: &mut World, label: &str) {
    let source_file = write_source(
        &format!("{label}.ml"),
        &dotted_list_source(WELL_PAST_THE_OLD_CDR_CAP),
    );
    let artifact = temp_path(&format!("{label}.mlbc"));
    let out = run(&[
        "compile",
        source_file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(SUCCESS),
        "compile should succeed, stderr: {}",
        stderr_of(&out)
    );
    world.artifacts.push(artifact);
}

fn assert_run_produces_the_expected_long_dotted_list(world: &World) {
    let artifact = world.last_artifact().clone();
    let out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(SUCCESS),
        "run should decode and execute successfully, stderr: {}",
        stderr_of(&out)
    );
    assert_eq!(stdout_of(&out), expected_display(WELL_PAST_THE_OLD_CDR_CAP));
}

fn assert_pathological_nesting_still_rejected(label: &str) {
    let module = module_with_const(nested_list_const(PAST_THE_OLD_CDR_CAP));
    let bytes = encode(&module);
    let artifact = temp_path(&format!("{label}.mlbc"));
    std::fs::write(&artifact, &bytes).unwrap();

    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(
        run_out.status.code(),
        Some(BAD_ARTIFACT),
        "run should still reject pathological car-side nesting, stderr: {}",
        stderr_of(&run_out)
    );

    let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
    assert_eq!(
        disasm_out.status.code(),
        Some(BAD_ARTIFACT),
        "disasm should still reject pathological car-side nesting, stderr: {}",
        stderr_of(&disasm_out)
    );
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1/E2/E3 share the same compile-then-run artifact. ---
        .step(
            "a program containing a dotted-list literal several thousand elements long, well past the const-nesting cap",
            |w, _text, _| {
                compile_long_dotted_list(w, "b23-e1");
            },
        )
        .step("it is compiled", |_w, _text, _| {
            // compilation already happened in the Given step, whose own
            // assertion already confirms success; nothing further to do.
        })
        .step(
            "compilation succeeds and produces an MLBC artifact, with no spurious rejection for the literal's length",
            |w, _text, _| {
                let artifact = w.last_artifact();
                assert!(
                    std::fs::metadata(artifact).is_ok(),
                    "the compiled artifact should exist on disk"
                );
            },
        )
        .step(
            "the MLBC artifact compiled from the long dotted-list literal",
            |w, _text, _| {
                compile_long_dotted_list(w, "b23-e2");
            },
        )
        .step("it is decoded by running it through the real CLI", |w, _text, _| {
            assert_run_produces_the_expected_long_dotted_list(w);
        })
        .step(
            "decoding succeeds with no truncation error, and every element along the chain plus the final tail match what was written",
            |w, _text, _| {
                assert_run_produces_the_expected_long_dotted_list(w);
            },
        )
        .step("the same compiled artifact", |w, _text, _| {
            compile_long_dotted_list(w, "b23-e3");
        })
        .step("it is run and the value is displayed", |w, _text, _| {
            assert_run_produces_the_expected_long_dotted_list(w);
        })
        .step(
            "every element and the final tail are exactly as authored, proving the round-tripped value is actually usable at runtime",
            |w, _text, _| {
                assert_run_produces_the_expected_long_dotted_list(w);
            },
        )
        // --- E4 ---
        .step(
            "a hand-crafted MLBC artifact with a List nested deeper than the const-nesting cap",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("it is run or disassembled through the real CLI", |_w, _text, _| {
            assert_pathological_nesting_still_rejected("b23-e4");
        })
        .step(
            "it is rejected with exit code 66, exactly as before this fix",
            |_w, _text, _| {
                assert_pathological_nesting_still_rejected("b23-e4-then");
            },
        )
        // --- E5: integration ---
        .step(
            "the long dotted-list literal's full compile-run-display path and the hand-crafted pathologically-nested artifact",
            |w, _text, _| {
                compile_long_dotted_list(w, "b23-e5");
            },
        )
        .step("both are exercised together in one review pass", |w, _text, _| {
            assert_run_produces_the_expected_long_dotted_list(w);
            assert_pathological_nesting_still_rejected("b23-e5-nested");
        })
        .step(
            "the long dotted list runs correctly end to end and the pathologically-nested artifact is still rejected with exit code 66 -- the fix restores the round-trip guarantee without opening a hole in the malformed-input safety net",
            |w, _text, _| {
                assert_run_produces_the_expected_long_dotted_list(w);
                assert_pathological_nesting_still_rejected("b23-e5-final");
            },
        )
}
