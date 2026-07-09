//! Step definitions for features/B18-robustness.feature.

use super::registry::Registry;
use super::world::{run, stderr_of, temp_path, write_source, World};
use magiclisp::exitcode::{BAD_ARTIFACT, RUNTIME_ERROR, SOURCE_ERROR, SUCCESS, USAGE_ERROR};

const ESTABLISHED_CODES: [i32; 5] =
    [SUCCESS, USAGE_ERROR, SOURCE_ERROR, BAD_ARTIFACT, RUNTIME_ERROR];

fn exited_cleanly(output: &std::process::Output) -> bool {
    output.status.code().is_some()
}

/// Compiles the fixed source `(display 1)` to a fresh `.mlbc` artifact and
/// returns its raw bytes -- this feature's own equivalent of
/// `tests/cli_integration/helpers.rs`'s `compile_good_artifact`, kept
/// local since this test binary is a separate crate target with its own
/// independent helper module.
fn compile_good_artifact(label: &str) -> Vec<u8> {
    let file = write_source(&format!("{label}.ml"), "(display 1)");
    let artifact = temp_path(&format!("{label}.mlbc"));
    let out = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(SUCCESS));
    std::fs::read(&artifact).unwrap()
}

/// The byte range of the entry chunk's own `code` section within a
/// `compile_good_artifact`-produced file -- see the identical helper in
/// `tests/cli_integration/b18.rs` for the full layout reasoning.
fn entry_code_range(bytes: &[u8]) -> std::ops::Range<usize> {
    let code_len = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
    26..26 + code_len
}

