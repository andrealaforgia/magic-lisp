//! Bytecode virtual machine.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use crate::bytecode::{Chunk, Const, Module, Op};
use crate::value::{Env, Value, is_truthy};

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

const NATIVE_NAMES: [&str; 14] = [
    "display", "newline", "+", "-", "*", "/", "=", "<", "<=", ">", ">=", "cons", "car", "cdr",
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
        Const::Float(n) => Value::Float(*n),
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
/// stack frame — Value::Closure calling into Vm::exec calling back into
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

/// Walks `depth - 1` parent links from `env` (depth 1 = `env` itself, the
/// immediately enclosing frame's captured locals; depth 2 = its parent;
/// etc.), matching how `Ctx::resolve_upvalue` counts levels at compile time.
fn resolve_env(env: &Env, depth: u8) -> Result<&Env, RuntimeError> {
    let mut current = env;
    for _ in 1..depth {
        current = current
            .parent
            .as_deref()
            .ok_or_else(|| error("upvalue depth exceeds the captured environment chain"))?;
    }
    Ok(current)
}

fn upvalue_cell(
    env: Option<&Rc<Env>>,
    depth: u8,
    slot: u8,
) -> Result<Rc<RefCell<Value>>, RuntimeError> {
    let env = env.ok_or_else(|| error("no captured environment to resolve an upvalue from"))?;
    let target = resolve_env(env, depth)?;
    target
        .locals
        .get(slot as usize)
        .cloned()
        .ok_or_else(|| error(format!("upvalue slot {slot} out of range")))
}

impl<'m> Vm<'m> {
    fn exec(
        &mut self,
        chunk: &Chunk,
        locals: &mut Vec<Rc<RefCell<Value>>>,
        env: Option<&Rc<Env>>,
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
                    let cell = locals
                        .get(slot)
                        .ok_or_else(|| error(format!("local slot {slot} out of range")))?;
                    stack.push(cell.borrow().clone());
                }
                op if op == Op::SetLocal as u8 => {
                    let slot = read_u8(code, &mut ip)? as usize;
                    let value = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during SET_LOCAL"))?;
                    let cell = locals
                        .get(slot)
                        .ok_or_else(|| error(format!("local slot {slot} out of range")))?;
                    *cell.borrow_mut() = value;
                    stack.push(Value::Unspecified);
                }
                op if op == Op::GetUpvalue as u8 => {
                    let depth = read_u8(code, &mut ip)?;
                    let slot = read_u8(code, &mut ip)?;
                    let cell = upvalue_cell(env, depth, slot)?;
                    let value = cell.borrow().clone();
                    stack.push(value);
                }
                op if op == Op::SetUpvalue as u8 => {
                    let depth = read_u8(code, &mut ip)?;
                    let slot = read_u8(code, &mut ip)?;
                    let value = stack
                        .pop()
                        .ok_or_else(|| error("stack underflow during SET_UPVALUE"))?;
                    let cell = upvalue_cell(env, depth, slot)?;
                    *cell.borrow_mut() = value;
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
                    locals.push(Rc::new(RefCell::new(value)));
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
                    // Every function value closes over whatever this frame
                    // currently has: its own locals (shared cells, not
                    // copies — mutations after this point are still visible
                    // through the closure) and whatever this frame itself
                    // closed over, so nesting captures transitively. A
                    // top-level define captures an empty, parentless
                    // environment, which is indistinguishable from "no
                    // captures" since nothing can ever resolve an upvalue
                    // into it.
                    let captured = Rc::new(Env {
                        locals: locals.clone(),
                        parent: env.cloned(),
                    });
                    stack.push(Value::Closure(idx, captured));
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
            Value::Closure(idx, closure_env) => {
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
                let result = self.exec(chunk, &mut locals, Some(closure_env), out);
                self.call_depth -= 1;
                result
            }
            other => Err(error(format!("cannot call a non-procedure value: {other}"))),
        }
    }
}

