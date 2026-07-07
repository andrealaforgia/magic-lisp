//! Bytecode virtual machine.

use std::collections::HashMap;
use std::io::Write;

use crate::bytecode::{Chunk, Const, Module, Op};
use crate::value::{Value, is_truthy};

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError {
    pub message: String,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runtime error: {}", self.message)
    }
}

fn error(message: impl Into<String>) -> RuntimeError {
    RuntimeError {
        message: message.into(),
    }
}

const NATIVE_NAMES: [&str; 10] = [
    "display", "newline", "+", "-", "*", "=", "<", "<=", ">", ">=",
];

pub fn default_globals() -> HashMap<String, Value> {
    NATIVE_NAMES
        .iter()
        .map(|&name| (name.to_string(), Value::Native(name.to_string())))
        .collect()
}

fn const_to_value(c: &Const) -> Value {
    match c {
        Const::Int(n) => Value::Int(*n),
        Const::Bool(b) => Value::Bool(*b),
        Const::Str(s) => Value::Str(s.clone()),
        Const::Symbol(s) => Value::Symbol(s.clone()),
        Const::List(items) => Value::List(items.iter().map(const_to_value).collect()),
        Const::Unspecified => Value::Unspecified,
    }
}

fn read_u32(code: &[u8], ip: &mut usize) -> Result<u32, RuntimeError> {
    let bytes = code
        .get(*ip..*ip + 4)
        .ok_or_else(|| error("truncated instruction operand"))?;
    *ip += 4;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_u8(code: &[u8], ip: &mut usize) -> Result<u8, RuntimeError> {
    let byte = *code
        .get(*ip)
        .ok_or_else(|| error("truncated instruction operand"))?;
    *ip += 1;
    Ok(byte)
}

fn constant_at(chunk: &Chunk, idx: u32) -> Result<&Const, RuntimeError> {
    chunk
        .constants
        .get(idx as usize)
        .ok_or_else(|| error(format!("constant index {idx} out of range")))
}

/// Consumes one unit of step budget, saturating at zero rather than
/// underflowing, so the budget can only ever shrink toward exhaustion.
fn consume_step(remaining: usize) -> usize {
    remaining.saturating_sub(1)
}

/// The total instruction-step budget for a chunk of `code_len` bytes: every
/// instruction is at least 1 byte, so no correct program needs more than
/// `code_len` steps; the `+ 1` is a one-step margin.
fn step_budget_for(code_len: usize) -> usize {
    code_len + 1
}

fn symbol_name(c: &Const) -> Result<String, RuntimeError> {
    match c {
        Const::Symbol(s) => Ok(s.clone()),
        other => Err(error(format!(
            "expected a symbol constant, found {other:?}"
        ))),
    }
}

struct Vm<'m> {
    module: &'m Module,
    globals: HashMap<String, Value>,
}

