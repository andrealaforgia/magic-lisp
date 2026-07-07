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

/// Caps native-Rust recursion depth across MagicLisp function calls (this
/// language has no tail-call elimination yet, and every call is a real Rust
/// stack frame — Value::Function calling into Vm::exec calling back into
/// Vm::call_value). Without this, ordinary, non-malicious recursive code
/// (idiomatic recursion — including named-`let` iteration, B3's headline
/// feature — can abort the whole process via native stack overflow instead
/// of returning a clean RuntimeError (a security-review finding on B3).
///
/// Sized empirically, not guessed, against `run`'s dedicated `VM_STACK_SIZE`
/// thread (see below): a debug build recursing with no depth guard at all
/// natively overflows that 64 MiB stack at a call depth of roughly
/// 7000-8000 (measured by bisection). 4000 keeps a healthy safety margin
/// under that measured worst case while still giving ordinary recursive
/// programs real headroom — e.g. a named-`let` loop summing 1..=1000 (which
/// used to fail outright against this crate's old 512/128 limits, before
/// `run` gave the VM its own generous stack) comfortably succeeds.
const MAX_CALL_DEPTH: usize = 4000;

struct Vm<'m> {
    module: &'m Module,
    globals: HashMap<String, Value>,
    call_depth: usize,
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
                if self.call_depth >= MAX_CALL_DEPTH {
                    return Err(error(format!(
                        "maximum call depth exceeded ({MAX_CALL_DEPTH}) — possible infinite or too-deep recursion"
                    )));
                }
                let chunk = self
                    .module
                    .functions
                    .get(*idx as usize)
                    .ok_or_else(|| error(format!("function index {idx} out of range")))?;
                let mut locals = bind_arguments(chunk, args)?;
                self.call_depth += 1;
                let result = self.exec(chunk, &mut locals, out);
                self.call_depth -= 1;
                result
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

/// The VM always executes on a dedicated thread with this much native stack,
/// rather than whatever stack the caller happens to be running on (which
/// might be a constrained default, e.g. 2 MiB on a spawned thread). This
/// makes MAX_CALL_DEPTH's safety margin a property of the interpreter
/// itself instead of an accident of the caller's environment, and gives
/// ordinary non-tail-recursive MagicLisp programs (this language has no
/// TCO yet, so idiomatic recursion — including named-`let` iteration,
/// B3's headline feature — burns one native frame per call) real, usable
/// headroom instead of failing on loops of only a few hundred iterations.
const VM_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Runs `module` to completion on a dedicated `VM_STACK_SIZE` thread.
///
/// If VM execution panics for any reason (a genuine internal bug, since
/// every intentional failure path already returns `Err(RuntimeError)`
/// rather than panicking), that panic is caught at the thread join below
/// and converted into a generic `RuntimeError` instead of unwinding into
/// the caller — so a caller-visible crash always means a real native stack
/// overflow, never an ordinary bug. This is deliberate defense in depth,
/// not a substitute for the guarded error paths elsewhere in this file.
pub fn run(module: &Module, out: &mut impl Write) -> Result<(), RuntimeError> {
    // `out` (e.g. a locked stdout handle) isn't necessarily Send, so it
    // can't be captured directly by the spawned thread's closure below.
    // Buffering into a plain (Send) Vec<u8> on that thread and flushing it
    // to the real `out` here, after the thread has been joined, sidesteps
    // that without requiring every caller's writer to be Send.
    let mut buffer = Vec::new();
    let result = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(VM_STACK_SIZE)
            .spawn_scoped(scope, || run_on_this_thread(module, &mut buffer))
            .expect("failed to spawn VM thread")
            .join()
    });
    let _ = out.write_all(&buffer);
    result.unwrap_or_else(|_| Err(error("internal error: VM thread panicked")))
}

