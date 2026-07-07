//! Compiles reader output into bytecode.

use crate::bytecode::{Chunk, Const, Module, Op};
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

fn err(message: impl Into<String>) -> CompileError {
    CompileError {
        message: message.into(),
    }
}

/// Caps expression-nesting depth so a pathologically deep (but structurally
/// valid) expression tree fails cleanly instead of risking a native stack
/// overflow while compiling.
const MAX_NESTING_DEPTH: usize = 512;

fn too_deep() -> CompileError {
    err(format!(
        "expression nesting exceeds the maximum supported depth ({MAX_NESTING_DEPTH})"
    ))
}

/// Local variable names in scope for the function body currently being
/// compiled (its formal parameters, in slot order). Empty at the top level:
/// this language has no closures over enclosing lambdas yet, so every
/// function body sees only its own parameters as locals; any other free
/// symbol resolves as a (possibly not-yet-defined) global.
struct Ctx {
    locals: Vec<String>,
}

impl Ctx {
    fn top_level() -> Self {
        Ctx { locals: Vec::new() }
    }

    fn resolve_local(&self, name: &str) -> Option<u8> {
        self.locals.iter().position(|n| n == name).map(|i| i as u8)
    }
}

enum Formals {
    Fixed(Vec<String>),
    FixedPlusRest(Vec<String>, String),
    AllRest(String),
}

fn expect_symbol_name(sexpr: &Sexpr) -> Result<String, CompileError> {
    match sexpr {
        Sexpr::Symbol(s) => Ok(s.clone()),
        other => Err(err(format!("expected a parameter name, found {other:?}"))),
    }
}

fn parse_formals(sexpr: &Sexpr) -> Result<Formals, CompileError> {
    match sexpr {
        Sexpr::Symbol(s) => Ok(Formals::AllRest(s.clone())),
        Sexpr::List(items) => {
            let names = items
                .iter()
                .map(expect_symbol_name)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Formals::Fixed(names))
        }
        Sexpr::DottedList(items, tail) => {
            let names = items
                .iter()
                .map(expect_symbol_name)
                .collect::<Result<Vec<_>, _>>()?;
            let tail_name = expect_symbol_name(tail)?;
            Ok(Formals::FixedPlusRest(names, tail_name))
        }
        other => Err(err(format!("invalid parameter list: {other:?}"))),
    }
}