fn bind_arguments(
    chunk: &Chunk,
    mut args: Vec<Value>,
) -> Result<Vec<Rc<RefCell<Value>>>, RuntimeError> {
    let arity = chunk.arity as usize;
    let to_cells = |values: Vec<Value>| -> Vec<Rc<RefCell<Value>>> {
        values
            .into_iter()
            .map(|v| Rc::new(RefCell::new(v)))
            .collect()
    };
    if chunk.has_rest {
        if args.len() < arity {
            return Err(error(format!(
                "expected at least {arity} argument(s), got {}",
                args.len()
            )));
        }
        let rest = args.split_off(arity);
        let mut locals = to_cells(args);
        locals.push(Rc::new(RefCell::new(Value::List(rest))));
        Ok(locals)
    } else {
        if args.len() != arity {
            return Err(error(format!(
                "expected exactly {arity} argument(s), got {}",
                args.len()
            )));
        }
        Ok(to_cells(args))
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
    let vm_result: Result<(), RuntimeError> = std::thread::scope(|scope| {
        // An OS-level spawn failure (e.g. thread/memory exhaustion) is rare
        // but, like a joined-out panic, must still surface as a clean
        // RuntimeError rather than a caller-visible panic — the doc comment
        // above promises no crash except a genuine native stack overflow.
        let handle = std::thread::Builder::new()
            .stack_size(VM_STACK_SIZE)
            .spawn_scoped(scope, || run_on_this_thread(module, &mut buffer))
            .map_err(|e| error(format!("failed to spawn VM thread: {e}")))?;
        handle
            .join()
            .unwrap_or_else(|_| Err(error("internal error: VM thread panicked")))
    });
    // The final write can fail too (broken pipe, full disk) — discarding
    // that error would silently report success (exit 0) despite some or
    // all of the program's output never reaching its destination (a
    // security-review finding: this used to propagate correctly when
    // display/newline wrote directly to `out`, before buffering was
    // introduced). VM failure takes priority when both occur, since it
    // happened first.
    let flush_result = out
        .write_all(&buffer)
        .map_err(|e| error(format!("failed to write output: {e}")));
    vm_result.and(flush_result)
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
    vm.exec(entry, &mut locals, None, out)?;
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
        "/" => native_divide(args),
        "=" => native_compare("=", args, |a, b| a == b),
        "<" => native_compare("<", args, |a, b| a < b),
        "<=" => native_compare("<=", args, |a, b| a <= b),
        ">" => native_compare(">", args, |a, b| a > b),
        ">=" => native_compare(">=", args, |a, b| a >= b),
        "cons" => {
            let [a, b] = args else {
                return Err(error(format!(
                    "cons expects exactly 2 arguments, got {}",
                    args.len()
                )));
            };
            Ok(Value::Pair(Box::new(a.clone()), Box::new(b.clone())))
        }
        "car" => match args {
            [Value::Pair(a, _)] => Ok((**a).clone()),
            [other] => Err(error(format!("car expects a pair, found {other}"))),
            _ => Err(error(format!(
                "car expects exactly 1 argument, got {}",
                args.len()
            ))),
        },
        "cdr" => match args {
            [Value::Pair(_, b)] => Ok((**b).clone()),
            [other] => Err(error(format!("cdr expects a pair, found {other}"))),
            _ => Err(error(format!(
                "cdr expects exactly 1 argument, got {}",
                args.len()
            ))),
        },
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

fn any_float(args: &[Value]) -> bool {
    args.iter().any(|v| matches!(v, Value::Float(_)))
}

fn to_f64s(opname: &str, args: &[Value]) -> Result<Vec<f64>, RuntimeError> {
    args.iter()
        .map(|v| match v {
            Value::Int(n) => Ok(*n as f64),
            Value::Float(n) => Ok(*n),
            other => Err(error(format!(
                "{opname} expects numeric arguments, found {other}"
            ))),
        })
        .collect()
}

fn native_plus(args: &[Value]) -> Result<Value, RuntimeError> {
    if any_float(args) {
        return Ok(Value::Float(to_f64s("+", args)?.iter().sum()));
    }
    let ints = to_ints("+", args)?;
    Ok(Value::Int(
        ints.iter().fold(0i64, |acc, n| acc.wrapping_add(*n)),
    ))
}

fn native_minus(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(error("- requires at least 1 argument"));
    }
    if any_float(args) {
        let nums = to_f64s("-", args)?;
        let (first, rest) = nums.split_first().unwrap();
        return Ok(Value::Float(if rest.is_empty() {
            -first
        } else {
            rest.iter().fold(*first, |acc, n| acc - n)
        }));
    }
    let ints = to_ints("-", args)?;
    let (first, rest) = ints.split_first().unwrap();
    Ok(Value::Int(if rest.is_empty() {
        first.wrapping_neg()
    } else {
        rest.iter().fold(*first, |acc, n| acc.wrapping_sub(*n))
    }))
}

fn native_times(args: &[Value]) -> Result<Value, RuntimeError> {
    if any_float(args) {
        return Ok(Value::Float(to_f64s("*", args)?.iter().product()));
    }
    let ints = to_ints("*", args)?;
    Ok(Value::Int(
        ints.iter().fold(1i64, |acc, n| acc.wrapping_mul(*n)),
    ))
}

/// The result of dividing two exact integers: still exact if the divisor
/// evenly divides the accumulator, otherwise the point where the running
/// result must become a float (per the division rule: "exact at every
/// step, or a float").
enum IntDivStep {
    Exact(i64),
    Inexact(f64),
}

fn int_div_step(acc: i64, divisor: i64) -> Result<IntDivStep, RuntimeError> {
    if divisor == 0 {
        return Err(error("division by exact zero"));
    }
    // i64::MIN / -1 (and the equivalent %) is the one integer-division case
    // Rust panics on unconditionally, even in release builds — the same
    // overflow class this file otherwise always handles via wrapping
    // arithmetic (e.g. native_minus's unary case already uses
    // wrapping_neg() for this exact input). checked_rem/checked_div return
    // None only for that one case here (divisor == 0 is already excluded
    // above), and i64::MIN negated wraps back to itself, which is exactly
    // this division's true (exact) mathematical result modulo 2^64.
    match acc.checked_rem(divisor) {
        Some(0) => Ok(IntDivStep::Exact(
            acc.checked_div(divisor)
                .unwrap_or_else(|| acc.wrapping_neg()),
        )),
        Some(_) => Ok(IntDivStep::Inexact(acc as f64 / divisor as f64)),
        None => Ok(IntDivStep::Exact(acc.wrapping_neg())),
    }
}

fn native_divide(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(error("/ requires at least 1 argument"));
    }
    if any_float(args) {
        let nums = to_f64s("/", args)?;
        let (first, rest) = nums.split_first().unwrap();
        return Ok(Value::Float(if rest.is_empty() {
            1.0 / first
        } else {
            rest.iter().fold(*first, |acc, n| acc / n)
        }));
    }

    let ints = to_ints("/", args)?;
    if let [only] = ints[..] {
        return Ok(match int_div_step(1, only)? {
            IntDivStep::Exact(n) => Value::Int(n),
            IntDivStep::Inexact(f) => Value::Float(f),
        });
    }
    let (first, rest) = ints.split_first().unwrap();
    // Stays an exact integer division as long as every step divides evenly;
    // the moment one step doesn't, the whole result becomes (and, per the
    // division rule, stays) a float for any remaining divisors.
    let mut exact_acc = *first;
    let mut float_acc: Option<f64> = None;
    for &divisor in rest {
        if let Some(acc) = float_acc {
            float_acc = Some(acc / divisor as f64);
            continue;
        }
        match int_div_step(exact_acc, divisor)? {
            IntDivStep::Exact(n) => exact_acc = n,
            IntDivStep::Inexact(f) => float_acc = Some(f),
        }
    }
    Ok(match float_acc {
        Some(f) => Value::Float(f),
        None => Value::Int(exact_acc),
    })
}

