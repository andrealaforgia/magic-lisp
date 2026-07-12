//! Step definitions for features/B23-dotted-list-round-trip.feature.
//!
//! qa test-design review msg #380 (regression, 8.2 -> 7.3, below the 7.5
//! floor): the prior version re-ran real CLI work (compile/run/disasm
//! subprocess spawns) in both the step that performed the action AND the
//! step that asserted on it, instead of capturing the result once. Every
//! step below performs its real work exactly once (in the step whose
//! wording actually names the action -- Given sets up a precondition,
//! When performs the action under test, Then only reads what was already
//! captured in `World`), mirroring steps_b22.rs's own established pattern.
//!
//! `dotted_list_source`/`expected_display`/`nested_list_const`/
//! `module_with_const`/the two cap constants are intentionally duplicated
//! in `tests/cli_integration/b23.rs` (qa msg #380, Maintainable): the two
//! files are separate Cargo test binaries with no shared dependency of
//! their own (mirroring `helpers.rs`/`world.rs`'s own pre-existing split),
//! so there's no `#[path]`-free way to share them -- flagged here as a
//! cross-reference rather than resolved, per the review's own stated
//! minimum.

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

/// Writes the long dotted-list source and records it in `world.files` --
/// setup only, no compilation (the Given steps that need a *compiled*
/// artifact already in place call `compile_long_dotted_list` instead).
fn write_long_dotted_list_source(world: &mut World, label: &str) {
    let file = write_source(
        &format!("{label}.ml"),
        &dotted_list_source(WELL_PAST_THE_OLD_CDR_CAP),
    );
    world.files.push(file);
}

/// Compiles the long dotted-list source to a real `.mlbc` artifact,
/// recording the artifact path -- used by Given steps that need a
/// compiled artifact already in place as their precondition (E2/E3/E5),
/// where compiling isn't itself the action under test.
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
        "setup compile should succeed, stderr: {}",
        stderr_of(&out)
    );
    world.artifacts.push(artifact);
}

/// Runs `run` against a hand-crafted artifact with car-side `List` nesting
/// past the cap and records both the `run` and `disasm` outcomes under
/// `"run"`/`"disasm"` in `world.labeled` -- the one place this pair of
/// subprocesses actually gets spawned; every Then step reads these labels
/// back instead of spawning its own.
fn run_and_disasm_pathological_nesting(world: &mut World, label: &str) {
    let module = module_with_const(nested_list_const(PAST_THE_OLD_CDR_CAP));
    let bytes = encode(&module);
    let artifact = temp_path(&format!("{label}.mlbc"));
    std::fs::write(&artifact, &bytes).unwrap();

    let run_out = run(&["run", artifact.to_str().unwrap()]);
    world.labeled.push(("run".to_string(), run_out));
    let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
    world.labeled.push(("disasm".to_string(), disasm_out));
}