fn sexpr_to_const(sexpr: &Sexpr) -> Result<Const, CompileError> {
    Ok(match sexpr {
        Sexpr::Int(n) => Const::Int(*n),
        Sexpr::Bool(b) => Const::Bool(*b),
        Sexpr::Str(s) => Const::Str(s.clone()),
        Sexpr::Symbol(s) => Const::Symbol(s.clone()),
        Sexpr::List(items) => Const::List(
            items
                .iter()
                .map(sexpr_to_const)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Sexpr::DottedList(..) => {
            return Err(err("dotted-pair data cannot be quoted"));
        }
    })
}

pub fn compile_program(forms: &[Sexpr]) -> Result<Module, CompileError> {
    let mut module = Module::default();
    let mut entry = Chunk::new();
    let ctx = Ctx::top_level();
    for form in forms {
        compile_expr(form, &ctx, &mut entry, &mut module, 0)?;
        entry.emit_pop();
    }
    entry.emit_halt();
    module.entry_index = module.functions.len() as u32;
    module.functions.push(entry);
    Ok(module)
}

/// Compiles a body of expressions where all but the last are evaluated for
/// effect and discarded, and the last one's value is left on the stack.
/// Shared by `begin` and function/lambda bodies.
fn compile_sequence(
    exprs: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    module: &mut Module,
    depth: usize,
) -> Result<(), CompileError> {
    let Some((last, rest)) = exprs.split_last() else {
        let idx = chunk.add_const(Const::Unspecified);
        chunk.emit_const(idx);
        return Ok(());
    };
    for e in rest {
        compile_expr(e, ctx, chunk, module, depth)?;
        chunk.emit_pop();
    }
    compile_expr(last, ctx, chunk, module, depth)
}

/// Compiles `formals body...` into a new function chunk appended to
/// `module`'s function table, returning its index. Shared by `lambda` and
/// `define`'s function-definition sugar.
fn compile_function(
    formals_sexpr: &Sexpr,
    body: &[Sexpr],
    module: &mut Module,
    depth: usize,
) -> Result<u32, CompileError> {
    // No separate depth check here: every caller reaches this function only
    // through compile_expr's own check at this same `depth` value first, so
    // a second check here would be unreachable dead code.
    let formals = parse_formals(formals_sexpr)?;
    let (locals, arity, has_rest) = match formals {
        Formals::Fixed(names) => {
            let arity = names.len() as u32;
            (names, arity, false)
        }
        Formals::FixedPlusRest(mut names, rest) => {
            let arity = names.len() as u32;
            names.push(rest);
            (names, arity, true)
        }
        Formals::AllRest(rest) => (vec![rest], 0, true),
    };

    let ctx = Ctx { locals };
    let mut fn_chunk = Chunk::new();
    fn_chunk.arity = arity;
    fn_chunk.has_rest = has_rest;
    compile_sequence(body, &ctx, &mut fn_chunk, module, depth + 1)?;
    fn_chunk.emit_return();

    let index = module.functions.len() as u32;
    module.functions.push(fn_chunk);
    Ok(index)
}

fn compile_quote(items: &[Sexpr], chunk: &mut Chunk) -> Result<(), CompileError> {
    if items.len() != 2 {
        return Err(err("quote requires exactly one datum"));
    }
    let const_value = sexpr_to_const(&items[1])?;
    let idx = chunk.add_const(const_value);
    chunk.emit_const(idx);
    Ok(())
}

fn compile_if(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    module: &mut Module,
    depth: usize,
) -> Result<(), CompileError> {
    if items.len() < 3 || items.len() > 4 {
        return Err(err(
            "if requires a condition and a then-branch, and takes an optional else-branch",
        ));
    }
    let condition = &items[1];
    let then_branch = &items[2];
    let else_branch = items.get(3);

    compile_expr(condition, ctx, chunk, module, depth + 1)?;
    let else_jump = chunk.emit_jump(Op::JumpIfFalse);
    compile_expr(then_branch, ctx, chunk, module, depth + 1)?;
    let end_jump = chunk.emit_jump(Op::Jump);

    chunk.patch_jump(else_jump);
    match else_branch {
        Some(else_expr) => compile_expr(else_expr, ctx, chunk, module, depth + 1)?,
        None => {
            let idx = chunk.add_const(Const::Unspecified);
            chunk.emit_const(idx);
        }
    }
    chunk.patch_jump(end_jump);
    Ok(())
}

fn compile_define(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    module: &mut Module,
    depth: usize,
) -> Result<(), CompileError> {
    let head = items
        .get(1)
        .ok_or_else(|| err("define requires a name or a (name . formals) head"))?;

    match head {
        Sexpr::Symbol(name) => {
            if items.len() != 3 {
                return Err(err(
                    "define with a plain name takes exactly one value expression",
                ));
            }
            compile_expr(&items[2], ctx, chunk, module, depth + 1)?;
            let idx = chunk.add_const(Const::Symbol(name.clone()));
            chunk.emit_def_global(idx);
            Ok(())
        }
        Sexpr::List(head_items) => {
            let (name_sexpr, formal_items) = head_items
                .split_first()
                .ok_or_else(|| err("define's function head cannot be empty"))?;
            let name = expect_symbol_name(name_sexpr)?;
            let formals_sexpr = Sexpr::List(formal_items.to_vec());
            let fn_index = compile_function(&formals_sexpr, &items[2..], module, depth)?;
            chunk.emit_make_function(fn_index);
            let idx = chunk.add_const(Const::Symbol(name));
            chunk.emit_def_global(idx);
            Ok(())
        }
        Sexpr::DottedList(head_items, tail) => {
            let (name_sexpr, formal_items) = head_items
                .split_first()
                .ok_or_else(|| err("define's function head cannot be empty"))?;
            let name = expect_symbol_name(name_sexpr)?;
            let formals_sexpr = if formal_items.is_empty() {
                (**tail).clone()
            } else {
                Sexpr::DottedList(formal_items.to_vec(), tail.clone())
            };
            let fn_index = compile_function(&formals_sexpr, &items[2..], module, depth)?;
            chunk.emit_make_function(fn_index);
            let idx = chunk.add_const(Const::Symbol(name));
            chunk.emit_def_global(idx);
            Ok(())
        }
        other => Err(err(format!("invalid define head: {other:?}"))),
    }
}

fn compile_lambda(
    items: &[Sexpr],
    chunk: &mut Chunk,
    module: &mut Module,
    depth: usize,
) -> Result<(), CompileError> {
    let formals_sexpr = items
        .get(1)
        .ok_or_else(|| err("lambda requires a parameter list"))?;
    let fn_index = compile_function(formals_sexpr, &items[2..], module, depth)?;
    chunk.emit_make_function(fn_index);
    Ok(())
}

fn compile_expr(
    expr: &Sexpr,
    ctx: &Ctx,
    chunk: &mut Chunk,
    module: &mut Module,
    depth: usize,
) -> Result<(), CompileError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(too_deep());
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
            if let Some(slot) = ctx.resolve_local(s) {
                chunk.emit_get_local(slot);
            } else {
                let idx = chunk.add_const(Const::Symbol(s.clone()));
                chunk.emit_get_global(idx);
            }
        }
        Sexpr::DottedList(..) => {
            return Err(err("dotted-pair syntax is only valid in a parameter list"));
        }
        Sexpr::List(items) => {
            if let Some(Sexpr::Symbol(op)) = items.first() {
                match op.as_str() {
                    "quote" => return compile_quote(items, chunk),
                    "if" => return compile_if(items, ctx, chunk, module, depth),
                    "define" => return compile_define(items, ctx, chunk, module, depth),
                    "lambda" => return compile_lambda(items, chunk, module, depth),
                    "begin" => return compile_sequence(&items[1..], ctx, chunk, module, depth + 1),
                    _ => {}
                }
            }
            let (callee, args) = items
                .split_first()
                .ok_or_else(|| err("cannot call the empty list ()"))?;
            compile_expr(callee, ctx, chunk, module, depth + 1)?;
            for arg in args {
                compile_expr(arg, ctx, chunk, module, depth + 1)?;
            }
            if args.len() > u8::MAX as usize {
                return Err(err(format!("too many arguments in call: {}", args.len())));
            }
            chunk.emit_call(args.len() as u8);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            } else if op == Op::DefGlobal as u8 {
                i += 4;
                Op::DefGlobal
            } else if op == Op::GetLocal as u8 {
                i += 1;
                Op::GetLocal
            } else if op == Op::MakeFunction as u8 {
                i += 4;
                Op::MakeFunction
            } else if op == Op::Jump as u8 {
                i += 4;
                Op::Jump
            } else if op == Op::JumpIfFalse as u8 {
                i += 4;
                Op::JumpIfFalse
            } else if op == Op::Call as u8 {
                i += 1;
                Op::Call
            } else if op == Op::Pop as u8 {
                Op::Pop
            } else if op == Op::Return as u8 {
                Op::Return
            } else if op == Op::Halt as u8 {
                Op::Halt
            } else {
                panic!("opcode_sequence: unrecognised opcode byte {op}");
            };
            ops.push(decoded);
        }
        ops
    }

    fn sym(s: &str) -> Sexpr {
        Sexpr::Symbol(s.to_string())
    }

    fn list(items: Vec<Sexpr>) -> Sexpr {
        Sexpr::List(items)
    }

    #[test]
    fn compiles_an_int_literal_to_const_then_pop_then_halt() {
        let module = compile_program(&[Sexpr::Int(5)]).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(entry.constants, vec![Const::Int(5)]);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn compiles_a_call_expression_as_callee_then_args_then_call() {
        let program = [list(vec![sym("+"), Sexpr::Int(1), Sexpr::Int(2)])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(
            entry.constants,
            vec![Const::Symbol("+".to_string()), Const::Int(1), Const::Int(2)]
        );
        assert_eq!(
            opcode_sequence(&entry.code),
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
        let program = [list(vec![
            sym("display"),
            list(vec![sym("+"), Sexpr::Int(1), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(entry.constants.len(), 4);
        assert!(entry.code.contains(&(Op::Call as u8)));
    }

    #[test]
    fn compiles_each_top_level_form_followed_by_its_own_pop() {
        let program = [Sexpr::Int(1), Sexpr::Int(2)];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        let pop_count = entry.code.iter().filter(|&&b| b == Op::Pop as u8).count();
        assert_eq!(pop_count, 2);
        assert_eq!(*entry.code.last().unwrap(), Op::Halt as u8);
    }

    #[test]
    fn compiles_a_bare_symbol_as_a_global_lookup() {
        let module = compile_program(&[sym("display")]).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(entry.constants, vec![Const::Symbol("display".to_string())]);
        assert_eq!(entry.code[0], Op::GetGlobal as u8);
    }

    #[test]
    fn compiles_string_and_bool_literals_as_constants() {
        let program = [Sexpr::Str("hi".to_string()), Sexpr::Bool(true)];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(
            entry.constants,
            vec![Const::Str("hi".to_string()), Const::Bool(true)]
        );
    }

    #[test]
    fn rejects_calling_the_empty_list() {
        let program = [list(vec![])];
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
        let mut items = vec![sym("+")];
        items.extend((0..n).map(|_| Sexpr::Int(1)));
        list(items)
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
            expr = list(vec![sym("+"), expr]);
        }
        expr
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
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("nesting") && error.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            error.message
        );
    }

    fn nested_in_callee_position(depth: usize) -> Sexpr {
        let mut expr = sym("+");
        for _ in 0..depth {
            expr = list(vec![expr]);
        }
        expr
    }

    #[test]
    fn accepts_callee_position_nesting_comfortably_under_the_configured_maximum() {
        let program = [nested_in_callee_position(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn accepts_callee_position_nesting_of_exactly_the_configured_maximum_depth() {
        let program = [nested_in_callee_position(MAX_NESTING_DEPTH)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn rejects_callee_position_nesting_of_one_more_than_the_configured_maximum_depth() {
        let program = [nested_in_callee_position(MAX_NESTING_DEPTH + 1)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("nesting") && error.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            error.message
        );
    }

    #[test]
    fn nesting_depth_inside_a_function_body_starts_one_deeper_than_top_level() {
        // The lambda itself is one level of nesting: a body expression that
        // would exactly reach the top-level limit on its own is now one over,
        // and must error, proving the body is compiled at depth+1, not depth.
        let body = nested_call(MAX_NESTING_DEPTH);
        let program = [list(vec![sym("lambda"), list(vec![]), body])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn if_condition_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("if"),
            nested_call(MAX_NESTING_DEPTH),
            Sexpr::Int(1),
            Sexpr::Int(2),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn if_then_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(true),
            nested_call(MAX_NESTING_DEPTH),
            Sexpr::Int(2),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn if_else_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(false),
            Sexpr::Int(1),
            nested_call(MAX_NESTING_DEPTH),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn define_value_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("define"),
            sym("x"),
            nested_call(MAX_NESTING_DEPTH),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn begin_propagates_nesting_depth_to_its_expressions() {
        let program = [list(vec![sym("begin"), nested_call(MAX_NESTING_DEPTH)])];
        assert!(compile_program(&program).is_err());
    }

    // --- B2 special forms ---

    #[test]
    fn compiles_quote_as_a_constant_push_with_no_evaluation() {
        let program = [list(vec![
            sym("quote"),
            list(vec![sym("+"), Sexpr::Int(1), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::Pop, Op::Halt]
        );
        assert_eq!(
            entry.constants,
            vec![Const::List(vec![
                Const::Symbol("+".to_string()),
                Const::Int(1),
                Const::Int(2),
            ])]
        );
    }

    #[test]
    fn compiles_if_with_else_as_condition_then_conditional_jump_then_both_branches() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(true),
            Sexpr::Int(1),
            Sexpr::Int(2),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::Const,       // condition
                Op::JumpIfFalse, // to else
                Op::Const,       // then
                Op::Jump,        // to end
                Op::Const,       // else
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_if_without_else_to_push_unspecified_on_the_false_path() {
        let program = [list(vec![sym("if"), Sexpr::Bool(false), Sexpr::Int(1)])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert!(entry.constants.contains(&Const::Unspecified));
    }

    #[test]
    fn rejects_if_with_too_few_arguments() {
        let program = [list(vec![sym("if"), Sexpr::Bool(true)])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_if_with_too_many_arguments() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(true),
            Sexpr::Int(1),
            Sexpr::Int(2),
            Sexpr::Int(3),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compiles_a_plain_define_as_value_then_def_global() {
        let program = [list(vec![sym("define"), sym("x"), Sexpr::Int(42)])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::DefGlobal, Op::Pop, Op::Halt]
        );
        assert!(entry.constants.contains(&Const::Symbol("x".to_string())));
    }

    #[test]
    fn compiles_a_fixed_arity_function_define() {
        // (define (add a b) (+ a b))
        let program = [list(vec![
            sym("define"),
            list(vec![sym("add"), sym("a"), sym("b")]),
            list(vec![sym("+"), sym("a"), sym("b")]),
        ])];
        let module = compile_program(&program).unwrap();
        // module.functions: [add's body chunk, entry chunk]
        assert_eq!(module.functions.len(), 2);
        let fn_chunk = &module.functions[0];
        assert_eq!(fn_chunk.arity, 2);
        assert!(!fn_chunk.has_rest);
        // both locals resolved via GET_LOCAL, not GET_GLOBAL
        assert_eq!(
            opcode_sequence(&fn_chunk.code),
            vec![
                Op::GetGlobal,
                Op::GetLocal,
                Op::GetLocal,
                Op::Call,
                Op::Return
            ]
        );
    }

    #[test]
    fn compiles_a_fixed_plus_rest_function_define() {
        // (define (f a b . rest) rest)
        let program = [list(vec![
            sym("define"),
            Sexpr::DottedList(vec![sym("f"), sym("a"), sym("b")], Box::new(sym("rest"))),
            sym("rest"),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        assert_eq!(fn_chunk.arity, 2);
        assert!(fn_chunk.has_rest);
        assert_eq!(
            opcode_sequence(&fn_chunk.code),
            vec![Op::GetLocal, Op::Return]
        );
    }

    #[test]
    fn compiles_an_all_rest_function_define() {
        // (define (f . args) args)
        let program = [list(vec![
            sym("define"),
            Sexpr::DottedList(vec![sym("f")], Box::new(sym("args"))),
            sym("args"),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        assert_eq!(fn_chunk.arity, 0);
        assert!(fn_chunk.has_rest);
    }

    #[test]
    fn compiles_lambda_to_make_function_referencing_a_new_chunk() {
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("x")]),
            list(vec![sym("+"), sym("x"), Sexpr::Int(1)]),
        ])];
        let module = compile_program(&program).unwrap();
        assert_eq!(module.functions.len(), 2); // lambda body + entry
        let entry = &module.functions[module.entry_index as usize];
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::MakeFunction, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn compiles_begin_running_each_expression_and_keeping_only_the_last_value() {
        let program = [list(vec![
            sym("begin"),
            Sexpr::Int(1),
            Sexpr::Int(2),
            Sexpr::Int(3),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = &module.functions[module.entry_index as usize];
        let pop_count = entry.code.iter().filter(|&&b| b == Op::Pop as u8).count();
        // 2 pops inside begin (for 1 and 2) + 1 pop for the top-level form's own result
        assert_eq!(pop_count, 3);
    }

    #[test]
    fn rejects_a_lambda_with_a_malformed_parameter_list() {
        let program = [list(vec![
            sym("lambda"),
            Sexpr::Int(5), // not a symbol, list, or dotted list
            Sexpr::Int(1),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_dotted_pair_syntax_outside_a_parameter_list() {
        let program = [Sexpr::DottedList(vec![sym("a")], Box::new(sym("b")))];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_quoting_dotted_pair_data() {
        let program = [list(vec![
            sym("quote"),
            Sexpr::DottedList(vec![sym("a")], Box::new(sym("b"))),
        ])];
        assert!(compile_program(&program).is_err());
    }
}
