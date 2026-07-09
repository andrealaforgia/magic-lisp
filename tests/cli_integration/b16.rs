//! B16: the disassembler.

use super::helpers::{run, stdout_of, temp_path, write_source};
use magiclisp::exitcode::SUCCESS;

const DEMO1_SRC: &str = "(define (add-n n) (lambda (x) (+ x n))) (display ((add-n 4) 3)) (newline)";
const DEMO2_SRC: &str =
    "(define (sign n) (if (< n 0) (quote neg) (quote pos))) (display (sign -2)) (newline)";

fn compile_then_disasm(label: &str, src: &str) -> String {
    let file = write_source(&format!("{label}.ml"), src);
    let artifact = temp_path(&format!("{label}.mlbc"));
    let compile_output = run(&[
        "compile",
        file.to_str().unwrap(),
        "-o",
        artifact.to_str().unwrap(),
    ]);
    assert_eq!(compile_output.status.code(), Some(SUCCESS));
    let disasm_output = run(&["disasm", artifact.to_str().unwrap()]);
    assert_eq!(disasm_output.status.code(), Some(SUCCESS));
    stdout_of(&disasm_output)
}

#[test]
fn b16_e1_function_headers_show_index_name_placeholder_arity_variadic_and_upvalue_count() {
    let listing = compile_then_disasm("b16-e1", DEMO1_SRC);
    assert!(
        listing.contains("name=<toplevel>"),
        "expected the entry function's own distinct placeholder: {listing}"
    );
    assert!(
        listing.contains("name=add-n, arity=1, variadic=false"),
        "expected the named outer function's header: {listing}"
    );
    assert!(
        listing.contains("name=<anonymous>") && listing.contains("upvalues=1"),
        "expected the anonymous inner closure's header with its upvalue count: {listing}"
    );
    // The two placeholders must be recognizably distinct from each other.
    assert_ne!("<toplevel>", "<anonymous>");
}

#[test]
fn b16_e2_the_constant_pool_lists_both_symbols_and_a_constant_of_a_different_type() {
    let listing = compile_then_disasm("b16-e2", DEMO2_SRC);
    assert!(listing.contains("Symbol neg"), "{listing}");
    assert!(listing.contains("Symbol pos"), "{listing}");
    assert!(
        listing.contains("Int 0"),
        "expected a non-symbol-typed constant alongside the two symbols: {listing}"
    );
}

#[test]
fn b16_e3_instructions_carry_a_numeric_offset_mnemonic_and_operands() {
    let listing = compile_then_disasm("b16-e3", DEMO1_SRC);
    // The inner (anonymous) function's own instructions: reading a local,
    // reading a captured/upvalue variable, and a return. This
    // implementation has no dedicated arithmetic opcode -- `+` is an
    // ordinary native procedure invoked the same way any other call is,
    // via GET_GLOBAL then CALL/TAIL_CALL -- so that's what appears here
    // instead of a distinct "ADD" mnemonic.
    assert!(listing.contains("GET_LOCAL"), "{listing}");
    assert!(listing.contains("GET_UPVALUE"), "{listing}");
    assert!(listing.contains("TAIL_CALL"), "{listing}");
    assert!(listing.contains("RETURN"), "{listing}");
    // The top-level function's own instructions.
    assert!(listing.contains("MAKE_FUNCTION"), "{listing}");
    assert!(listing.contains("DEF_GLOBAL"), "{listing}");
    assert!(listing.contains("GET_GLOBAL"), "{listing}");
    assert!(listing.contains("CONST"), "{listing}");
    assert!(listing.contains("CALL"), "{listing}");
    assert!(listing.contains("POP"), "{listing}");
    assert!(listing.contains("HALT"), "{listing}");
    // Every instruction line in EVERY function's "code:" section carries a
    // numeric offset -- a hex-formatted line-start on every actual
    // instruction line, none missing. Tracked via a small state machine
    // across the whole multi-function dump (qa test-design review msg
    // #322: a `skip_while`/`take_while` pair anchored on the FIRST "code:"
    // section only ever inspects that one function, leaving a real,
    // demonstrated blind spot for a corrupted offset in any LATER
    // function's own code section).
    let mut in_code_section = false;
    let mut code_lines: Vec<&str> = Vec::new();
    for line in listing.lines() {
        let trimmed = line.trim();
        if line.starts_with("==") || trimmed == "constants:" {
            in_code_section = false;
        } else if trimmed == "code:" {
            in_code_section = true;
        } else if in_code_section {
            code_lines.push(line);
        }
    }
    assert!(!code_lines.is_empty(), "{listing}");
    for line in &code_lines {
        let offset_field = line.split_whitespace().next().unwrap();
        assert!(
            offset_field.len() == 4 && offset_field.chars().all(|c| c.is_ascii_hexdigit()),
            "expected a 4-hex-digit numeric offset, got {offset_field:?} in line {line:?}: {listing}"
        );
    }
}

#[test]
fn b16_e4_a_jump_targets_an_absolute_offset_landing_on_a_real_instruction_boundary() {
    let listing = compile_then_disasm("b16-e4", DEMO2_SRC);
    let jump_line = listing
        .lines()
        .find(|l| l.contains("JUMP_IF_FALSE"))
        .unwrap_or_else(|| panic!("expected a JUMP_IF_FALSE line: {listing}"));
    let target = jump_line
        .split("->")
        .nth(1)
        .unwrap_or_else(|| panic!("expected a '-> target' operand: {jump_line}"))
        .trim();
    let target_offset_field = format!("{target}  ");
    assert!(
        listing
            .lines()
            .any(|l| l.trim_start().starts_with(&target_offset_field)),
        "expected the jump's absolute target {target:?} to match some instruction line's own offset column exactly: {listing}"
    );
}

#[test]
fn b16_e5_both_demo_programs_disassemble_with_every_described_property_present() {
    let demo1 = compile_then_disasm("b16-e5-1", DEMO1_SRC);
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
    assert!(demo1.contains("DEF_GLOBAL"), "{demo1}");
    assert!(demo1.contains("GET_GLOBAL"), "{demo1}");
    assert!(demo1.contains("CONST"), "{demo1}");
    assert!(demo1.contains("CALL"), "{demo1}");
    assert!(demo1.contains("POP"), "{demo1}");
    assert!(demo1.contains("HALT"), "{demo1}");

    let demo2 = compile_then_disasm("b16-e5-2", DEMO2_SRC);
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
}