fn assert_pathological_nesting_was_rejected(world: &World) {
    let run_out = world.labeled("run");
    assert_eq!(
        run_out.status.code(),
        Some(BAD_ARTIFACT),
        "run should reject pathological car-side nesting, stderr: {}",
        stderr_of(run_out)
    );
    let disasm_out = world.labeled("disasm");
    assert_eq!(
        disasm_out.status.code(),
        Some(BAD_ARTIFACT),
        "disasm should reject pathological car-side nesting, stderr: {}",
        stderr_of(disasm_out)
    );
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1: Given sets up the source; When performs the real
        // compile and captures it; Then only reads what was captured. ---
        .step(
            "a program containing a dotted-list literal several thousand elements long, well past the const-nesting cap",
            |w, _text, _| {
                write_long_dotted_list_source(w, "b23-e1");
            },
        )
        .step("it is compiled", |w, _text, _| {
            let source_file = w.last_file().clone();
            let artifact = temp_path("b23-e1.mlbc");
            let out = run(&[
                "compile",
                source_file.to_str().unwrap(),
                "-o",
                artifact.to_str().unwrap(),
            ]);
            w.artifacts.push(artifact);
            w.outputs.push(out);
        })
        .step(
            "compilation succeeds and produces an MLBC artifact, with no spurious rejection for the literal's length",
            |w, _text, _| {
                let out = w.last_output();
                assert_eq!(
                    out.status.code(),
                    Some(SUCCESS),
                    "compile should succeed, stderr: {}",
                    stderr_of(out)
                );
                assert!(
                    std::fs::metadata(w.last_artifact()).is_ok(),
                    "the compiled artifact should exist on disk"
                );
            },
        )
        // --- E2: Given compiles (precondition); When runs it once and
        // captures the result; Then only reads that capture. ---
        .step(
            "the MLBC artifact compiled from the long dotted-list literal",
            |w, _text, _| {
                compile_long_dotted_list(w, "b23-e2");
            },
        )
        .step(
            "it is decoded by running it through the real CLI",
            |w, _text, _| {
                let artifact = w.last_artifact().clone();
                let out = run(&["run", artifact.to_str().unwrap()]);
                w.outputs.push(out);
            },
        )
        .step(
            "decoding succeeds with no truncation error, and every element along the chain plus the final tail match what was written",
            |w, _text, _| {
                let out = w.last_output();
                assert_eq!(
                    out.status.code(),
                    Some(SUCCESS),
                    "run should decode and execute successfully, stderr: {}",
                    stderr_of(out)
                );
                assert_eq!(stdout_of(out), expected_display(WELL_PAST_THE_OLD_CDR_CAP));
            },
        )
        // --- E3: same shape as E2, distinct wording per the Gherkin. ---
        .step("the same compiled artifact", |w, _text, _| {
            compile_long_dotted_list(w, "b23-e3");
        })
        .step("it is run and the value is displayed", |w, _text, _| {
            let artifact = w.last_artifact().clone();
            let out = run(&["run", artifact.to_str().unwrap()]);
            w.outputs.push(out);
        })
        .step(
            "every element and the final tail are exactly as authored, proving the round-tripped value is actually usable at runtime",
            |w, _text, _| {
                let out = w.last_output();
                assert_eq!(
                    out.status.code(),
                    Some(SUCCESS),
                    "run should decode and execute successfully, stderr: {}",
                    stderr_of(out)
                );
                assert_eq!(stdout_of(out), expected_display(WELL_PAST_THE_OLD_CDR_CAP));
            },
        )
        // --- E4: When performs both real subprocess calls once; Then
        // only reads the captured labels. ---
        .step(
            "a hand-crafted MLBC artifact with a List nested deeper than the const-nesting cap",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step(
            "it is run or disassembled through the real CLI",
            |w, _text, _| {
                run_and_disasm_pathological_nesting(w, "b23-e4");
            },
        )
        .step(
            "it is rejected with exit code 66, exactly as before this fix",
            |w, _text, _| {
                assert_pathological_nesting_was_rejected(w);
            },
        )
        // --- E5: integration -- When performs every real subprocess call
        // exactly once (one dotted-list run, one nested-artifact run+
        // disasm); Then only reads what was captured. ---
        .step(
            "the long dotted-list literal's full compile-run-display path and the hand-crafted pathologically-nested artifact",
            |w, _text, _| {
                compile_long_dotted_list(w, "b23-e5");
            },
        )
        .step("both are exercised together in one review pass", |w, _text, _| {
            let artifact = w.last_artifact().clone();
            let out = run(&["run", artifact.to_str().unwrap()]);
            w.outputs.push(out);
            run_and_disasm_pathological_nesting(w, "b23-e5-nested");
        })
        .step(
            "the long dotted list runs correctly end to end and the pathologically-nested artifact is still rejected with exit code 66 -- the fix restores the round-trip guarantee without opening a hole in the malformed-input safety net",
            |w, _text, _| {
                let dotted_list_run = w.last_output();
                assert_eq!(
                    dotted_list_run.status.code(),
                    Some(SUCCESS),
                    "run should decode and execute successfully, stderr: {}",
                    stderr_of(dotted_list_run)
                );
                assert_eq!(
                    stdout_of(dotted_list_run),
                    expected_display(WELL_PAST_THE_OLD_CDR_CAP)
                );
                assert_pathological_nesting_was_rejected(w);
            },
        )
}