impl<'m> Vm<'m> {
    fn exec(
        &mut self,
        chunk: &Chunk,
        locals: &mut Vec<Value>,
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        let mut stack: Vec<Value> = Vec::new();
        let code = &chunk.code;
        let mut ip = 0usize;

        // Every instruction is at least 1 byte and (for this language, which
        // has no backward jumps yet) ip only ever moves forward within a
        // single exec() call, so no correct program executes more than
        // code.len() instructions here. This bounds total loop iterations
        // independently of ip's own bookkeeping, so a broken operand-advance
        // can never hang the interpreter — it fails cleanly instead.
        let mut remaining_steps = step_budget_for(code.len());

        loop {
            if remaining_steps == 0 {
                return Err(error(
                    "exceeded the maximum instruction step budget (possible decoder bug)",
                ));
            }
            remaining_steps = consume_step(remaining_steps);

            let opcode = *code
                .get(ip)
                .ok_or_else(|| error("ran off the end of the instruction stream"))?;
            ip += 1;

            match opcode {
                op if op == Op::Const as u8 => {
                    let idx = read_u32(code, &mut ip)?;
                    stack.push(const_to_value(constant_at(chunk, idx)?));
                }
                op if op == Op::GetGlobal as u8 => {
                    let idx = read_u32(code, &mut ip)?;
                    let name = symbol_name(constant_at(chunk, idx)?)?;
                    let value = self
                        .globals
                        .get(&name)
                        .cloned()
                        .ok_or_else(|| error(format!("unbound global: {name}")))?;
                    stack.push(value);
                }
                op if op == Op::DefGlobal as u8 => {
                    let idx = read_u32(code, &mut ip)?;
                    let name = symbol_name(constant_at(chunk, idx)?)?;
                    let value = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during DEF_GLOBAL"))?;
                    self.globals.insert(name, value);
                    stack.push(Value::Unspecified);
                }
                op if op == Op::GetLocal as u8 => {
                    let slot = read_u8(code, &mut ip)? as usize;
                    let value = locals
                        .get(slot)
                        .cloned()
                        .ok_or_else(|| error(format!("local slot {slot} out of range")))?;
                    stack.push(value);
                }
                op if op == Op::SetLocal as u8 => {
                    let slot = read_u8(code, &mut ip)? as usize;
                    let value = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during SET_LOCAL"))?;
                    let target = locals
                        .get_mut(slot)
                        .ok_or_else(|| error(format!("local slot {slot} out of range")))?;
                    *target = value;
                    stack.push(Value::Unspecified);
                }
                op if op == Op::SetGlobal as u8 => {
                    let idx = read_u32(code, &mut ip)?;
                    let name = symbol_name(constant_at(chunk, idx)?)?;
                    let value = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during SET_GLOBAL"))?;
                    if !self.globals.contains_key(&name) {
                        return Err(error(format!("cannot set! undefined variable: {name}")));
                    }
                    self.globals.insert(name, value);
                    stack.push(Value::Unspecified);
                }
                op if op == Op::PushLocal as u8 => {
                    let value = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during PUSH_LOCAL"))?;
                    locals.push(value);
                }
                op if op == Op::Dup as u8 => {
                    let value = stack
                        .last()
                        .cloned()
                        .ok_or_else(|| error("stack underflow during DUP"))?;
                    stack.push(value);
                }
                op if op == Op::Swap as u8 => {
                    let len = stack.len();
                    if len < 2 {
                        return Err(error("stack underflow during SWAP"));
                    }
                    stack.swap(len - 1, len - 2);
                }
                op if op == Op::Eqv as u8 => {
                    let b = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during EQV"))?;
                    let a = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during EQV"))?;
                    stack.push(Value::Bool(a == b));
                }
                op if op == Op::MakeFunction as u8 => {
                    let idx = read_u32(code, &mut ip)?;
                    if idx as usize >= self.module.functions.len() {
                        return Err(error(format!("function index {idx} out of range")));
                    }
                    stack.push(Value::Function(idx));
                }
                op if op == Op::Jump as u8 => {
                    let target = read_u32(code, &mut ip)? as usize;
                    ip = target;
                }
                op if op == Op::JumpIfFalse as u8 => {
                    let target = read_u32(code, &mut ip)? as usize;
                    let cond = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during JUMP_IF_FALSE"))?;
                    if !is_truthy(&cond) {
                        ip = target;
                    }
                }
                op if op == Op::Call as u8 => {
                    let argc = read_u8(code, &mut ip)? as usize;
                    if stack.len() < argc + 1 {
                        return Err(error("stack underflow during CALL"));
                    }
                    let args = stack.split_off(stack.len() - argc);
                    let callee = stack.pop().unwrap();
                    let result = self.call_value(&callee, args, out)?;
                    stack.push(result);
                }
                op if op == Op::Pop as u8 => {
                    stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during POP"))?;
                }
                op if op == Op::Return as u8 => {
                    return Ok(stack.pop().unwrap_or(Value::Unspecified));
                }
                op if op == Op::Halt as u8 => {
                    out.flush().map_err(|e| error(e.to_string()))?;
                    return Ok(Value::Unspecified);
                }
                other => return Err(error(format!("undefined opcode: {other}"))),
            }
        }
    }

