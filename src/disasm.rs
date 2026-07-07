//! Human-readable disassembly of a compiled module.

use crate::bytecode::{Chunk, Const, Module, Op};
use std::fmt::Write as _;

fn read_u32(code: &[u8], ip: usize) -> Option<u32> {
    let bytes = code.get(ip..ip + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().unwrap()))
}

/// Forces at least one byte of forward progress from `offset`, so a malformed
/// chunk (or a broken decoder) can never make the caller's loop spin forever.
fn advance_past(offset: usize, natural_next: usize) -> usize {
    if natural_next <= offset {
        offset + 1
    } else {
        natural_next
    }
}

/// Consumes one unit of step budget, saturating at zero rather than
/// underflowing, so the budget can only ever shrink toward exhaustion.
fn consume_step(remaining: usize) -> usize {
    remaining.saturating_sub(1)
}

fn describe_const_value(c: &Const) -> String {
    match c {
        Const::Int(n) => n.to_string(),
        Const::Bool(b) => b.to_string(),
        Const::Str(s) => format!("{s:?}"),
        Const::Symbol(s) => s.clone(),
        Const::List(items) => {
            let inner: Vec<String> = items.iter().map(describe_const_value).collect();
            format!("({})", inner.join(" "))
        }
        Const::Unspecified => "<unspecified>".to_string(),
    }
}

fn describe_const(chunk: &Chunk, idx: u32) -> String {
    match chunk.constants.get(idx as usize) {
        Some(c) => describe_const_value(c),
        None => "<out of range>".to_string(),
    }
}