/// Runs every queued full CLI invocation in `world.pending_commands`,
/// appending each real process `Output` to `world.outputs` -- the shared
/// implementation behind the "each is run" When step, reused verbatim by
/// E1/E2/E4/E6 (all four share this exact wording in the feature file)
/// with Givens spanning both malformed source files and corrupted
/// artifact files.
fn run_pending_commands(world: &mut World) {
    let commands = std::mem::take(&mut world.pending_commands);
    for args in commands {
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        world.outputs.push(run(&arg_refs));
    }
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "unbalanced parentheses, an unterminated string, a raw unescaped newline inside a string, an oversized whole-number literal, and a stray misplaced dot",
            |w, _text, _| {
                let cases: [(&str, &str); 5] = [
                    ("b18-e1-parens", "(display (+ 1 2)"),
                    ("b18-e1-string", "(display \"unterminated"),
                    ("b18-e1-newline", "(display \"broken\nstring\")"),
                    ("b18-e1-overflow", "(display 999999999999999999999999999)"),
                    ("b18-e1-dot", "(display (1 . 2 . 3))"),
                ];
                w.pending_commands = cases
                    .iter()
                    .map(|(label, src)| {
                        let file = write_source(&format!("{label}.ml"), src);
                        vec!["eval".to_string(), file.to_str().unwrap().to_string()]
                    })
                    .collect();
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "each ends the process with the source-error exit code and no crash output",
            |w, _text, _| {
                for output in &w.outputs {
                    assert!(exited_cleanly(output), "{output:?}");
                    assert_eq!(output.status.code(), Some(SOURCE_ERROR));
                    assert!(!stderr_of(output).is_empty());
                }
                assert_eq!(w.outputs.len(), 5);
            },
        )
        // --- E2 ---
        .step(
            "an artifact with non-zero flags where none are allowed, handed to both run and disasm",
            |w, _text, _| {
                let mut bytes = compile_good_artifact("b18-e2-flags");
                bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
                let artifact = temp_path("b18-e2-flags-bad.mlbc");
                std::fs::write(&artifact, &bytes).unwrap();
                let path = artifact.to_str().unwrap().to_string();
                w.pending_commands = vec![
                    vec!["run".to_string(), path.clone()],
                    vec!["disasm".to_string(), path],
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step("each ends with the file-format-error exit code", |w, _text, _| {
            assert_eq!(w.outputs.len(), 2);
            for output in &w.outputs {
                assert!(exited_cleanly(output), "{output:?}");
                assert_eq!(output.status.code(), Some(BAD_ARTIFACT));
                assert!(!stderr_of(output).is_empty());
            }
        })
        // --- E3 ---
        .step(
            "an artifact with an undefined opcode byte, an out-of-range constant-pool index, and an instruction truncated mid-operand — each corrupting only internal code/constant bytes, not the header",
            |w, _text, _| {
                let good = compile_good_artifact("b18-e3-base");
                let code_range = entry_code_range(&good);

                let mut opcode_bad = good.clone();
                opcode_bad[code_range.end - 1] = 250;
                let opcode_path = temp_path("b18-e3-opcode.mlbc");
                std::fs::write(&opcode_path, &opcode_bad).unwrap();

                let mut const_idx_bad = good.clone();
                let operand = code_range.start + 1..code_range.start + 5;
                const_idx_bad[operand].copy_from_slice(&u32::MAX.to_le_bytes());
                let const_idx_path = temp_path("b18-e3-constidx.mlbc");
                std::fs::write(&const_idx_path, &const_idx_bad).unwrap();

                let mut truncated = good.clone();
                truncated[code_range.end - 1] = 0; // CONST opcode, no operand left
                let truncated_path = temp_path("b18-e3-truncated.mlbc");
                std::fs::write(&truncated_path, &truncated).unwrap();

                w.notes = vec![
                    opcode_path.to_str().unwrap().to_string(),
                    const_idx_path.to_str().unwrap().to_string(),
                    truncated_path.to_str().unwrap().to_string(),
                ];
            },
        )
        .step(
            "each is run (and, for the undefined-opcode case, also disassembled)",
            |w, _text, _| {
                let opcode_path = w.notes[0].clone();
                let const_idx_path = w.notes[1].clone();
                let truncated_path = w.notes[2].clone();
                w.outputs.push(run(&["run", &opcode_path]));
                w.outputs.push(run(&["disasm", &opcode_path]));
                w.outputs.push(run(&["run", &const_idx_path]));
                w.outputs.push(run(&["run", &truncated_path]));
            },
        )
        .step(
            "each is reported as a runtime error or file-format error (either acceptable) with no crash, and disasm may instead gracefully label an unrecognized opcode and continue rather than erroring, since exit 0 with no crash also satisfies \"always lands on an established outcome\"",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 4);
                let run_opcode = &w.outputs[0];
                assert!(exited_cleanly(run_opcode));
                assert!(matches!(
                    run_opcode.status.code(),
                    Some(RUNTIME_ERROR) | Some(BAD_ARTIFACT)
                ));

                let disasm_opcode = &w.outputs[1];
                assert!(exited_cleanly(disasm_opcode));

                let run_const_idx = &w.outputs[2];
                assert!(exited_cleanly(run_const_idx));
                assert!(matches!(
                    run_const_idx.status.code(),
                    Some(RUNTIME_ERROR) | Some(BAD_ARTIFACT)
                ));

                let run_truncated = &w.outputs[3];
                assert!(exited_cleanly(run_truncated));
                assert!(matches!(
                    run_truncated.status.code(),
                    Some(RUNTIME_ERROR) | Some(BAD_ARTIFACT)
                ));
            },
        )
        // --- E4 ---
        .step(
            "an unrecognized verb and a required argument left off",
            |w, _text, _| {
                let file = write_source("b18-e4.ml", "(display 1)");
                w.pending_commands = vec![
                    vec!["frobnicate".to_string(), file.to_str().unwrap().to_string()],
                    vec!["eval".to_string()],
                ];
            },
        )
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step("each ends with the usage-error exit code", |w, _text, _| {
            assert_eq!(w.outputs.len(), 2);
            for output in &w.outputs {
                assert!(exited_cleanly(output), "{output:?}");
                assert_eq!(output.status.code(), Some(USAGE_ERROR));
            }
        })
        // --- E5 ---
        .step(
            "dozens of malformed source snippets spanning many categories beyond E1's five, and every possible single-byte corruption of a valid compiled artifact run through both run and disasm",
            |_w, _text, _| {},
        )
        .step("the full sweep is executed", |w, _text, _| {
            let snippets = [
                "(display (+ 1 2)",
                "(display \"unterminated",
                "(display \"broken\nstring\")",
                "(display 999999999999999999999999999)",
                "(display (1 . 2 . 3))",
                "(",
                "(((((((((((",
                ")",
                "))))))))))",
                "",
                "   \n\t  ",
                "; just a comment, nothing else",
                "(display \"bad escape \\q\")",
                "(display \"unterminated escape \\",
                "#(1 2",
                "#(",
                "#\\",
                "#z",
                "#xzz",
                "#bzz",
                "#ozz",
                "(display .)",
                "(. )",
                "(display (quote )",
                "(display (lambda))",
                "(define)",
                "(display (+ 1 2",
                "\"",
                "(\"",
                "(display #t",
            ];
            for (i, src) in snippets.iter().enumerate() {
                let file = write_source(&format!("b18-e5-src-{i}.ml"), src);
                let out = run(&["eval", file.to_str().unwrap()]);
                assert!(exited_cleanly(&out), "snippet {i} ({src:?})");
                assert!(
                    ESTABLISHED_CODES.contains(&out.status.code().unwrap()),
                    "snippet {i} ({src:?}) exit {:?}",
                    out.status.code()
                );
            }

            let good = compile_good_artifact("b18-e5-sweep-base");
            for i in 0..good.len() {
                let mut bytes = good.clone();
                bytes[i] = 0xFF;
                let artifact = temp_path(&format!("b18-e5-artifact-{i}.mlbc"));
                std::fs::write(&artifact, &bytes).unwrap();
                for verb in ["run", "disasm"] {
                    let out = run(&[verb, artifact.to_str().unwrap()]);
                    assert!(exited_cleanly(&out), "offset {i} ({verb})");
                    assert!(
                        ESTABLISHED_CODES.contains(&out.status.code().unwrap()),
                        "offset {i} ({verb}) exit {:?}",
                        out.status.code()
                    );
                }
            }
            w.notes.push(format!(
                "{} source snippets + {} artifact offsets (x2 verbs) all exited cleanly on an established code",
                snippets.len(),
                good.len()
            ));
        })
        .step(
            "every single run exits on an established exit code with no signal-killed/crash termination",
            |w, _text, _| {
                assert!(!w.notes.is_empty(), "the sweep step should have run and recorded a summary");
            },
        )
        // --- E6 ---
        .step("each of the six DEMO cases from the behaviour spec", |w, _text, _| {
            let demo1 = write_source("b18-e6-1.ml", "(((");
            let demo2 = write_source("b18-e6-2.ml", "(display \"no closing quote)");
            let demo3 = write_source(
                "b18-e6-3.ml",
                "(display 123456789012345678901234567890)",
            );
            let demo4 = temp_path("b18-e6-4.mlbc");
            std::fs::write(&demo4, b"this is not a valid MLBC artifact at all").unwrap();
            let good = compile_good_artifact("b18-e6-5");
            let demo5 = temp_path("b18-e6-5.mlbc");
            std::fs::write(&demo5, &good[..good.len() / 2]).unwrap();

            w.pending_commands = vec![
                vec!["eval".to_string(), demo1.to_str().unwrap().to_string()],
                vec!["eval".to_string(), demo2.to_str().unwrap().to_string()],
                vec!["eval".to_string(), demo3.to_str().unwrap().to_string()],
                vec!["run".to_string(), demo4.to_str().unwrap().to_string()],
                vec!["run".to_string(), demo5.to_str().unwrap().to_string()],
                vec!["bogus-verb".to_string()],
            ];
        })
        .step("each is run", |w, _text, _| {
            run_pending_commands(w);
        })
        .step(
            "each exits with exactly its prescribed established exit code, with no crash output",
            |w, _text, _| {
                assert_eq!(w.outputs.len(), 6);
                let expected = [
                    SOURCE_ERROR,
                    SOURCE_ERROR,
                    SOURCE_ERROR,
                    BAD_ARTIFACT,
                    BAD_ARTIFACT,
                    USAGE_ERROR,
                ];
                for (output, code) in w.outputs.iter().zip(expected) {
                    assert!(exited_cleanly(output), "{output:?}");
                    assert_eq!(output.status.code(), Some(code));
                }
            },
        )
}
