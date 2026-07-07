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

pub fn compile_program(forms: &[Sexpr]) -> Result<Chunk, CompileError> {
    let mut chunk = Chunk::new();
    for form in forms {
        compile_expr(form, &mut chunk)?;
        chunk.emit_pop();
    }
    chunk.emit_halt();
    Ok(chunk)
}

fn compile_expr(expr: &Sexpr, chunk: &mut Chunk) -> Result<(), CompileError> {
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
            compile_expr(callee, chunk)?;
            for arg in args {
                compile_expr(arg, chunk)?;
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

    #[test]
    fn compiles_an_int_literal_to_const_then_pop_then_halt() {
        let chunk = compile_program(&[Sexpr::Int(5)]).unwrap();
        assert_eq!(chunk.constants, vec![Const::Int(5)]);
        assert_eq!(
            chunk.code,
            vec![Op::Const as u8, 0, 0, 0, 0, Op::Pop as u8, Op::Halt as u8]
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
        assert_eq!(
            chunk.constants,
            vec![Const::Symbol("+".to_string()), Const::Int(1), Const::Int(2),]
        );
        assert_eq!(
            chunk.code,
            vec![
                Op::GetGlobal as u8,
                0,
                0,
                0,
                0,
                Op::Const as u8,
                1,
                0,
                0,
                0,
                Op::Const as u8,
                2,
                0,
                0,
                0,
                Op::Call as u8,
                2,
                Op::Pop as u8,
                Op::Halt as u8,
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
}