    fn call_value(
        &mut self,
        callee: &Value,
        args: Vec<Value>,
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::Native(name) => call_native(name, &args, out),
            Value::Function(idx) => {
                let chunk = self
                    .module
                    .functions
                    .get(*idx as usize)
                    .ok_or_else(|| error(format!("function index {idx} out of range")))?;
                let mut locals = bind_arguments(chunk, args)?;
                self.exec(chunk, &mut locals, out)
            }
            other => Err(error(format!("cannot call a non-procedure value: {other}"))),
        }
    }
}

fn bind_arguments(chunk: &Chunk, mut args: Vec<Value>) -> Result<Vec<Value>, RuntimeError> {
    let arity = chunk.arity as usize;
    if chunk.has_rest {
        if args.len() < arity {
            return Err(error(format!(
                "expected at least {arity} argument(s), got {}",
                args.len()
            )));
        }
        let rest = args.split_off(arity);
        let mut locals = args;
        locals.push(Value::List(rest));
        Ok(locals)
    } else {
        if args.len() != arity {
            return Err(error(format!(
                "expected exactly {arity} argument(s), got {}",
                args.len()
            )));
        }
        Ok(args)
    }
}

pub fn run(module: &Module, out: &mut impl Write) -> Result<(), RuntimeError> {
    let mut vm = Vm {
        module,
        globals: default_globals(),
    };
    let entry = module
        .functions
        .get(module.entry_index as usize)
        .ok_or_else(|| error("entry function index out of range"))?;
    let mut locals = Vec::new();
    vm.exec(entry, &mut locals, out)?;
    Ok(())
}

fn call_native(name: &str, args: &[Value], out: &mut impl Write) -> Result<Value, RuntimeError> {
    match name {
        "display" => {
            let value = args
                .first()
                .ok_or_else(|| error("display expects exactly 1 argument"))?;
            write!(out, "{value}").map_err(|e| error(e.to_string()))?;
            Ok(Value::Unspecified)
        }
        "newline" => {
            writeln!(out).map_err(|e| error(e.to_string()))?;
            Ok(Value::Unspecified)
        }
        "+" => native_plus(args),
        "-" => native_minus(args),
        "*" => native_times(args),
        "=" => native_compare("=", args, |a, b| a == b),
        "<" => native_compare("<", args, |a, b| a < b),
        "<=" => native_compare("<=", args, |a, b| a <= b),
        ">" => native_compare(">", args, |a, b| a > b),
        ">=" => native_compare(">=", args, |a, b| a >= b),
        other => Err(error(format!("unknown native procedure: {other}"))),
    }
}

fn to_ints(opname: &str, args: &[Value]) -> Result<Vec<i64>, RuntimeError> {
    args.iter()
        .map(|v| match v {
            Value::Int(n) => Ok(*n),
            other => Err(error(format!(
                "{opname} expects integer arguments, found {other}"
            ))),
        })
        .collect()
}

fn native_plus(args: &[Value]) -> Result<Value, RuntimeError> {
    let ints = to_ints("+", args)?;
    Ok(Value::Int(
        ints.iter().fold(0i64, |acc, n| acc.wrapping_add(*n)),
    ))
}

fn native_minus(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(error("- requires at least 2 arguments"));
    }
    let ints = to_ints("-", args)?;
    let (first, rest) = ints.split_first().unwrap();
    Ok(Value::Int(
        rest.iter().fold(*first, |acc, n| acc.wrapping_sub(*n)),
    ))
}

fn native_times(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(error("* requires at least 2 arguments"));
    }
    let ints = to_ints("*", args)?;
    Ok(Value::Int(
        ints.iter().fold(1i64, |acc, n| acc.wrapping_mul(*n)),
    ))
}