/// Disassembles a single function's bytecode into a legible instruction
/// listing, one instruction per line, without any surrounding header.
pub fn disassemble_chunk(chunk: &Chunk) -> String {
    let mut out = String::new();
    let code = &chunk.code;
    let mut ip = 0usize;

    // A second, independent forward-progress guarantee alongside advance_past:
    // no chunk has more instructions than it has bytes, so this bounds the
    // total number of loop iterations regardless of how `ip` itself behaves.
    let mut remaining_steps = code.len();

    while ip < code.len() {
        if remaining_steps == 0 {
            let _ = writeln!(
                out,
                "<disassembly aborted: decoder made no forward progress>"
            );
            break;
        }
        remaining_steps = consume_step(remaining_steps);

        let offset = ip;
        let opcode = code[ip];
        ip += 1;

        let line = match opcode {
            op if op == Op::Const as u8 => match read_u32(code, ip) {
                Some(idx) => {
                    ip += 4;
                    format!("CONST         {idx:<6} ; {}", describe_const(chunk, idx))
                }
                None => {
                    let line = "CONST         <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::GetGlobal as u8 => match read_u32(code, ip) {
                Some(idx) => {
                    ip += 4;
                    format!("GET_GLOBAL    {idx:<6} ; {}", describe_const(chunk, idx))
                }
                None => {
                    let line = "GET_GLOBAL    <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::DefGlobal as u8 => match read_u32(code, ip) {
                Some(idx) => {
                    ip += 4;
                    format!("DEF_GLOBAL    {idx:<6} ; {}", describe_const(chunk, idx))
                }
                None => {
                    let line = "DEF_GLOBAL    <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::GetLocal as u8 => match code.get(ip) {
                Some(&slot) => {
                    ip += 1;
                    format!("GET_LOCAL     {slot}")
                }
                None => {
                    let line = "GET_LOCAL     <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::SetLocal as u8 => match code.get(ip) {
                Some(&slot) => {
                    ip += 1;
                    format!("SET_LOCAL     {slot}")
                }
                None => {
                    let line = "SET_LOCAL     <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::SetGlobal as u8 => match read_u32(code, ip) {
                Some(idx) => {
                    ip += 4;
                    format!("SET_GLOBAL    {idx:<6} ; {}", describe_const(chunk, idx))
                }
                None => {
                    let line = "SET_GLOBAL    <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::PushLocal as u8 => "PUSH_LOCAL".to_string(),
            op if op == Op::Dup as u8 => "DUP".to_string(),
            op if op == Op::Swap as u8 => "SWAP".to_string(),
            op if op == Op::Eqv as u8 => "EQV".to_string(),
            op if op == Op::MakeFunction as u8 => match read_u32(code, ip) {
                Some(idx) => {
                    ip += 4;
                    format!("MAKE_FUNCTION {idx}")
                }
                None => {
                    let line = "MAKE_FUNCTION <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::Jump as u8 => match read_u32(code, ip) {
                Some(target) => {
                    ip += 4;
                    format!("JUMP          -> {target:04x}")
                }
                None => {
                    let line = "JUMP          <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::JumpIfFalse as u8 => match read_u32(code, ip) {
                Some(target) => {
                    ip += 4;
                    format!("JUMP_IF_FALSE -> {target:04x}")
                }
                None => {
                    let line = "JUMP_IF_FALSE <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::Call as u8 => match code.get(ip) {
                Some(&argc) => {
                    ip += 1;
                    format!("CALL          {argc}")
                }
                None => {
                    let line = "CALL          <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::Pop as u8 => "POP".to_string(),
            op if op == Op::Return as u8 => "RETURN".to_string(),
            op if op == Op::Halt as u8 => "HALT".to_string(),
            other => format!("<unknown opcode {other}>"),
        };

        let _ = writeln!(out, "{offset:04x}  {line}");

        ip = advance_past(offset, ip);
    }

    out
}

/// Disassembles every function in a compiled module, labeling each with its
/// index and marking the entry function.
pub fn disassemble(module: &Module) -> String {
    let mut out = String::new();
    for (index, chunk) in module.functions.iter().enumerate() {
        let marker = if index as u32 == module.entry_index {
            " (entry)"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "== function {index}{marker}: arity={}, has_rest={}, {} bytes code, {} constants ==",
            chunk.arity,
            chunk.has_rest,
            chunk.code.len(),
            chunk.constants.len()
        );
        out.push_str(&disassemble_chunk(chunk));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_program;
    use crate::reader::read_program;

    #[test]
    fn advance_past_keeps_the_natural_next_position_when_it_moved_forward() {
        assert_eq!(advance_past(0, 5), 5);
        assert_eq!(advance_past(3, 4), 4);
    }

    #[test]
    fn advance_past_forces_exactly_one_byte_of_progress_when_natural_next_did_not_advance() {
        assert_eq!(advance_past(3, 3), 4);
        assert_eq!(advance_past(3, 1), 4);
        assert_eq!(advance_past(3, 0), 4);
    }

    #[test]
    fn consume_step_decrements_the_remaining_budget_by_exactly_one() {
        assert_eq!(consume_step(5), 4);
        assert_eq!(consume_step(1), 0);
    }

    #[test]
    fn consume_step_saturates_at_zero_instead_of_underflowing() {
        assert_eq!(consume_step(0), 0);
    }

    fn entry_chunk_for(src: &str) -> Chunk {
        let forms = read_program(src).unwrap();
        let module = compile_program(&forms).unwrap();
        module.functions[module.entry_index as usize].clone()
    }

    fn module_for(src: &str) -> Module {
        let forms = read_program(src).unwrap();
        compile_program(&forms).unwrap()
    }

    #[test]
    fn lists_a_halt_only_program() {
        let listing = disassemble_chunk(&entry_chunk_for("1"));
        assert!(listing.contains("HALT"));
    }

    #[test]
    fn names_every_opcode_used_by_a_call_expression() {
        let listing = disassemble_chunk(&entry_chunk_for("(display (+ 1 2))"));
        assert!(listing.contains("GET_GLOBAL"), "{listing}");
        assert!(listing.contains("CONST"), "{listing}");
        assert!(listing.contains("CALL"), "{listing}");
        assert!(listing.contains("POP"), "{listing}");
        assert!(listing.contains("HALT"), "{listing}");
    }

    #[test]
    fn annotates_get_global_with_the_resolved_symbol_name() {
        let listing = disassemble_chunk(&entry_chunk_for("(newline)"));
        assert!(listing.contains("newline"), "{listing}");
    }

    #[test]
    fn annotates_const_with_the_literal_value() {
        let listing = disassemble_chunk(&entry_chunk_for("42"));
        assert!(listing.contains("42"), "{listing}");
    }

    #[test]
    fn is_legible_multi_line_text_not_a_single_opaque_blob() {
        let listing = disassemble_chunk(&entry_chunk_for("(display (+ 1 2)) (newline)"));
        assert!(listing.lines().count() > 3);
    }

    #[test]
    fn does_not_panic_on_a_chunk_with_a_truncated_trailing_instruction() {
        let mut chunk = Chunk::new();
        chunk.code.push(Op::Const as u8);
        chunk.code.push(0); // only 1 of the required 4 operand bytes
        let listing = disassemble_chunk(&chunk); // must return, not panic
        assert!(!listing.is_empty());
    }

    #[test]
    fn resolves_each_const_operand_to_its_own_distinct_index_not_always_the_first() {
        // "(+ 1 2)" has constants [Symbol("+"), Int(1), Int(2)] at indices 0, 1, 2.
        // A disassembler that always resolved index 0 would show "+" for every
        // operand instead of the actual 1 and 2.
        let listing = disassemble_chunk(&entry_chunk_for("(+ 1 2)"));
        assert!(listing.contains("; 1"), "{listing}");
        assert!(listing.contains("; 2"), "{listing}");
    }

    #[test]
    fn an_unrecognised_opcode_byte_is_reported_as_unknown_not_mistaken_for_halt() {
        let mut chunk = Chunk::new();
        chunk.code.push(253); // no opcode is numbered 253
        let listing = disassemble_chunk(&chunk);
        assert!(listing.contains("unknown opcode"), "{listing}");
        assert!(!listing.contains("HALT"), "{listing}");
    }

    #[test]
    fn never_loops_forever_on_a_pathological_chunk() {
        // A regression guard for the instruction-pointer-advancement invariant:
        // no matter how odd the bytes are, disassemble_chunk() must return.
        let mut chunk = Chunk::new();
        chunk.code = vec![Op::Const as u8, 0, 0, 0, 0, Op::Call as u8, 0, 99, 99, 99];
        let listing = disassemble_chunk(&chunk);
        assert!(!listing.is_empty());
    }

    #[test]
    fn module_level_disassembly_shows_every_function() {
        // (define (f x) x) defines a second function alongside the entry.
        let module = module_for("(define (f x) x)");
        assert_eq!(module.functions.len(), 2);
        let listing = disassemble(&module);
        assert!(listing.contains("function 0"), "{listing}");
        assert!(listing.contains("function 1"), "{listing}");
    }

    #[test]
    fn module_level_disassembly_marks_the_entry_function() {
        let module = module_for("(define (f x) x)");
        let listing = disassemble(&module);
        assert!(listing.contains("(entry)"), "{listing}");
    }

    #[test]
    fn names_the_new_b2_opcodes() {
        let listing = disassemble(&module_for("(define x (if #t 1 2))"));
        assert!(listing.contains("DEF_GLOBAL"), "{listing}");
        assert!(listing.contains("JUMP_IF_FALSE"), "{listing}");
        assert!(listing.contains("JUMP "), "{listing}");
    }

    #[test]
    fn names_get_local_make_function_and_return_for_a_lambda() {
        let listing = disassemble(&module_for("(lambda (x) x)"));
        assert!(listing.contains("GET_LOCAL"), "{listing}");
        assert!(listing.contains("MAKE_FUNCTION"), "{listing}");
        assert!(listing.contains("RETURN"), "{listing}");
    }

    #[test]
    fn advances_past_def_globals_operand_to_the_correct_next_instruction() {
        // (define x 1) at the entry level compiles to CONST, DEF_GLOBAL, POP,
        // HALT. If DEF_GLOBAL's operand advance were wrong, the decoder would
        // start reading POP/HALT's bytes from mid-operand instead, corrupting
        // or dropping them.
        let listing = disassemble_chunk(&entry_chunk_for("(define x 1)"));
        assert_eq!(listing.matches("POP").count(), 1, "{listing}");
        assert_eq!(listing.matches("HALT").count(), 1, "{listing}");
        assert!(!listing.contains("unknown opcode"), "{listing}");
    }

    #[test]
    fn advances_past_make_functions_operand_to_the_correct_next_instruction() {
        let listing = disassemble_chunk(&entry_chunk_for("(lambda () 1)"));
        assert_eq!(listing.matches("POP").count(), 1, "{listing}");
        assert_eq!(listing.matches("HALT").count(), 1, "{listing}");
        assert!(!listing.contains("unknown opcode"), "{listing}");
    }

    #[test]
    fn advances_past_get_locals_operand_to_the_correct_next_instruction() {
        // (define (f x) x) — f's body is GET_LOCAL then RETURN.
        let module = module_for("(define (f x) x)");
        let listing = disassemble_chunk(&module.functions[0]);
        assert_eq!(listing.matches("RETURN").count(), 1, "{listing}");
        assert!(!listing.contains("unknown opcode"), "{listing}");
    }

    #[test]
    fn advances_past_set_locals_operand_to_the_correct_next_instruction() {
        // (lambda (x) (set! x 1) x) — SET_LOCAL, then POP, GET_LOCAL, RETURN.
        let module = module_for("(lambda (x) (set! x 1) x)");
        let listing = disassemble_chunk(&module.functions[0]);
        assert_eq!(listing.matches("POP").count(), 1, "{listing}");
        assert_eq!(listing.matches("RETURN").count(), 1, "{listing}");
        assert!(!listing.contains("unknown opcode"), "{listing}");
    }

    #[test]
    fn advances_past_set_globals_operand_to_the_correct_next_instruction() {
        // (define x 1) (set! x 2) — CONST, DEF_GLOBAL, POP, CONST,
        // SET_GLOBAL, POP, HALT at the entry level.
        let listing = disassemble_chunk(&entry_chunk_for("(define x 1) (set! x 2)"));
        assert_eq!(listing.matches("POP").count(), 2, "{listing}");
        assert_eq!(listing.matches("HALT").count(), 1, "{listing}");
        assert!(!listing.contains("unknown opcode"), "{listing}");
    }

    #[test]
    fn marks_only_the_actual_entry_function_not_the_others() {
        let module = module_for("(define (f x) x)");
        let listing = disassemble(&module);
        let function_headers: Vec<&str> = listing
            .lines()
            .filter(|l| l.starts_with("== function"))
            .collect();
        assert_eq!(function_headers.len(), 2, "{listing}");
        for (index, header) in function_headers.iter().enumerate() {
            if index as u32 == module.entry_index {
                assert!(header.contains("(entry)"), "{header}");
            } else {
                assert!(!header.contains("(entry)"), "{header}");
            }
        }
    }

    #[test]
    fn names_the_b3_local_binding_and_stack_opcodes() {
        // (let ((x 1)) (set! x 2)) exercises PUSH_LOCAL and SET_LOCAL.
        let listing = disassemble(&module_for("(let ((x 1)) (set! x 2))"));
        assert!(listing.contains("PUSH_LOCAL"), "{listing}");
        assert!(listing.contains("SET_LOCAL"), "{listing}");
    }

    #[test]
    fn names_set_global() {
        let listing = disassemble(&module_for("(define x 1) (set! x 2)"));
        assert!(listing.contains("SET_GLOBAL"), "{listing}");
    }

    #[test]
    fn names_dup_and_eqv_from_a_case_expression() {
        let listing = disassemble(&module_for("(case 1 ((1) 'a) (else 'b))"));
        assert!(listing.contains("DUP"), "{listing}");
        assert!(listing.contains("EQV"), "{listing}");
    }

    #[test]
    fn names_swap_from_a_cond_arrow_clause() {
        let listing = disassemble(&module_for("(cond (5 => display))"));
        assert!(listing.contains("SWAP"), "{listing}");
    }

    #[test]
    fn does_not_panic_on_a_chunk_with_a_truncated_set_local_operand() {
        let mut chunk = Chunk::new();
        chunk.code.push(Op::SetLocal as u8);
        let listing = disassemble_chunk(&chunk);
        assert!(!listing.is_empty());
    }

    #[test]
    fn does_not_panic_on_a_chunk_with_a_truncated_set_global_operand() {
        let mut chunk = Chunk::new();
        chunk.code.push(Op::SetGlobal as u8);
        chunk.code.push(0);
        let listing = disassemble_chunk(&chunk);
        assert!(!listing.is_empty());
    }
}
