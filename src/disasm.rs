//! Human-readable disassembly of a compiled chunk.

use crate::bytecode::{Chunk, Const, Op};
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

fn describe_const(chunk: &Chunk, idx: u32) -> String {
    match chunk.constants.get(idx as usize) {
        Some(Const::Int(n)) => n.to_string(),
        Some(Const::Bool(b)) => b.to_string(),
        Some(Const::Str(s)) => format!("{s:?}"),
        Some(Const::Symbol(s)) => s.clone(),
        None => "<out of range>".to_string(),
    }
}

pub fn disassemble(chunk: &Chunk) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "== chunk: {} bytes code, {} constants ==",
        chunk.code.len(),
        chunk.constants.len()
    );
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
                    format!("CONST      {idx:<6} ; {}", describe_const(chunk, idx))
                }
                None => {
                    let line = "CONST      <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::GetGlobal as u8 => match read_u32(code, ip) {
                Some(idx) => {
                    ip += 4;
                    format!("GET_GLOBAL {idx:<6} ; {}", describe_const(chunk, idx))
                }
                None => {
                    let line = "GET_GLOBAL <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::Call as u8 => match code.get(ip) {
                Some(&argc) => {
                    ip += 1;
                    format!("CALL       {argc}")
                }
                None => {
                    let line = "CALL       <truncated operand>".to_string();
                    ip = code.len();
                    line
                }
            },
            op if op == Op::Pop as u8 => "POP".to_string(),
            op if op == Op::Halt as u8 => "HALT".to_string(),
            other => format!("<unknown opcode {other}>"),
        };

        let _ = writeln!(out, "{offset:04x}  {line}");

        ip = advance_past(offset, ip);
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

    fn chunk_for(src: &str) -> Chunk {
        let forms = read_program(src).unwrap();
        compile_program(&forms).unwrap()
    }

    #[test]
    fn lists_a_halt_only_program() {
        let listing = disassemble(&chunk_for("1"));
        assert!(listing.contains("HALT"));
    }

    #[test]
    fn names_every_opcode_used_by_a_call_expression() {
        let listing = disassemble(&chunk_for("(display (+ 1 2))"));
        assert!(listing.contains("GET_GLOBAL"), "{listing}");
        assert!(listing.contains("CONST"), "{listing}");
        assert!(listing.contains("CALL"), "{listing}");
        assert!(listing.contains("POP"), "{listing}");
        assert!(listing.contains("HALT"), "{listing}");
    }

    #[test]
    fn annotates_get_global_with_the_resolved_symbol_name() {
        let listing = disassemble(&chunk_for("(newline)"));
        assert!(listing.contains("newline"), "{listing}");
    }

    #[test]
    fn annotates_const_with_the_literal_value() {
        let listing = disassemble(&chunk_for("42"));
        assert!(listing.contains("42"), "{listing}");
    }

    #[test]
    fn is_legible_multi_line_text_not_a_single_opaque_blob() {
        let listing = disassemble(&chunk_for("(display (+ 1 2)) (newline)"));
        assert!(listing.lines().count() > 3);
    }

    #[test]
    fn does_not_panic_on_a_chunk_with_a_truncated_trailing_instruction() {
        use crate::bytecode::Op;
        let mut chunk = Chunk::new();
        chunk.code.push(Op::Const as u8);
        chunk.code.push(0); // only 1 of the required 4 operand bytes
        let listing = disassemble(&chunk); // must return, not panic
        assert!(!listing.is_empty());
    }

    #[test]
    fn resolves_each_const_operand_to_its_own_distinct_index_not_always_the_first() {
        // "(+ 1 2)" has constants [Symbol("+"), Int(1), Int(2)] at indices 0, 1, 2.
        // A disassembler that always resolved index 0 would show "+" for every
        // operand instead of the actual 1 and 2.
        let listing = disassemble(&chunk_for("(+ 1 2)"));
        assert!(listing.contains("; 1"), "{listing}");
        assert!(listing.contains("; 2"), "{listing}");
    }

    #[test]
    fn an_unrecognised_opcode_byte_is_reported_as_unknown_not_mistaken_for_halt() {
        let mut chunk = Chunk::new();
        chunk.code.push(250); // no opcode is numbered 250
        let listing = disassemble(&chunk);
        assert!(listing.contains("unknown opcode"), "{listing}");
        assert!(!listing.contains("HALT"), "{listing}");
    }

    #[test]
    fn never_loops_forever_on_a_pathological_chunk() {
        // A regression guard for the instruction-pointer-advancement invariant:
        // no matter how odd the bytes are, disassemble() must return.
        let chunk = Chunk {
            code: vec![Op::Const as u8, 0, 0, 0, 0, Op::Call as u8, 0, 99, 99, 99],
            constants: vec![],
        };
        let listing = disassemble(&chunk);
        assert!(!listing.is_empty());
    }
}