fn native_compare(
    opname: &str,
    args: &[Value],
    holds: fn(i64, i64) -> bool,
) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(error(format!("{opname} requires at least 2 arguments")));
    }
    let ints = to_ints(opname, args)?;
    let all_hold = ints.windows(2).all(|pair| holds(pair[0], pair[1]));
    Ok(Value::Bool(all_hold))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_program;
    use crate::reader::read_program;

    #[test]
    fn consume_step_decrements_the_remaining_budget_by_exactly_one() {
        assert_eq!(consume_step(5), 4);
        assert_eq!(consume_step(1), 0);
    }

    #[test]
    fn consume_step_saturates_at_zero_instead_of_underflowing() {
        assert_eq!(consume_step(0), 0);
    }

    #[test]
    fn step_budget_is_code_length_plus_a_one_step_margin() {
        assert_eq!(step_budget_for(0), 1);
        assert_eq!(step_budget_for(10), 11);
    }

    fn eval(src: &str) -> Result<String, RuntimeError> {
        let forms = read_program(src).expect("valid source for this test");
        let module = compile_program(&forms).expect("compilable source for this test");
        let mut out = Vec::new();
        run(&module, &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    fn module_of(chunk: Chunk) -> Module {
        Module {
            entry_index: 0,
            functions: vec![chunk],
        }
    }

    #[test]
    fn displays_the_sum_of_two_integers_followed_by_a_newline() {
        assert_eq!(eval("(display (+ 1 2)) (newline)").unwrap(), "3\n");
    }

    #[test]
    fn plus_with_zero_arguments_is_zero() {
        assert_eq!(eval("(display (+))").unwrap(), "0");
    }

    #[test]
    fn plus_with_one_argument_is_that_argument() {
        assert_eq!(eval("(display (+ 5))").unwrap(), "5");
    }

    #[test]
    fn plus_with_more_than_two_arguments_sums_them_all() {
        assert_eq!(eval("(display (+ 1 2 3 4))").unwrap(), "10");
    }

    #[test]
    fn several_displays_and_newlines_appear_in_order_and_are_fully_flushed() {
        let src = "(display 1) (newline) (display 2) (newline) (display 3) (newline)";
        assert_eq!(eval(src).unwrap(), "1\n2\n3\n");
    }

    #[test]
    fn calling_an_unbound_global_is_a_runtime_error() {
        assert!(eval("(this-is-not-defined 1 2)").is_err());
    }

    #[test]
    fn calling_a_non_procedure_value_is_a_runtime_error() {
        // 1 is pushed as the callee position via a hand-built chunk below,
        // since the reader/compiler never produce this shape from source text.
        use crate::bytecode::Const;
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_call(0);
        chunk.emit_pop();
        chunk.emit_halt();
        let mut out = Vec::new();
        assert!(run(&module_of(chunk), &mut out).is_err());
    }

    #[test]
    fn an_undefined_opcode_is_a_runtime_error_not_a_panic() {
        let mut chunk = Chunk::new();
        chunk.code.push(254); // no opcode is numbered 254
        let mut out = Vec::new();
        assert!(run(&module_of(chunk), &mut out).is_err());
    }

    #[test]
    fn an_out_of_range_constant_index_is_a_runtime_error_not_a_panic() {
        let mut chunk = Chunk::new();
        chunk.code.push(Op::Const as u8);
        chunk.code.extend_from_slice(&99u32.to_le_bytes());
        let mut out = Vec::new();
        assert!(run(&module_of(chunk), &mut out).is_err());
    }

    #[test]
    fn plus_rejects_non_integer_arguments() {
        assert!(eval(r#"(display (+ 1 "two"))"#).is_err());
    }

    #[test]
    fn runtime_error_display_includes_the_underlying_message() {
        let e = RuntimeError {
            message: "something specific went wrong".to_string(),
        };
        assert_eq!(
            e.to_string(),
            "runtime error: something specific went wrong"
        );
    }

    #[test]
    fn a_call_with_exactly_argc_items_and_no_callee_underneath_is_a_clean_error_not_a_panic() {
        // Stack holds exactly 1 value (meant as the sole argument) with nothing
        // beneath it to serve as the callee: CALL 1 must fail cleanly, not
        // panic trying to pop a callee that isn't there.
        use crate::bytecode::Const;
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_call(1);
        chunk.emit_halt();
        let mut out = Vec::new();
        assert!(run(&module_of(chunk), &mut out).is_err());
    }

    #[test]
    fn minus_with_two_arguments_subtracts() {
        assert_eq!(eval("(display (- 5 3))").unwrap(), "2");
    }

    #[test]
    fn minus_with_more_than_two_arguments_subtracts_cumulatively_left_to_right() {
        assert_eq!(eval("(display (- 10 1 2 3))").unwrap(), "4");
    }

    #[test]
    fn minus_with_fewer_than_two_arguments_is_a_runtime_error() {
        assert!(eval("(display (- 5))").is_err());
    }

    #[test]
    fn times_with_two_arguments_multiplies() {
        assert_eq!(eval("(display (* 3 4))").unwrap(), "12");
    }

    #[test]
    fn times_with_more_than_two_arguments_multiplies_them_all() {
        assert_eq!(eval("(display (* 1 2 3 4))").unwrap(), "24");
    }

    #[test]
    fn times_with_fewer_than_two_arguments_is_a_runtime_error() {
        assert!(eval("(display (* 5))").is_err());
    }

    #[test]
    fn less_than_is_true_for_a_strictly_increasing_chain() {
        assert_eq!(eval("(display (< 1 2 3))").unwrap(), "#t");
    }

    #[test]
    fn less_than_is_false_when_only_the_endpoints_would_satisfy_it() {
        // A naive endpoints-only check (1 < 2) would wrongly say true;
        // the middle pair (3, 2) breaks the chain.
        assert_eq!(eval("(display (< 1 3 2))").unwrap(), "#f");
    }

    #[test]
    fn comparisons_require_at_least_two_arguments() {
        assert!(eval("(display (< 1))").is_err());
        assert!(eval("(display (= 1))").is_err());
    }

    #[test]
    fn equals_checks_the_whole_chain_not_just_adjacent_pairs() {
        assert_eq!(eval("(display (= 2 2 2))").unwrap(), "#t");
        assert_eq!(eval("(display (= 2 2 3))").unwrap(), "#f");
    }

    #[test]
    fn plus_wraps_on_overflow_instead_of_erroring() {
        assert_eq!(
            eval(&format!("(display (+ {} 1))", i64::MAX)).unwrap(),
            i64::MAX.wrapping_add(1).to_string()
        );
    }

    #[test]
    fn times_wraps_on_overflow_instead_of_erroring() {
        assert_eq!(
            eval(&format!("(display (* {} 2))", i64::MAX)).unwrap(),
            i64::MAX.wrapping_mul(2).to_string()
        );
    }

    #[test]
    fn less_than_or_equal_holds_for_equal_and_increasing_values() {
        assert_eq!(eval("(display (<= 1 1 2))").unwrap(), "#t");
        assert_eq!(eval("(display (<= 2 1))").unwrap(), "#f");
    }

    #[test]
    fn greater_than_holds_for_a_strictly_decreasing_chain() {
        assert_eq!(eval("(display (> 3 2 1))").unwrap(), "#t");
        assert_eq!(eval("(display (> 1 2))").unwrap(), "#f");
    }

    #[test]
    fn greater_than_is_strict_and_rejects_equal_values() {
        // Distinguishes > from >=: equal adjacent values must not satisfy >.
        assert_eq!(eval("(display (> 2 2))").unwrap(), "#f");
    }

    #[test]
    fn greater_than_or_equal_holds_for_equal_and_decreasing_values() {
        assert_eq!(eval("(display (>= 2 2 1))").unwrap(), "#t");
        assert_eq!(eval("(display (>= 1 2))").unwrap(), "#f");
    }

    #[test]
    fn a_fixed_plus_rest_function_called_with_exactly_the_fixed_count_gets_an_empty_rest_list() {
        assert_eq!(
            eval("(define (f a b . rest) rest) (display (f 1 2))").unwrap(),
            "()"
        );
    }

    #[test]
    fn a_fixed_plus_rest_function_called_with_fewer_than_the_fixed_count_is_a_runtime_error() {
        assert!(eval("(define (f a b . rest) rest) (display (f 1))").is_err());
    }

    fn run_to_string(chunk: Chunk) -> Result<String, RuntimeError> {
        let mut out = Vec::new();
        run(&module_of(chunk), &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn push_local_appends_a_new_local_slot_readable_via_get_local() {
        let mut chunk = Chunk::new();
        let five = chunk.add_const(Const::Int(5));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        chunk.emit_const(five);
        chunk.emit_push_local();
        chunk.emit_get_global(display_sym);
        chunk.emit_get_local(0);
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "5");
    }

    #[test]
    fn set_local_overwrites_an_existing_local_slot() {
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        let two = chunk.add_const(Const::Int(2));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        chunk.emit_const(one);
        chunk.emit_push_local();
        chunk.emit_const(two);
        chunk.emit_set_local(0);
        chunk.emit_pop(); // discard set!'s Unspecified result
        chunk.emit_get_global(display_sym);
        chunk.emit_get_local(0);
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "2");
    }

    #[test]
    fn set_global_on_an_undefined_name_is_a_runtime_error() {
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        let name = chunk.add_const(Const::Symbol("never-defined".to_string()));
        chunk.emit_const(one);
        chunk.emit_set_global(name);
        chunk.emit_pop();
        chunk.emit_halt();
        assert!(run_to_string(chunk).is_err());
    }

    #[test]
    fn set_global_on_a_defined_name_updates_it() {
        let mut chunk = Chunk::new();
        let zero = chunk.add_const(Const::Int(0));
        let one = chunk.add_const(Const::Int(1));
        let x = chunk.add_const(Const::Symbol("x".to_string()));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        chunk.emit_const(zero);
        chunk.emit_def_global(x);
        chunk.emit_pop();
        chunk.emit_const(one);
        chunk.emit_set_global(x);
        chunk.emit_pop();
        chunk.emit_get_global(display_sym);
        chunk.emit_get_global(x);
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "1");
    }

    #[test]
    fn dup_duplicates_the_top_of_stack() {
        let mut chunk = Chunk::new();
        let seven = chunk.add_const(Const::Int(7));
        let plus = chunk.add_const(Const::Symbol("+".to_string()));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        chunk.emit_get_global(display_sym);
        chunk.emit_get_global(plus);
        chunk.emit_const(seven);
        chunk.emit_dup();
        chunk.emit_call(2);
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "14");
    }

    #[test]
    fn swap_exchanges_the_top_two_stack_values() {
        // A distractor is pushed first so the stack holds 5 items at the
        // point of the swap (not 4): at 4 items, `len - 2` and `len / 2`
        // both happen to equal 2, so a mutation of one into the other would
        // go undetected. At 5, they diverge (3 vs 2), and — since a wrong
        // index there would swap the callee itself out of position — a
        // mutant makes this whole program fail to run rather than just
        // computing the wrong number.
        let mut chunk = Chunk::new();
        let distractor = chunk.add_const(Const::Int(999));
        let minus = chunk.add_const(Const::Symbol("-".to_string()));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        let one = chunk.add_const(Const::Int(1));
        let ten = chunk.add_const(Const::Int(10));
        chunk.emit_const(distractor);
        chunk.emit_get_global(display_sym);
        chunk.emit_get_global(minus);
        chunk.emit_const(one);
        chunk.emit_const(ten);
        chunk.emit_swap(); // stack: [999, display, minus, 10, 1] -> (- 10 1)
        chunk.emit_call(2);
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_pop(); // discard the distractor
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "9");
    }

    #[test]
    fn swap_with_exactly_two_stack_values_succeeds() {
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        let two = chunk.add_const(Const::Int(2));
        chunk.emit_const(one);
        chunk.emit_const(two);
        chunk.emit_swap();
        chunk.emit_pop();
        chunk.emit_pop();
        chunk.emit_halt();
        assert!(run_to_string(chunk).is_ok());
    }

    #[test]
    fn swap_with_fewer_than_two_stack_values_is_a_runtime_error_not_a_panic() {
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_swap();
        chunk.emit_pop();
        chunk.emit_halt();
        assert!(run_to_string(chunk).is_err());
    }

    #[test]
    fn eqv_compares_values_structurally() {
        let mut chunk = Chunk::new();
        let a = chunk.add_const(Const::Int(3));
        let b = chunk.add_const(Const::Int(3));
        let c = chunk.add_const(Const::Int(4));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        chunk.emit_get_global(display_sym);
        chunk.emit_const(a);
        chunk.emit_const(b);
        chunk.emit_eqv();
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_get_global(display_sym);
        chunk.emit_const(a);
        chunk.emit_const(c);
        chunk.emit_eqv();
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "#t#f");
    }
}
