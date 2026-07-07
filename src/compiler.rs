//! Compiles reader output into bytecode.

use crate::bytecode::{Chunk, Const};
use crate::reader::Sexpr;

#[derive(Debug, Clone, PartialEq)]
pub struct CompileError {
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "compile error: {}", self.message)
    }
}

/// Caps expression-nesting depth so a pathologically deep (but structurally
/// valid) expression tree fails cleanly instead of risking a native stack
/// overflow while compiling.
const MAX_NESTING_DEPTH: usize = 512;

pub fn compile_program(forms: &[Sexpr]) -> Result<Chunk, CompileError> {
    let mut chunk = Chunk::new();
    for form in forms {
        compile_expr(form, &mut chunk, 0)?;
        chunk.emit_pop();
    }
    chunk.emit_halt();
    Ok(chunk)
}

fn compile_expr(expr: &Sexpr, chunk: &mut Chunk, depth: usize) -> Result<(), CompileError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(CompileError {
            message: format!(
                "expression nesting exceeds the maximum supported depth ({MAX_NESTING_DEPTH})"
            ),
        });
    }
    match expr {
        Sexpr::Int(n) => {
            let idx = chunk.add_const(Const::Int(*n));
            chunk.emit_const(idx);
        }
        Sexpr::Bool(b) => {
            let idx = chunk.add_const(Const::Bool(*b));
            chunk.emit_const(idx);
        }
        Sexpr::Str(s) => {
            let idx = chunk.add_const(Const::Str(s.clone()));
            chunk.emit_const(idx);
        }
        Sexpr::Symbol(s) => {
            let idx = chunk.add_const(Const::Symbol(s.clone()));
            chunk.emit_get_global(idx);
        }
        Sexpr::List(items) => {
            let (callee, args) = items.split_first().ok_or_else(|| CompileError {
                message: "cannot call the empty list ()".to_string(),
            })?;
            compile_expr(callee, chunk, depth + 1)?;
            for arg in args {
                compile_expr(arg, chunk, depth + 1)?;
            }
            if args.len() > u8::MAX as usize {
                return Err(CompileError {
                    message: format!("too many arguments in call: {}", args.len()),
                });
            }
            chunk.emit_call(args.len() as u8);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::Op;

    /// Decodes just the sequence of opcodes from a chunk's code, discarding
    /// operand bytes — so tests can assert on the compiler's control-flow
    /// shape (which opcodes, in what order) without coupling to the exact
    /// operand encoding, which is bytecode.rs's concern (see its own
    /// round-trip tests) and not the compiler's observable behavior.
    fn opcode_sequence(code: &[u8]) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut i = 0;
        while i < code.len() {
            let op = code[i];
            i += 1;
            let decoded = if op == Op::Const as u8 {
                i += 4;
                Op::Const
            } else if op == Op::GetGlobal as u8 {
                i += 4;
                Op::GetGlobal
            } else if op == Op::Call as u8 {
                i += 1;
                Op::Call
            } else if op == Op::Pop as u8 {
                Op::Pop
            } else if op == Op::Halt as u8 {
                Op::Halt
            } else {
                panic!("opcode_sequence: unrecognised opcode byte {op}");
            };
            ops.push(decoded);
        }
        ops
    }

    #[test]
    fn compiles_an_int_literal_to_const_then_pop_then_halt() {
        let chunk = compile_program(&[Sexpr::Int(5)]).unwrap();
        assert_eq!(chunk.constants, vec![Const::Int(5)]);
        assert_eq!(
            opcode_sequence(&chunk.code),
            vec![Op::Const, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn compiles_a_call_expression_as_callee_then_args_then_call() {
        let program = [Sexpr::List(vec![
            Sexpr::Symbol("+".to_string()),
            Sexpr::Int(1),
            Sexpr::Int(2),
        ])];
        let chunk = compile_program(&program).unwrap();
        // Constant-pool order is the direct evidence of "callee first, then
        // args in order" — the actual behavior this test cares about.
        assert_eq!(
            chunk.constants,
            vec![Const::Symbol("+".to_string()), Const::Int(1), Const::Int(2)]
        );
        assert_eq!(
            opcode_sequence(&chunk.code),
            vec![
                Op::GetGlobal, // callee
                Op::Const,     // arg 1
                Op::Const,     // arg 2
                Op::Call,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_nested_calls_depth_first() {
        let program = [Sexpr::List(vec![
            Sexpr::Symbol("display".to_string()),
            Sexpr::List(vec![
                Sexpr::Symbol("+".to_string()),
                Sexpr::Int(1),
                Sexpr::Int(2),
            ]),
        ])];
        let chunk = compile_program(&program).unwrap();
        assert_eq!(chunk.constants.len(), 4);
        assert!(chunk.code.contains(&(Op::Call as u8)));
    }

    #[test]
    fn compiles_each_top_level_form_followed_by_its_own_pop() {
        let program = [Sexpr::Int(1), Sexpr::Int(2)];
        let chunk = compile_program(&program).unwrap();
        let pop_count = chunk.code.iter().filter(|&&b| b == Op::Pop as u8).count();
        assert_eq!(pop_count, 2);
        assert_eq!(*chunk.code.last().unwrap(), Op::Halt as u8);
    }

    #[test]
    fn compiles_a_bare_symbol_as_a_global_lookup() {
        let chunk = compile_program(&[Sexpr::Symbol("display".to_string())]).unwrap();
        assert_eq!(chunk.constants, vec![Const::Symbol("display".to_string())]);
        assert_eq!(chunk.code[0], Op::GetGlobal as u8);
    }

    #[test]
    fn compiles_string_and_bool_literals_as_constants() {
        let program = [Sexpr::Str("hi".to_string()), Sexpr::Bool(true)];
        let chunk = compile_program(&program).unwrap();
        assert_eq!(
            chunk.constants,
            vec![Const::Str("hi".to_string()), Const::Bool(true)]
        );
    }

    #[test]
    fn rejects_calling_the_empty_list() {
        let program = [Sexpr::List(vec![])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compile_error_display_includes_the_underlying_message() {
        let e = CompileError {
            message: "something specific went wrong".to_string(),
        };
        assert_eq!(
            e.to_string(),
            "compile error: something specific went wrong"
        );
    }

    fn call_with_n_args(n: usize) -> Sexpr {
        let mut items = vec![Sexpr::Symbol("+".to_string())];
        items.extend((0..n).map(|_| Sexpr::Int(1)));
        Sexpr::List(items)
    }

    #[test]
    fn accepts_a_call_with_exactly_the_maximum_representable_argument_count() {
        let program = [call_with_n_args(u8::MAX as usize)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn rejects_a_call_with_one_more_than_the_maximum_representable_argument_count() {
        let program = [call_with_n_args(u8::MAX as usize + 1)];
        assert!(compile_program(&program).is_err());
    }

    fn nested_call(depth: usize) -> Sexpr {
        let mut expr = Sexpr::Int(1);
        for _ in 0..depth {
            expr = Sexpr::List(vec![Sexpr::Symbol("+".to_string()), expr]);
        }
        expr
    }

    #[test]
    fn rejects_expression_nesting_deeper_than_the_configured_maximum() {
        // Guards against unbounded recursion in compile_expr on a
        // pathologically deep (but structurally valid) expression tree
        // (a security-review finding on B1).
        let program = [nested_call(600)];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn accepts_expression_nesting_comfortably_under_the_configured_maximum() {
        let program = [nested_call(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn accepts_expression_nesting_of_exactly_the_configured_maximum_depth() {
        let program = [nested_call(MAX_NESTING_DEPTH)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn rejects_expression_nesting_of_one_more_than_the_configured_maximum_depth() {
        let program = [nested_call(MAX_NESTING_DEPTH + 1)];
        assert!(compile_program(&program).is_err());
    }

    fn nested_in_callee_position(depth: usize) -> Sexpr {
        // Nests via the callee slot only (never the argument list), to
        // isolate the depth-tracking on that recursive call from the one on
        // the argument-list recursive call.
        let mut expr = Sexpr::Symbol("+".to_string());
        for _ in 0..depth {
            expr = Sexpr::List(vec![expr]);
        }
        expr
    }

    #[test]
    fn tracks_depth_through_the_callee_position_not_just_the_argument_list() {
        let program = [nested_in_callee_position(600)];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn accepts_callee_position_nesting_comfortably_under_the_configured_maximum() {
        let program = [nested_in_callee_position(100)];
        assert!(compile_program(&program).is_ok());
    }
}
