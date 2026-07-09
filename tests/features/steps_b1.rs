//! Step definitions for features/B1-walking-skeleton.feature.

use magiclisp::exitcode::{BAD_ARTIFACT, RUNTIME_ERROR, SOURCE_ERROR, SUCCESS, USAGE_ERROR};

use super::registry::Registry;
use super::world::{run, run_with_stdin, stderr_of, stdout_of, temp_path, write_source};

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- shared across several scenarios ---
        .step(
            "a source file containing \"(display (+ 1 2)) (newline)\"",
            |w, _text, _| {
                w.files
                    .push(write_source("b1-e1e2.ml", "(display (+ 1 2)) (newline)"));
            },
        )
        .step("the user runs `magiclisp eval <file>`", |w, _text, _| {
            let file = w.last_file().clone();
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step("the process exits with code 0", |w, _text, _| {
            assert_eq!(w.last_output().status.code(), Some(SUCCESS));
        })
        // --- E1 ---
        .step("stdout is exactly \"3\\n\"", |w, _text, _| {
            assert_eq!(stdout_of(w.last_output()), "3\n");
        })
        // --- E2 ---
        .step("the user runs `magiclisp compile <file> -o <out>`", |w, _text, _| {
            let file = w.last_file().clone();
            let artifact = temp_path("b1-e2.mlbc");
            let out = run(&[
                "compile",
                file.to_str().unwrap(),
                "-o",
                artifact.to_str().unwrap(),
            ]);
            assert_eq!(out.status.code(), Some(SUCCESS), "compile should succeed");
            w.artifacts.push(artifact);
            w.outputs.push(out);
        })
        .step("then runs `magiclisp run <out>`", |w, _text, _| {
            let artifact = w.last_artifact().clone();
            w.outputs.push(run(&["run", artifact.to_str().unwrap()]));
        })
        .step(
            "stdout is byte-identical to running `magiclisp eval <file>` directly (\"3\\n\")",
            |w, _text, _| {
                let file = w.last_file().clone();
                let eval_out = run(&["eval", file.to_str().unwrap()]);
                let run_stdout = stdout_of(w.last_output());
                assert_eq!(run_stdout, stdout_of(&eval_out));
                assert_eq!(run_stdout, "3\n");
            },
        )
        // --- E3 ---
        .step(
            "a compiled artifact produced from \"(display (+ 1 2)) (newline)\"",
            |w, _text, _| {
                let file = write_source("b1-e3.ml", "(display (+ 1 2)) (newline)");
                let artifact = temp_path("b1-e3.mlbc");
                let out = run(&[
                    "compile",
                    file.to_str().unwrap(),
                    "-o",
                    artifact.to_str().unwrap(),
                ]);
                assert_eq!(out.status.code(), Some(SUCCESS));
                w.files.push(file);
                w.artifacts.push(artifact);
            },
        )
        .step("the user runs `magiclisp disasm <out>`", |w, _text, _| {
            let artifact = w.last_artifact().clone();
            w.outputs.push(run(&["disasm", artifact.to_str().unwrap()]));
        })
        .step(
            "stdout is a legible instruction listing (not raw bytes, not a crash)",
            |w, _text, _| {
                let text = stdout_of(w.last_output());
                assert!(text.is_ascii(), "not legible text: {text}");
                assert!(text.contains("CALL"), "{text}");
                assert!(text.contains("HALT"), "{text}");
                assert!(text.lines().count() >= 3, "{text}");
            },
        )
        // --- E4 ---
        .step(
            "the five verbs compile, run, eval, disasm, repl (repl also being the default with no verb)",
            |_w, _text, _| { /* purely descriptive; the When step below does the real work */ },
        )
        .step("each is invoked with suitable arguments", |w, _text, _| {
            let src = write_source("b1-e4.ml", "(display 1)");
            let eval_out = run(&["eval", src.to_str().unwrap()]);
            assert_eq!(stdout_of(&eval_out), "1");
            assert_eq!(eval_out.status.code(), Some(SUCCESS));

            let artifact = temp_path("b1-e4.mlbc");
            let compile_out = run(&[
                "compile",
                src.to_str().unwrap(),
                "-o",
                artifact.to_str().unwrap(),
            ]);
            assert_eq!(compile_out.status.code(), Some(SUCCESS));
            assert!(std::path::Path::new(&artifact).exists());

            let run_out = run(&["run", artifact.to_str().unwrap()]);
            assert_eq!(stdout_of(&run_out), "1");
            assert_eq!(run_out.status.code(), Some(SUCCESS));

            let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
            assert!(stdout_of(&disasm_out).contains("HALT"));
            assert!(!stdout_of(&disasm_out).trim_start().starts_with('1'));
            assert_eq!(disasm_out.status.code(), Some(SUCCESS));

            // B17 gives the REPL its own real prompt ("> ") before each
            // entry; `display`'s own side-effect output ("1") is
            // interleaved with those prompts, since `display` itself
            // returns the unspecified value (no auto-print of its own).
            let repl_out = run_with_stdin(&["repl"], b"(display 1)\n");
            assert_eq!(stdout_of(&repl_out), "> 1> ");
            assert_eq!(repl_out.status.code(), Some(SUCCESS));

            let default_out = run_with_stdin(&[], b"(display 1)\n");
            assert_eq!(stdout_of(&default_out), "> 1> ");
            assert_eq!(default_out.status.code(), Some(SUCCESS));

            let unknown_out = run(&["frobnicate", src.to_str().unwrap()]);
            assert_eq!(unknown_out.status.code(), Some(USAGE_ERROR));

            w.labeled = vec![
                ("eval".into(), eval_out),
                ("compile".into(), compile_out),
                ("run".into(), run_out),
                ("disasm".into(), disasm_out),
                ("repl".into(), repl_out),
                ("default".into(), default_out),
                ("unknown".into(), unknown_out),
            ];
        })
        .step(
            "none are silently ignored, confused with another verb, or left unrouted",
            |w, _text, _| {
                // Distinct verbs produce distinctly-shaped output: eval/run
                // print the program's own output ("1"), disasm never does.
                assert_eq!(stdout_of(w.labeled("eval")), "1");
                assert_eq!(stdout_of(w.labeled("run")), "1");
                assert!(!stdout_of(w.labeled("disasm")).trim_start().starts_with('1'));
            },
        )
        .step(
            "an unbuilt or unknown verb fails cleanly with a distinct exit code, not a hang or crash",
            |w, _text, _| {
                let unknown = w.labeled("unknown");
                assert_eq!(unknown.status.code(), Some(USAGE_ERROR));
                assert!(!stderr_of(unknown).is_empty());
            },
        )
        // --- E5 ---
        .step("a source file \"kitchen-sink.ml\" containing", |w, _text, docstring| {
            let src = docstring.expect("this step should carry a docstring");
            w.files.push(write_source("kitchen-sink.ml", src));
        })
        .step("the user runs `magiclisp eval kitchen-sink.ml`", |w, _text, _| {
            let file = w.last_file().clone();
            w.outputs.push(run(&["eval", file.to_str().unwrap()]));
        })
        .step("stdout is exactly", |w, _text, docstring| {
            let expected = docstring.expect("this step should carry a docstring");
            // The docstring's dedented lines are joined by "\n" with no
            // trailing newline; the program's own output ends with one
            // (each `(newline)` call), so the docstring needs it added back.
            assert_eq!(stdout_of(w.last_output()), format!("{expected}\n"));
        })
        .step(
            "disassembling the compiled form shows two distinct CALL 2 instructions for the nested \"(+ 42 (+ 1 2))\" call (inner then outer) — the structural fingerprint of a genuinely nested list, distinguishing it from a flattened \"(+ 42 1 2)\" which would disassemble to a single \"CALL 3\"",
            |w, _text, _| {
                let file = w.last_file().clone();
                let artifact = temp_path("kitchen-sink.mlbc");
                let compile_out = run(&[
                    "compile",
                    file.to_str().unwrap(),
                    "-o",
                    artifact.to_str().unwrap(),
                ]);
                assert_eq!(compile_out.status.code(), Some(SUCCESS));
                let listing = stdout_of(&run(&["disasm", artifact.to_str().unwrap()]));
                let call_2_count = listing.lines().filter(|l| l.contains("CALL") && l.contains(" 2")).count();
                assert!(call_2_count >= 2, "expected two CALL 2 instructions: {listing}");
            },
        )
        .step(
            "the leading \";\" comment line produces no output and no read error, proving it was skipped as a comment rather than treated as code",
            |w, _text, _| {
                // Already proven by the successful, exact-match run above;
                // this step re-asserts the specific claim about the comment.
                assert_eq!(w.last_output().status.code(), Some(SUCCESS));
                assert!(stderr_of(w.last_output()).is_empty());
            },
        )
        // --- E6 ---
        .step(
            "a source file containing a string literal with a literal, unescaped newline before its closing quote",
            |w, _text, _| {
                w.files.push(write_source(
                    "b1-e6.ml",
                    "(display \"broken\nstring\")",
                ));
            },
        )
        .step(
            "stderr reports a read error mentioning the unescaped newline",
            |w, _text, _| {
                let stderr = stderr_of(w.last_output());
                assert!(!stderr.is_empty());
                assert!(
                    stderr.to_lowercase().contains("newline") || stderr.to_lowercase().contains("read error"),
                    "{stderr}"
                );
            },
        )
        .step("no stdout is produced", |w, _text, _| {
            assert!(stdout_of(w.last_output()).is_empty());
        })
        .step("the process exits with the source-error exit code", |w, _text, _| {
            assert_eq!(w.last_output().status.code(), Some(SOURCE_ERROR));
        })
        // --- E7 ---
        .step(
            "a compiled artifact corrupted in one of four ways: wrong magic bytes, unsupported version byte, truncated tail, or an out-of-range internal pointer",
            |w, _text, _| {
                let src = write_source("b1-e7.ml", "(display 1)");
                let good_artifact = temp_path("b1-e7-good.mlbc");
                let out = run(&[
                    "compile",
                    src.to_str().unwrap(),
                    "-o",
                    good_artifact.to_str().unwrap(),
                ]);
                assert_eq!(out.status.code(), Some(SUCCESS));
                let bytes = std::fs::read(&good_artifact).unwrap();

                let mut bad_magic = bytes.clone();
                bad_magic[0..4].copy_from_slice(b"NOPE");

                let mut bad_version = bytes.clone();
                bad_version[4] = 200;

                let truncated = bytes[..bytes.len() - 4].to_vec();

                let mut bad_pointer = bytes.clone();
                bad_pointer[8..12].copy_from_slice(&999u32.to_le_bytes());

                for (label, corrupt) in [
                    ("bad_magic", bad_magic),
                    ("bad_version", bad_version),
                    ("truncated", truncated),
                    ("bad_pointer", bad_pointer),
                ] {
                    let path = temp_path(&format!("b1-e7-{label}.mlbc"));
                    std::fs::write(&path, corrupt).unwrap();
                    w.artifacts.push(path);
                }
            },
        )
        .step(
            "the user runs `magiclisp run <corrupt-file>` or `magiclisp disasm <corrupt-file>`",
            |w, _text, _| {
                // w.artifacts holds the 4 corrupt artifacts pushed above;
                // run BOTH verbs against each (8 invocations total).
                let corrupt: Vec<_> = w.artifacts.clone();
                for path in corrupt {
                    w.outputs.push(run(&["run", path.to_str().unwrap()]));
                    w.outputs.push(run(&["disasm", path.to_str().unwrap()]));
                }
            },
        )
        .step("the CLI rejects it with a clear stderr message", |w, _text, _| {
            assert_eq!(w.outputs.len(), 8, "expected 8 recorded invocations");
            for out in &w.outputs {
                assert!(!stderr_of(out).is_empty());
            }
        })
        .step(
            "the process exits with the invalid-artifact exit code",
            |w, _text, _| {
                for out in &w.outputs {
                    assert_eq!(out.status.code(), Some(BAD_ARTIFACT));
                }
            },
        )
        .step(
            "it does not crash, hang, or silently produce wrong output",
            |w, _text, _| {
                // A crash/signal termination shows up as a missing exit
                // code on Unix; hangs would already have made this test
                // suite itself never finish, so process completion plus a
                // real numeric exit code is exactly what rules both out.
                for out in &w.outputs {
                    assert!(out.status.code().is_some());
                }
            },
        )
        // --- E8 ---
        .step(
            "one concrete case for each of: success, incorrect CLI usage, source program error, invalid/corrupt compiled artifact, and a runtime failure",
            |_w, _text, _| { /* descriptive; the When step runs all five cases */ },
        )
        .step("each is run", |w, _text, _| {
            let success_file = write_source("b1-e8-success.ml", "(display 1)");
            let success = run(&["eval", success_file.to_str().unwrap()]);

            let usage = run(&["eval"]);

            let source_file = write_source("b1-e8-source.ml", "\"unterminated");
            let source_err = run(&["eval", source_file.to_str().unwrap()]);

            let bad_artifact = temp_path("b1-e8-bad.mlbc");
            std::fs::write(&bad_artifact, b"garbage, not MLBC").unwrap();
            let bad_artifact_out = run(&["run", bad_artifact.to_str().unwrap()]);

            let runtime_file = write_source("b1-e8-runtime.ml", "(this-is-undefined)");
            let runtime_err = run(&["eval", runtime_file.to_str().unwrap()]);

            w.labeled = vec![
                ("success".into(), success),
                ("usage".into(), usage),
                ("source".into(), source_err),
                ("bad_artifact".into(), bad_artifact_out),
                ("runtime".into(), runtime_err),
            ];
        })
        .step("the exit codes are pairwise distinct", |w, _text, _| {
            let codes: Vec<i32> = w
                .labeled
                .iter()
                .map(|(_, o)| o.status.code().unwrap())
                .collect();
            for i in 0..codes.len() {
                for j in (i + 1)..codes.len() {
                    assert_ne!(
                        codes[i], codes[j],
                        "{} and {} share exit code {}",
                        w.labeled[i].0, w.labeled[j].0, codes[i]
                    );
                }
            }
            assert_eq!(w.labeled[0].1.status.code(), Some(SUCCESS));
            assert_eq!(w.labeled[1].1.status.code(), Some(USAGE_ERROR));
            assert_eq!(w.labeled[2].1.status.code(), Some(SOURCE_ERROR));
            assert_eq!(w.labeled[3].1.status.code(), Some(BAD_ARTIFACT));
            assert_eq!(w.labeled[4].1.status.code(), Some(RUNTIME_ERROR));
        })
        // --- E9 ---
        .step(
            "a source file that displays and newlines the results of (+), (+ 5), (+ 1 2), and (+ 1 2 3 4) in sequence",
            |w, _text, _| {
                w.files.push(write_source(
                    "b1-e9.ml",
                    "(display (+)) (newline) (display (+ 5)) (newline) \
                     (display (+ 1 2)) (newline) (display (+ 1 2 3 4)) (newline)",
                ));
            },
        )
        .step(
            "stdout is exactly \"0\\n5\\n3\\n10\\n\" in that order",
            |w, _text, _| {
                assert_eq!(stdout_of(w.last_output()), "0\n5\n3\n10\n");
            },
        )
        // --- E10 ---
        .step(
            "a source file \"pipeline.ml\" containing \"(display (+ 19 23)) (newline)\"",
            |w, _text, _| {
                w.files
                    .push(write_source("pipeline.ml", "(display (+ 19 23)) (newline)"));
            },
        )
        .step(
            "the user runs `magiclisp compile pipeline.ml -o pipeline.mlbc` in one process",
            |w, _text, _| {
                let file = w.last_file().clone();
                let artifact = temp_path("pipeline.mlbc");
                let out = run(&[
                    "compile",
                    file.to_str().unwrap(),
                    "-o",
                    artifact.to_str().unwrap(),
                ]);
                w.artifacts.push(artifact);
                w.outputs.push(out);
            },
        )
        .step(
            "then runs `magiclisp run pipeline.mlbc` in a separate process",
            |w, _text, _| {
                let artifact = w.last_artifact().clone();
                w.outputs.push(run(&["run", artifact.to_str().unwrap()]));
            },
        )
        .step(
            "then runs `magiclisp disasm pipeline.mlbc` in another separate process",
            |w, _text, _| {
                let artifact = w.last_artifact().clone();
                w.outputs.push(run(&["disasm", artifact.to_str().unwrap()]));
            },
        )
        .step(
            "the compile step exits 0 and leaves the artifact file on disk",
            |w, _text, _| {
                assert_eq!(w.outputs[0].status.code(), Some(SUCCESS));
                assert!(w.last_artifact().exists());
            },
        )
        .step("the run step prints \"42\\n\" and exits 0", |w, _text, _| {
            assert_eq!(stdout_of(&w.outputs[1]), "42\n");
            assert_eq!(w.outputs[1].status.code(), Some(SUCCESS));
        })
        .step(
            "the disasm step prints a legible listing ending in HALT and exits 0",
            |w, _text, _| {
                let listing = stdout_of(&w.outputs[2]);
                assert!(listing.trim_end().ends_with("HALT"), "{listing}");
                assert_eq!(w.outputs[2].status.code(), Some(SUCCESS));
            },
        )
        // --- E11 ---
        .step(
            "the same source file compiled twice via two separate `magiclisp compile` invocations to two different output paths",
            |w, _text, _| {
                let file = write_source(
                    "b1-e11.ml",
                    "(display (+ 1 2)) (newline) (display \"hi\") (display true) (display false)",
                );
                let a = temp_path("b1-e11-a.mlbc");
                let b = temp_path("b1-e11-b.mlbc");
                let out_a = run(&["compile", file.to_str().unwrap(), "-o", a.to_str().unwrap()]);
                assert_eq!(out_a.status.code(), Some(SUCCESS));
                let out_b = run(&["compile", file.to_str().unwrap(), "-o", b.to_str().unwrap()]);
                assert_eq!(out_b.status.code(), Some(SUCCESS));
                w.files.push(file);
                w.artifacts.push(a);
                w.artifacts.push(b);
            },
        )
        .step(
            "the two resulting artifact files are compared byte-for-byte",
            |w, _text, _| {
                let bytes_a = std::fs::read(&w.artifacts[0]).unwrap();
                let bytes_b = std::fs::read(&w.artifacts[1]).unwrap();
                w.notes.push(if bytes_a == bytes_b {
                    "identical".to_string()
                } else {
                    "different".to_string()
                });
            },
        )
        .step("they are byte-identical", |w, _text, _| {
            assert_eq!(w.notes.last().map(String::as_str), Some("identical"));
        })
}