fn native_compare(
    opname: &str,
    args: &[Value],
    holds: fn(f64, f64) -> bool,
) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(error(format!("{opname} requires at least 2 arguments")));
    }
    let nums = to_f64s(opname, args)?;
    let all_hold = nums.windows(2).all(|pair| holds(pair[0], pair[1]));
    Ok(Value::Bool(all_hold))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_program;
    use crate::reader::read_program;

    /// A writer that always fails, simulating a broken pipe or full disk —
    /// used to prove the final flush's error isn't silently discarded.
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("simulated broken pipe"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_failing_final_flush_is_reported_as_a_runtime_error_not_silently_ignored() {
        // A security-review finding: run() buffers all VM output into a
        // Vec<u8> (needed since `out` isn't necessarily Send) and only
        // writes it to the real `out` once, at the end — discarding that
        // write's Result would silently report success even if none of the
        // program's output actually reached its destination.
        let forms = read_program("(display 1)").unwrap();
        let module = compile_program(&forms).unwrap();
        assert!(run(&module, &mut FailingWriter).is_err());
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

    // --- B5: closures and pairs ---

    #[test]
    fn a_closure_captures_an_outer_local_usable_after_the_creator_returns() {
        let out = eval("(define (make-adder n) (lambda (x) (+ x n))) (display ((make-adder 3) 4))")
            .unwrap();
        assert_eq!(out, "7");
    }

    #[test]
    fn two_closures_from_the_same_call_share_one_mutable_cell() {
        let out = eval(
            "(define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v))))) \
             (define p (pairf)) \
             ((cdr p) 10) \
             (display ((car p)))",
        )
        .unwrap();
        assert_eq!(out, "10");
    }

    #[test]
    fn two_separate_calls_to_the_same_factory_produce_independent_cells() {
        let out = eval(
            "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n))) \
             (define a (counter)) (define b (counter)) \
             (display (a)) (newline) (display (a)) (newline) (display (b))",
        )
        .unwrap();
        assert_eq!(out, "1\n2\n1");
    }

    #[test]
    fn cons_constructs_a_pair_retrievable_via_car_and_cdr() {
        assert_eq!(
            eval("(display (car (cons 1 2))) (display (cdr (cons 1 2)))").unwrap(),
            "12"
        );
    }

    #[test]
    fn car_on_a_non_pair_is_a_runtime_error() {
        // Checks the specific message, not just is_err(): a wrong-type
        // single argument and a wrong argument *count* both return some
        // Err, so a bare is_err() can't tell "correctly rejected as not a
        // pair" apart from "fell through to the argument-count arm" --
        // both would look identical to the caller.
        let err = eval("(display (car 5))").unwrap_err();
        assert!(
            err.message.contains("expects a pair"),
            "expected a not-a-pair error, got: {}",
            err.message
        );
    }

    #[test]
    fn cdr_on_a_non_pair_is_a_runtime_error() {
        let err = eval("(display (cdr 5))").unwrap_err();
        assert!(
            err.message.contains("expects a pair"),
            "expected a not-a-pair error, got: {}",
            err.message
        );
    }

    #[test]
    fn cons_requires_exactly_two_arguments() {
        assert!(eval("(display (cons 1))").is_err());
        assert!(eval("(display (cons 1 2 3))").is_err());
    }

    #[test]
    fn a_doubly_nested_closure_captures_a_grandparent_local_via_a_two_level_upvalue() {
        // Beyond both required demos (which only nest one level deep): the
        // innermost lambda's free variable resolves as depth=2, walking
        // through the middle lambda's own (empty, for this variable)
        // environment to reach the outermost frame's local.
        let out =
            eval("(define (outer x) (lambda () (lambda () x))) (display (((outer 42))))").unwrap();
        assert_eq!(out, "42");
    }

    #[test]
    fn mutating_a_captured_variable_before_the_closure_is_ever_called_is_still_observed() {
        // Proves the shared cell reflects whatever set! last wrote,
        // independent of when the closure itself happens to be invoked --
        // not a value snapshotted at closure-creation time.
        let out = eval(
            "(define (f) (let ((x 1)) (define g (lambda () x)) (set! x 99) (g))) (display (f))",
        )
        .unwrap();
        assert_eq!(out, "99");
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
        // exactly MAX_CALL_DEPTH nested Value::Closure calls, the last
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
                    vm.exec(entry, &mut Vec::new(), None, &mut out).unwrap();
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
    fn minus_with_exactly_one_argument_negates_it() {
        // B4 completes -'s variadic rule: unlike B2's original >=2 minimum,
        // a single argument now negates rather than erroring.
        assert_eq!(eval("(display (- 5))").unwrap(), "-5");
    }

    #[test]
    fn minus_with_zero_arguments_is_a_runtime_error() {
        assert!(eval("(display (-))").is_err());
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
    fn times_with_exactly_one_argument_is_that_argument() {
        // B4 completes *'s variadic rule: unlike B2's original >=2 minimum,
        // 0 or 1 arguments now work like +'s (identity 1, or the value itself).
        assert_eq!(eval("(display (* 5))").unwrap(), "5");
    }

    #[test]
    fn times_with_zero_arguments_is_one() {
        assert_eq!(eval("(display (*))").unwrap(), "1");
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
    fn equals_is_true_for_a_chain_of_equal_values() {
        assert_eq!(eval("(display (= 2 2 2))").unwrap(), "#t");
    }

    #[test]
    fn equals_checks_the_whole_chain_not_just_adjacent_pairs() {
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

    // --- B4: division and float arithmetic ---

    #[test]
    fn division_with_zero_arguments_is_a_runtime_error() {
        assert!(eval("(display (/))").is_err());
    }

    #[test]
    fn division_with_one_integer_argument_inverts_it_as_a_float_when_inexact() {
        assert_eq!(eval("(display (/ 6))").unwrap(), "0.16666666666666666");
    }

    #[test]
    fn division_with_one_integer_argument_stays_exact_when_it_divides_one_evenly() {
        assert_eq!(eval("(display (/ 1))").unwrap(), "1");
        assert_eq!(eval("(display (/ -1))").unwrap(), "-1");
    }

    #[test]
    fn division_with_one_float_argument_inverts_it() {
        // A qa test-design review found this exact case (the single-float-
        // argument path through native_divide's any_float branch, `1.0 /
        // first`) had no test at all -- confirmed via independent mutation
        // replay to genuinely survive `%`/`*` mutants, unlike the sibling
        // single-integer-argument case above.
        assert_eq!(eval("(display (/ 4.0))").unwrap(), "0.25");
    }

    #[test]
    fn whole_number_division_that_comes_out_exact_yields_an_integer() {
        assert_eq!(eval("(display (/ 6 3))").unwrap(), "2");
    }

    #[test]
    fn whole_number_division_that_does_not_come_out_exact_yields_a_float() {
        assert_eq!(eval("(display (/ 7 2))").unwrap(), "3.5");
    }

    #[test]
    fn a_whole_number_divided_by_a_float_yields_a_float_even_when_exact() {
        assert_eq!(eval("(display (/ 6 3.0))").unwrap(), "2.0");
    }

    #[test]
    fn dividing_by_exact_integer_zero_is_a_runtime_error() {
        assert!(eval("(display (/ 6 0))").is_err());
    }

    #[test]
    fn dividing_i64_min_by_negative_one_is_exact_and_does_not_panic() {
        // A security-review finding: i64::MIN % -1 (and / -1) is the one
        // integer-division case Rust panics on unconditionally, even in
        // release builds. i64::MIN negated wraps back to itself (its true
        // value has no positive i64 representation), matching how
        // native_minus's own unary case already handles this input.
        let out = eval(&format!("(display (/ {} -1))", i64::MIN)).unwrap();
        assert_eq!(out, i64::MIN.to_string());
    }

    #[test]
    fn a_division_chain_reaching_i64_min_mid_fold_then_dividing_by_negative_one_does_not_panic() {
        let out = eval(&format!("(display (/ {} 1 -1))", i64::MIN)).unwrap();
        assert_eq!(out, i64::MIN.to_string());
    }

    #[test]
    fn once_a_division_chain_goes_inexact_a_later_integer_zero_divisor_follows_float_rules() {
        // 7/2 is already inexact (3.5), so this must NOT error like an
        // exact int/0 division would -- it follows IEEE float rules instead.
        assert_eq!(eval("(display (/ 7 2 0))").unwrap(), "+inf.0");
    }

    #[test]
    fn dividing_a_float_by_zero_follows_ieee_rules_instead_of_erroring() {
        assert_eq!(eval("(display (/ 1.0 0.0))").unwrap(), "+inf.0");
        assert_eq!(eval("(display (/ -1.0 0.0))").unwrap(), "-inf.0");
    }

    #[test]
    fn plus_promotes_to_float_when_any_argument_is_a_float() {
        assert_eq!(eval("(display (+ 1 2.0))").unwrap(), "3.0");
    }

    #[test]
    fn minus_promotes_to_float_when_any_argument_is_a_float() {
        assert_eq!(eval("(display (- 5.0 2))").unwrap(), "3.0");
    }

    #[test]
    fn times_promotes_to_float_when_any_argument_is_a_float() {
        assert_eq!(eval("(display (* 2 2.5))").unwrap(), "5.0");
    }

    #[test]
    fn comparisons_support_mixed_integer_and_float_arguments() {
        assert_eq!(eval("(display (< 1 1.5 2))").unwrap(), "#t");
        assert_eq!(eval("(display (= 2 2.0))").unwrap(), "#t");
    }

    #[test]
    fn less_than_or_equal_holds_for_equal_and_increasing_values() {
        assert_eq!(eval("(display (<= 1 1 2))").unwrap(), "#t");
    }

    #[test]
    fn less_than_or_equal_is_false_when_the_chain_decreases() {
        assert_eq!(eval("(display (<= 2 1))").unwrap(), "#f");
    }

    #[test]
    fn greater_than_holds_for_a_strictly_decreasing_chain() {
        assert_eq!(eval("(display (> 3 2 1))").unwrap(), "#t");
    }

    #[test]
    fn greater_than_is_false_when_the_chain_increases() {
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
    }

    #[test]
    fn greater_than_or_equal_is_false_when_the_chain_increases() {
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
