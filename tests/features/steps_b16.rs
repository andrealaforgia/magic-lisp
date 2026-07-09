//! Step definitions for features/B16-disassembler.feature.

use super::registry::Registry;
use super::world::{run, stdout_of, write_source};

const DEMO1_SRC: &str =
    "(define (add-n n) (lambda (x) (+ x n))) (display ((add-n 4) 3)) (newline)";
const DEMO2_SRC: &str = "(define (sign n) (if (< n 0) (quote neg) (quote pos))) (display (sign -2)) (newline)";

fn compile_then_disasm(label: &str, src: &str) -> String {
    let file = write_source(&format!("{label}.ml"), src);
    let artifact = super::world::temp_path(&format!("{label}.mlbc"));
    let compile_output = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert!(compile_output.status.success());
    let disasm_output = run(&["disasm", artifact.to_str().unwrap()]);
    assert!(disasm_output.status.success());
    stdout_of(&disasm_output)
}

pub(crate) fn registry() -> Registry {
    Registry::new()
        // --- E1 ---
        .step(
            "a program defining a named function that returns a closure over one of its parameters, called from the top level",
            |w, _text, _| {
                w.pending = vec![DEMO1_SRC.to_string()];
            },
        )
        .step("it is compiled and disassembled", |w, _text, _| {
            let src = w.pending.remove(0);
            w.notes.push(compile_then_disasm("b16-run", &src));
        })
        .step(
            "the dump shows three functions: the top-level entry (its own distinct placeholder), the named outer function (arity 1, non-variadic), and the anonymous inner closure (a DIFFERENT placeholder than the top-level's, reporting exactly 1 captured upvalue)",
            |w, _text, _| {
                let listing = w.notes.last().unwrap();
                assert!(listing.contains("name=<toplevel>"), "{listing}");
                assert!(
                    listing.contains("name=add-n, arity=1, variadic=false"),
                    "{listing}"
                );
                assert!(
                    listing.contains("name=<anonymous>") && listing.contains("upvalues=1"),
                    "{listing}"
                );
            },
        )
        // --- E2 ---
        .step(
            "a program whose constant pool contains both symbols and a number",
            |w, _text, _| {
                w.pending = vec![DEMO2_SRC.to_string()];
            },
        )
        .step(
            "each constant entry shows its index, its type label, and its value in write form, with the type label correctly varying across at least two distinct types",
            |w, _text, _| {
                let listing = w.notes.last().unwrap();
                assert!(listing.contains("Symbol neg"), "{listing}");
                assert!(listing.contains("Symbol pos"), "{listing}");
                assert!(listing.contains("Int 0"), "{listing}");
            },
        )
        // --- E3 ---
        .step("the same closure-over-parameter program", |w, _text, _| {
            w.pending = vec![DEMO1_SRC.to_string()];
        })
        .step(
            "the inner function's instructions include reading a local, reading a captured upvalue, and a return (with the arithmetic itself expressed as an ordinary global-procedure call — GET_GLOBAL + TAIL_CALL — since `+` is an established first-class, redefinable procedure, not a special-cased operator, per B7), the top-level function's instructions include constructing a closure, defining a global, reading a global, loading a constant, making a call, discarding a value, and a halt, and every line in both dumps carries a numeric offset",
            |w, _text, _| {
                let listing = w.notes.last().unwrap();
                assert!(listing.contains("GET_LOCAL"), "{listing}");
                assert!(listing.contains("GET_UPVALUE"), "{listing}");
                assert!(listing.contains("TAIL_CALL"), "{listing}");
                assert!(listing.contains("RETURN"), "{listing}");
                assert!(listing.contains("MAKE_FUNCTION"), "{listing}");
                assert!(listing.contains("DEF_GLOBAL"), "{listing}");
                assert!(listing.contains("GET_GLOBAL"), "{listing}");
                assert!(listing.contains("CONST"), "{listing}");
                assert!(listing.contains("CALL"), "{listing}");
                assert!(listing.contains("POP"), "{listing}");
                assert!(listing.contains("HALT"), "{listing}");
                // Collects only the "code:" section's own lines from every
                // function block in the dump (skipping headers and the
                // "constants:" section, which has its own differently-
                // shaped "index: Type value" lines) via a small state
                // machine tracking which section is currently being read.
                let mut in_code_section = false;
                let mut code_lines: Vec<&str> = Vec::new();
                for line in listing.lines() {
                    let trimmed = line.trim();
                    if line.starts_with("==") {
                        in_code_section = false;
                    } else if trimmed == "constants:" {
                        in_code_section = false;
                    } else if trimmed == "code:" {
                        in_code_section = true;
                    } else if in_code_section {
                        code_lines.push(line);
                    }
                }
                assert!(!code_lines.is_empty(), "{listing}");
                for line in &code_lines {
                    let offset_field = line.trim_start().split_whitespace().next().unwrap();
                    assert!(
                        offset_field.len() == 4
                            && offset_field.chars().all(|c| c.is_ascii_hexdigit()),
                        "expected a 4-hex-digit numeric offset, got {offset_field:?}: {listing}"
                    );
                }
            },
        )
        // --- E4 ---
        .step(
            "the conditional-branch program's compiled and disassembled form",
            |w, _text, _| {
                w.notes.push(compile_then_disasm("b16-e4", DEMO2_SRC));
            },
        )
        .step(
            "the conditional-jump instruction's target value is cross-referenced against the dump's own offset column",
            |_w, _text, _| {},
        )
        .step(
            "the target value exactly matches another instruction's own offset elsewhere in the same dump, proving it's a genuine absolute address, not a relative displacement or an arbitrary number",
            |w, _text, _| {
                let listing = w.notes.last().unwrap();
                let jump_line = listing
                    .lines()
                    .find(|l| l.contains("JUMP_IF_FALSE"))
                    .unwrap_or_else(|| panic!("expected a JUMP_IF_FALSE line: {listing}"));
                let target = jump_line.split("->").nth(1).unwrap().trim();
                let target_offset_field = format!("{target}  ");
                assert!(
                    listing
                        .lines()
                        .any(|l| l.trim_start().starts_with(&target_offset_field)),
                    "expected the jump's absolute target {target:?} to match some instruction line's own offset column exactly: {listing}"
                );
            },
        )
        // --- E5 ---
        .step(
            "both DEMO programs from the behaviour spec, each compiled and disassembled",
            |w, _text, _| {
                w.notes.push(compile_then_disasm("b16-e5-1", DEMO1_SRC));
                w.notes.push(compile_then_disasm("b16-e5-2", DEMO2_SRC));
            },
        )
        .step("the full dumps are inspected", |_w, _text, _| {})
        .step(
            "demo 1's three-function structure (correct placeholders/arity/variadic/upvalue-count, required instructions in both the inner and top-level functions, all lines offset) and demo 2's absolute boundary-landing jump target plus its two-symbol constant pool all hold at once",
            |w, _text, _| {
                let demo1 = &w.notes[0];
                assert!(demo1.contains("name=<toplevel>"), "{demo1}");
                assert!(
                    demo1.contains("name=add-n, arity=1, variadic=false"),
                    "{demo1}"
                );
                assert!(
                    demo1.contains("name=<anonymous>") && demo1.contains("upvalues=1"),
                    "{demo1}"
                );
                assert!(demo1.contains("GET_LOCAL"), "{demo1}");
                assert!(demo1.contains("GET_UPVALUE"), "{demo1}");
                assert!(demo1.contains("RETURN"), "{demo1}");
                assert!(demo1.contains("MAKE_FUNCTION"), "{demo1}");
                assert!(demo1.contains("HALT"), "{demo1}");

                let demo2 = &w.notes[1];
                let jump_line = demo2
                    .lines()
                    .find(|l| l.contains("JUMP_IF_FALSE"))
                    .unwrap_or_else(|| panic!("expected a JUMP_IF_FALSE line: {demo2}"));
                let target = jump_line.split("->").nth(1).unwrap().trim();
                let target_offset_field = format!("{target}  ");
                assert!(
                    demo2
                        .lines()
                        .any(|l| l.trim_start().starts_with(&target_offset_field)),
                    "{demo2}"
                );
                assert!(demo2.contains("Symbol neg"), "{demo2}");
                assert!(demo2.contains("Symbol pos"), "{demo2}");
            },
        )
}