fn run_on_this_thread(module: &Module, out: &mut impl Write) -> Result<(), RuntimeError> {
    let mut vm = Vm {
        module,
        globals: default_globals(),
        call_depth: 0,
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
    fn recursion_up_to_the_call_depth_limit_still_succeeds() {
        // MAX_CALL_DEPTH - 1 recursive steps plus the initial call is
        // exactly MAX_CALL_DEPTH nested Value::Function calls, the last
        // one landing on call_depth == MAX_CALL_DEPTH - 1 (checked against
        // the limit *before* incrementing), so this is the deepest
        // recursion the guard is supposed to still allow.
        let src = format!(
            "(define (count-down n) (if (= n 0) 0 (count-down (- n 1)))) \
             (display (count-down {}))",
            MAX_CALL_DEPTH - 1
        );
        assert_eq!(eval(&src).unwrap(), "0");
    }

    #[test]
    fn recursion_beyond_the_call_depth_limit_is_a_clean_runtime_error_not_a_crash() {
        // One recursive step deeper than the previous test's boundary is
        // enough to push the (MAX_CALL_DEPTH + 1)-th nested call past the
        // limit. Without the call_depth guard this input would abort the
        // whole test process via native stack overflow instead of failing
        // a single test (a security-review finding on B3).
        let src = format!(
            "(define (count-down n) (if (= n 0) 0 (count-down (- n 1)))) \
             (display (count-down {}))",
            MAX_CALL_DEPTH
        );
        let err = eval(&src).unwrap_err();
        assert!(
            err.message.contains("call depth"),
            "expected a call-depth error, got: {}",
            err.message
        );
    }

    #[test]
    fn call_depth_is_restored_after_a_call_returns_so_sequential_calls_each_get_a_fresh_budget() {
        // If call_depth were incremented (instead of decremented) as calls
        // return, the counter would leak upward across independent calls
        // instead of being restored. Each of these two calls is safely
        // shallow on its own (well under MAX_CALL_DEPTH), so this only
        // passes if the first call's depth accounting is fully unwound
        // before the second one starts.
        let src = format!(
            "(define (count-down n) (if (= n 0) 0 (count-down (- n 1)))) \
             (display (count-down {n})) (display (count-down {n}))",
            n = MAX_CALL_DEPTH - 10
        );
        assert_eq!(eval(&src).unwrap(), "00");
    }

    #[test]
    fn call_depth_is_restored_after_a_call_errors_out_from_exceeding_the_limit() {
        // MagicLisp has no exception handling, so a program-level error
        // always terminates the whole `run()` — there's no way to observe
        // depth restoration-after-error through source text alone. This is
        // a white-box test on a single, shared Vm instance instead: every
        // level of the recursive chain that triggered the depth error must
        // still unwind and decrement call_depth on the way back up (the
        // decrement isn't skipped just because the call ultimately
        // failed), or a subsequent, independent, safely-shallow call on
        // that same Vm would incorrectly start from leftover depth and
        // fail too.
        //
        // Driving MAX_CALL_DEPTH's worth of real native recursion needs the
        // same generous stack `run()` normally provides; calling straight
        // into call_value here bypasses that wrapper, so this test spawns
        // its own equivalently-sized thread rather than relying on the
        // ambient (possibly much smaller) test-thread stack.
        let forms =
            read_program("(define (count-down n) (if (= n 0) 0 (count-down (- n 1))))").unwrap();
        let module = compile_program(&forms).unwrap();
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(VM_STACK_SIZE)
                .spawn_scoped(scope, || {
                    let mut vm = Vm {
                        module: &module,
                        globals: default_globals(),
                        call_depth: 0,
                    };
                    let mut out = Vec::new();
                    let entry = &module.functions[module.entry_index as usize];
                    vm.exec(entry, &mut Vec::new(), &mut out).unwrap();
                    let count_down = vm.globals.get("count-down").cloned().unwrap();

                    let over_limit = vm.call_value(
                        &count_down,
                        vec![Value::Int(MAX_CALL_DEPTH as i64)],
                        &mut out,
                    );
                    assert!(over_limit.is_err());
                    assert_eq!(
                        vm.call_depth, 0,
                        "call_depth must be fully unwound after an error, not leaked"
                    );

                    let still_works = vm
                        .call_value(&count_down, vec![Value::Int(5)], &mut out)
                        .unwrap();
                    assert_eq!(still_works, Value::Int(0));
                })
                .expect("failed to spawn test thread")
                .join()
                .expect("test thread panicked");
        });
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
        //
        // The assertion checks the exact message, not just is_err(): `run`
        // executes on a spawned thread and converts an escaping panic into
        // a generic RuntimeError too (see run's doc comment), so a bare
        // is_err() can't tell "the underflow guard did its job" apart from
        // "the guard was silently broken and something downstream panicked
        // instead" — both would look identical to the caller. This is what
        // caught a real `stack.len() < argc + 1` -> `argc * 1` mutant that
        // a plain is_err() assertion missed.
        use crate::bytecode::Const;
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_call(1);
        chunk.emit_halt();
        let mut out = Vec::new();
        let err = run(&module_of(chunk), &mut out).unwrap_err();
        assert!(
            err.message.contains("stack underflow during CALL"),
            "expected a stack-underflow error, got: {}",
            err.message
        );
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
