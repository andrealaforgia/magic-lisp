//! Bytecode virtual machine.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::rc::Rc;
use std::sync::mpsc;

use crate::bytecode::{Chunk, Const, Module, Op};
use crate::compiler::sexpr_to_const;
use crate::reader::{self, Sexpr};
use crate::value::{Env, Value, is_truthy, value_equal, value_eqv, write_repr};

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

const NATIVE_NAMES: [&str; 124] = [
    "display",
    "newline",
    "+",
    "-",
    "*",
    "/",
    "=",
    "<",
    "<=",
    ">",
    ">=",
    "cons",
    "car",
    "cdr",
    // B7: the numeric library (spec 4.1).
    "quotient",
    "remainder",
    "modulo",
    "abs",
    "min",
    "max",
    "zero?",
    "positive?",
    "negative?",
    "even?",
    "odd?",
    "floor",
    "ceiling",
    "round",
    "truncate",
    "sqrt",
    "expt",
    "exp",
    "log",
    "sin",
    "cos",
    "tan",
    "atan",
    "number?",
    "integer?",
    "float?",
    "exact->inexact",
    "inexact->exact",
    "number->string",
    "string->number",
    // B8: type predicates and the three equality relations (spec 3.7, 4.2).
    "eq?",
    "eqv?",
    "equal?",
    "not",
    "null?",
    "pair?",
    "list?",
    "symbol?",
    "string?",
    "char?",
    "boolean?",
    "procedure?",
    "vector?",
    "hash?",
    "make-hash",
    // B9: pairs and lists (spec 5.1).
    "set-car!",
    "set-cdr!",
    "caar",
    "cadr",
    "cdar",
    "cddr",
    "caddr",
    "list",
    "length",
    "append",
    "reverse",
    "list-ref",
    "list-tail",
    "last-pair",
    "member",
    "memv",
    "memq",
    "assoc",
    "assv",
    "assq",
    "map",
    "for-each",
    "filter",
    "fold-left",
    "fold-right",
    "reduce",
    "apply",
    // B10: strings and characters (spec 6.1, 6.2).
    "string-length",
    "string-ref",
    "substring",
    "string-append",
    "string=?",
    "string<?",
    "string>?",
    "symbol->string",
    "string->symbol",
    "list->string",
    "string->list",
    "string-upcase",
    "string-downcase",
    "char->integer",
    "integer->char",
    "char=?",
    "char<?",
    "char-alphabetic?",
    "char-numeric?",
    "char-whitespace?",
    // B11: vectors and hash tables (spec 4.5, 4.6).
    "vector",
    "make-vector",
    "vector-ref",
    "vector-set!",
    "vector-length",
    "vector->list",
    "list->vector",
    "vector-fill!",
    "hash-ref",
    "hash-set!",
    "hash-remove!",
    "hash-count",
    "hash-keys",
    "hash-has-key?",
    // B12: input reading and the write/display output distinction (spec
    // 3.2, 4.8).
    "read",
    "read-line",
    "eof-object?",
    "write",
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
        Const::Str(s) => Value::Str(Rc::new(s.clone())),
        Const::Symbol(s) => Value::Symbol(s.clone()),
        Const::List(items) => Value::List(Rc::new(items.iter().map(const_to_value).collect())),
        Const::Char(c) => Value::Char(*c),
        Const::Vector(items) => Value::Vector(Rc::new(RefCell::new(
            items.iter().map(const_to_value).collect(),
        ))),
        Const::Pair(car, cdr) => {
            // Walks the cdr spine iteratively, then folds the result back
            // into a Pair chain from the tail outward: a dotted-list
            // literal's `Const::Pair` chain length is program data, not
            // nesting depth, so recursing here once per element would
            // crash on an ordinary large literal (warden security review,
            // msg #146) even though the source is a single flat form.
            let mut cars = vec![const_to_value(car)];
            let mut tail: &Const = cdr;
            let final_tail = loop {
                match tail {
                    Const::Pair(next_car, next_cdr) => {
                        cars.push(const_to_value(next_car));
                        tail = next_cdr;
                    }
                    other => break const_to_value(other),
                }
            };
            let mut acc = final_tail;
            for car in cars.into_iter().rev() {
                acc = Value::Pair(Rc::new(RefCell::new((car, acc))));
            }
            acc
        }
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

/// Caps native-Rust recursion depth across MagicLisp function calls made in
/// non-tail position (B6 gave tail calls their own O(1)-space trampoline in
/// `exec`, so only *genuine* non-tail recursion consumes native stack here —
/// Value::Closure calling into Vm::exec calling back into Vm::call_value).
/// Without this, ordinary, non-malicious deep recursion can abort the whole
/// process via native stack overflow instead of returning a clean
/// RuntimeError (a security-review finding on B3).
///
/// Sized empirically, not guessed, against `run`'s dedicated `VM_STACK_SIZE`
/// thread (see below): with that stack, a debug build recursing with no
/// depth guard at all natively overflows at a call depth of roughly
/// 295,000-300,000 (measured by bisection: a non-tail-recursive sum,
/// `(+ n (sum (- n 1)))`, run at increasing depths until it crashes).
/// 150,000 keeps very close to a 2x safety margin under that measured worst
/// case while comfortably clearing B6's requirement of correctly completing
/// genuine (non-tail) recursion "on the order of 100,000" levels deep.
const MAX_CALL_DEPTH: usize = 150_000;

struct Vm<'m> {
    module: &'m Module,
    globals: HashMap<String, Value>,
    call_depth: usize,
    /// Text pulled from stdin but not yet consumed by `read`/`read-line`
    /// (spec 4.8) -- refilled one raw chunk at a time via `stdin_channel`
    /// only when there isn't already enough buffered to satisfy the
    /// current call, so a program that never calls `read`/`read-line`
    /// never touches stdin at all.
    stdin_buffer: String,
    stdin_channel: StdinChannel,
}

/// A lazy, on-demand relay to the real stdin reader living on the thread
/// that called [`run_with_stdin`], used because that reader (e.g. a locked
/// stdout handle, or any generic `&mut impl BufRead`) isn't necessarily
/// `Send` and so can't be moved into the VM's own dedicated execution
/// thread directly -- unlike eagerly reading everything up front (blocking
/// until the ENTIRE stream reaches end-of-input before the VM even starts),
/// this only blocks waiting for one line at a time, and only when the
/// running program actually calls `read`/`read-line`, so a program that
/// never reads stdin never touches it -- critical when stdin is an
/// interactive terminal or an otherwise-still-open stream rather than a
/// short, already-closed pipe.
struct StdinChannel {
    request: mpsc::Sender<()>,
    response: mpsc::Receiver<Option<String>>,
}

impl StdinChannel {
    /// Requests and returns the next raw chunk (one line, including its
    /// trailing newline if the underlying stream had one), or `None` once
    /// the underlying stream is genuinely exhausted (or, for [`Self::none`],
    /// unconditionally).
    fn next_chunk(&self) -> Option<String> {
        self.request.send(()).ok()?;
        self.response.recv().ok().flatten()
    }

    /// A relay-less stand-in for callers (plain [`run`]) that supply no
    /// stdin at all: both channel halves are dropped immediately, so the
    /// very first (and only) send in [`Self::next_chunk`] fails right away
    /// without ever needing a servicing thread on the other end.
    fn none() -> Self {
        let (request, _) = mpsc::channel();
        let (_, response) = mpsc::channel();
        StdinChannel { request, response }
    }
}

/// Walks `depth - 1` parent links from `env` (depth 1 = `env` itself, the
/// immediately enclosing frame's captured locals; depth 2 = its parent;
/// etc.), matching how `Ctx::resolve_upvalue` counts levels at compile time.
fn resolve_env(env: &Env, depth: u8) -> Result<&Env, RuntimeError> {
    // depth == 0 is never emitted by the compiler (1 is the immediately
    // enclosing frame) and isn't a meaningful encoding of anything -- reject
    // it explicitly rather than letting the `1..depth` loop below silently
    // treat it the same as depth == 1 (a security-review finding: harmless
    // today since only hand-crafted bytecode could ever produce it, but a
    // degenerate encoding accepted by construction is worth closing off).
    if depth == 0 {
        return Err(error("upvalue depth 0 is not a valid encoding"));
    }
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
        mut chunk: &'m Chunk,
        mut locals: Vec<Rc<RefCell<Value>>>,
        mut env: Option<Rc<Env>>,
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        // A tail call (see Op::TailCall below) reassigns chunk/locals/env and
        // `continue`s this outer loop instead of recursing into `exec`
        // again, so a chain of tail calls of any length reuses this single
        // native stack frame — the whole point of tail-call optimization.
        'trampoline: loop {
            let mut stack: Vec<Value> = Vec::new();
            let code = &chunk.code;
            let mut ip = 0usize;

            // Every instruction is at least 1 byte and (for this language,
            // which has no backward jumps yet) ip only ever moves forward
            // within a single pass of this loop, so no correct program
            // executes more than code.len() instructions here. This bounds
            // total loop iterations independently of ip's own bookkeeping,
            // so a broken operand-advance can never hang the interpreter —
            // it fails cleanly instead.
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
                        let cell = upvalue_cell(env.as_ref(), depth, slot)?;
                        let value = cell.borrow().clone();
                        stack.push(value);
                    }
                    op if op == Op::SetUpvalue as u8 => {
                        let depth = read_u8(code, &mut ip)?;
                        let slot = read_u8(code, &mut ip)?;
                        let value = stack
                            .pop()
                            .ok_or_else(|| error("stack underflow during SET_UPVALUE"))?;
                        let cell = upvalue_cell(env.as_ref(), depth, slot)?;
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
                        stack.push(Value::Bool(value_eqv(&a, &b)));
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
                            parent: env.clone(),
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
                    op if op == Op::TailCall as u8 => {
                        let argc = read_u8(code, &mut ip)? as usize;
                        if stack.len() < argc + 1 {
                            return Err(error("stack underflow during TAIL_CALL"));
                        }
                        let args = stack.split_off(stack.len() - argc);
                        let callee = stack.pop().unwrap();
                        match callee {
                            Value::Closure(idx, closure_env) => {
                                // Reuse this native frame instead of recursing:
                                // reassign the frame state and loop, rather than
                                // calling exec() again. call_depth deliberately
                                // stays untouched -- a tail call doesn't grow the
                                // native stack, so it must not count against the
                                // depth guard that exists to bound native
                                // recursion.
                                let next_chunk =
                                    self.module.functions.get(idx as usize).ok_or_else(|| {
                                        error(format!("function index {idx} out of range"))
                                    })?;
                                locals = bind_arguments(next_chunk, args)?;
                                chunk = next_chunk;
                                env = Some(closure_env);
                                continue 'trampoline;
                            }
                            Value::Native(name) => {
                                // A tail call to a native has no further
                                // MagicLisp code following it by construction,
                                // so its result is exec's own result.
                                return call_native(self, &name, &args, out);
                            }
                            other => {
                                return Err(error(format!(
                                    "cannot call a non-procedure value: {other}"
                                )));
                            }
                        }
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
    }

    fn call_value(
        &mut self,
        callee: &Value,
        args: Vec<Value>,
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::Native(name) => call_native(self, name, &args, out),
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
                let locals = bind_arguments(chunk, args)?;
                self.call_depth += 1;
                let result = self.exec(chunk, locals, Some(closure_env.clone()), out);
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
        locals.push(Rc::new(RefCell::new(Value::List(Rc::new(rest)))));
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
/// itself instead of an accident of the caller's environment. Tail-recursive
/// MagicLisp code (B6) runs in O(1) native stack regardless of length, but
/// genuine non-tail recursion still burns one native frame per call, and
/// B6 requires that to reach on the order of 100,000 levels deep — this
/// stack size (raised from B3-era's 64 MiB) is what makes a MAX_CALL_DEPTH
/// that high survivable without ever reaching real hardware stack overflow;
/// see MAX_CALL_DEPTH's own doc comment for the bisected numbers.
const VM_STACK_SIZE: usize = 3 * 1024 * 1024 * 1024;

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
    // No relay to service: `StdinChannel::none()` fails every request
    // instantly on its own, so this can spawn and join exactly like before
    // stdin support existed at all.
    let mut buffer = Vec::new();
    let vm_result: Result<(), RuntimeError> = std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .stack_size(VM_STACK_SIZE)
            .spawn_scoped(scope, || {
                run_on_this_thread(module, &mut buffer, StdinChannel::none())
            })
            .map_err(|e| error(format!("failed to spawn VM thread: {e}")))?;
        handle
            .join()
            .unwrap_or_else(|_| Err(error("internal error: VM thread panicked")))
    });
    let flush_result = out
        .write_all(&buffer)
        .map_err(|e| error(format!("failed to write output: {e}")));
    vm_result.and(flush_result)
}

/// Like [`run`], but also makes `input` available to the `read`/`read-line`
/// natives (spec 4.8), lazily: `input` (e.g. a locked stdin handle) isn't
/// necessarily `Send`, so it can't be moved into the VM's own dedicated
/// execution thread directly the way `out`'s owned `Vec<u8>` buffer is
/// below. Instead, the VM thread requests one line at a time over a
/// channel, serviced by THIS thread (which does own `input`) in the loop
/// below, so a program that never calls `read`/`read-line` never blocks on
/// `input` at all -- unlike eagerly reading the entire stream to its end
/// before the VM even starts, which would hang indefinitely on an
/// interactive terminal or any other still-open stream a program has no
/// intention of ever reading from.
pub fn run_with_stdin(
    module: &Module,
    out: &mut impl Write,
    input: &mut impl BufRead,
) -> Result<(), RuntimeError> {
    let mut buffer = Vec::new();
    let (req_tx, req_rx) = mpsc::channel::<()>();
    let (resp_tx, resp_rx) = mpsc::channel::<Option<String>>();
    let stdin_channel = StdinChannel {
        request: req_tx,
        response: resp_rx,
    };
    let vm_result: Result<(), RuntimeError> = std::thread::scope(|scope| {
        // An OS-level spawn failure (e.g. thread/memory exhaustion) is rare
        // but, like a joined-out panic, must still surface as a clean
        // RuntimeError rather than a caller-visible panic — the doc comment
        // above promises no crash except a genuine native stack overflow.
        let handle = std::thread::Builder::new()
            .stack_size(VM_STACK_SIZE)
            .spawn_scoped(scope, || {
                run_on_this_thread(module, &mut buffer, stdin_channel)
            })
            .map_err(|e| error(format!("failed to spawn VM thread: {e}")))?;
        // Services stdin requests one at a time, exactly as they're made.
        // The VM thread finishing (or panicking) drops its sender, which
        // ends this loop on its own via a plain `Err` from `recv` -- no
        // polling or timeout needed.
        while let Ok(()) = req_rx.recv() {
            let mut line = String::new();
            let chunk = match input.read_line(&mut line) {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(line),
            };
            if resp_tx.send(chunk).is_err() {
                break;
            }
        }
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

fn run_on_this_thread(
    module: &Module,
    out: &mut impl Write,
    stdin_channel: StdinChannel,
) -> Result<(), RuntimeError> {
    let mut vm = Vm {
        module,
        globals: default_globals(),
        call_depth: 0,
        stdin_buffer: String::new(),
        stdin_channel,
    };
    let entry = module
        .functions
        .get(module.entry_index as usize)
        .ok_or_else(|| error("entry function index out of range"))?;
    vm.exec(entry, Vec::new(), None, out)?;
    Ok(())
}

fn call_native(
    vm: &mut Vm,
    name: &str,
    args: &[Value],
    out: &mut impl Write,
) -> Result<Value, RuntimeError> {
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
            Ok(Value::Pair(Rc::new(RefCell::new((a.clone(), b.clone())))))
        }
        "car" => native_unary("car", args, |v| car_of("car", v)),
        "cdr" => native_unary("cdr", args, |v| cdr_of("cdr", v)),
        "caar" => native_unary("caar", args, |v| cxr("caar", "aa", v)),
        "cadr" => native_unary("cadr", args, |v| cxr("cadr", "ad", v)),
        "cdar" => native_unary("cdar", args, |v| cxr("cdar", "da", v)),
        "cddr" => native_unary("cddr", args, |v| cxr("cddr", "dd", v)),
        "caddr" => native_unary("caddr", args, |v| cxr("caddr", "add", v)),
        "set-car!" => native_set_half("set-car!", args, |cell, v| cell.0 = v),
        "set-cdr!" => native_set_half("set-cdr!", args, |cell, v| cell.1 = v),
        "list" => Ok(vec_to_list(args.to_vec())),
        "length" => native_unary("length", args, |v| {
            Ok(Value::Int(list_to_vec("length", v)?.len() as i64))
        }),
        "append" => {
            let [a, b] = args else {
                return Err(error(format!(
                    "append expects exactly 2 arguments, got {}",
                    args.len()
                )));
            };
            let mut items = list_to_vec("append", a)?;
            items.extend(list_to_vec("append", b)?);
            Ok(vec_to_list(items))
        }
        "reverse" => native_unary("reverse", args, |v| {
            let mut items = list_to_vec("reverse", v)?;
            items.reverse();
            Ok(vec_to_list(items))
        }),
        "list-ref" => {
            let [v, Value::Int(n)] = args else {
                return Err(error(format!(
                    "list-ref expects a list and an integer index, got {} arguments",
                    args.len()
                )));
            };
            let items = list_to_vec("list-ref", v)?;
            usize::try_from(*n)
                .ok()
                .and_then(|i| items.get(i).cloned())
                .ok_or_else(|| error(format!("list-ref index {n} is out of range")))
        }
        "list-tail" => {
            let [v, Value::Int(n)] = args else {
                return Err(error(format!(
                    "list-tail expects a list and an integer index, got {} arguments",
                    args.len()
                )));
            };
            if *n < 0 {
                return Err(error(format!("list-tail index {n} must not be negative")));
            }
            let mut current = v.clone();
            for _ in 0..*n {
                current = cdr_of("list-tail", &current)?;
            }
            Ok(current)
        }
        "last-pair" => native_unary("last-pair", args, |v| last_pair("last-pair", v)),
        "member" => native_member("member", args, value_equal),
        "memv" => native_member("memv", args, value_eqv),
        "memq" => native_member("memq", args, value_eqv),
        "assoc" => native_assoc("assoc", args, value_equal),
        "assv" => native_assoc("assv", args, value_eqv),
        "assq" => native_assoc("assq", args, value_eqv),
        "map" => native_map(vm, args, out),
        "for-each" => native_for_each(vm, args, out),
        "filter" => native_filter(vm, args, out),
        "fold-left" => native_fold_left(vm, args, out),
        "fold-right" => native_fold_right(vm, args, out),
        "reduce" => native_reduce(vm, args, out),
        "apply" => native_apply(vm, args, out),
        "string-length" => native_string_length(args),
        "string-ref" => native_string_ref(args),
        "substring" => native_substring(args),
        "string-append" => native_string_append(args),
        "string=?" => native_string_compare("string=?", args, |a, b| a == b),
        "string<?" => native_string_compare("string<?", args, |a, b| a < b),
        "string>?" => native_string_compare("string>?", args, |a, b| a > b),
        "symbol->string" => native_symbol_to_string(args),
        "string->symbol" => native_string_to_symbol(args),
        "list->string" => native_list_to_string(args),
        "string->list" => native_string_to_list(args),
        "string-upcase" => native_unary("string-upcase", args, |v| match v {
            Value::Str(s) => Ok(Value::Str(Rc::new(s.to_uppercase()))),
            other => Err(error(format!(
                "string-upcase expects a string, found {other}"
            ))),
        }),
        "string-downcase" => native_unary("string-downcase", args, |v| match v {
            Value::Str(s) => Ok(Value::Str(Rc::new(s.to_lowercase()))),
            other => Err(error(format!(
                "string-downcase expects a string, found {other}"
            ))),
        }),
        "char->integer" => native_unary("char->integer", args, |v| match v {
            Value::Char(c) => Ok(Value::Int(*c as i64)),
            other => Err(error(format!(
                "char->integer expects a character, found {other}"
            ))),
        }),
        "integer->char" => native_unary("integer->char", args, |v| match v {
            Value::Int(n) => u32::try_from(*n)
                .ok()
                .and_then(char::from_u32)
                .map(Value::Char)
                .ok_or_else(|| error(format!("integer->char: {n} is not a valid character code"))),
            other => Err(error(format!(
                "integer->char expects an integer, found {other}"
            ))),
        }),
        "char=?" => native_binary_predicate(
            "char=?",
            args,
            |a, b| matches!((a, b), (Value::Char(x), Value::Char(y)) if x == y),
        ),
        "char<?" => native_binary_predicate(
            "char<?",
            args,
            |a, b| matches!((a, b), (Value::Char(x), Value::Char(y)) if x < y),
        ),
        "char-alphabetic?" => native_type_predicate(
            "char-alphabetic?",
            args,
            |v| matches!(v, Value::Char(c) if c.is_alphabetic()),
        ),
        "char-numeric?" => native_type_predicate(
            "char-numeric?",
            args,
            |v| matches!(v, Value::Char(c) if c.is_numeric()),
        ),
        "char-whitespace?" => native_type_predicate(
            "char-whitespace?",
            args,
            |v| matches!(v, Value::Char(c) if c.is_whitespace()),
        ),
        "quotient" => native_quotient(args),
        "remainder" => native_remainder(args),
        "modulo" => native_modulo(args),
        "abs" => native_abs(args),
        "min" => native_min_max("min", args, |a, b| a < b),
        "max" => native_min_max("max", args, |a, b| a > b),
        "zero?" => native_numeric_predicate("zero?", args, |n| n == 0.0),
        "positive?" => native_numeric_predicate("positive?", args, |n| n > 0.0),
        "negative?" => native_numeric_predicate("negative?", args, |n| n < 0.0),
        "even?" => native_int_predicate("even?", args, |n| n % 2 == 0),
        "odd?" => native_int_predicate("odd?", args, |n| n % 2 != 0),
        "floor" => native_rounding("floor", args, f64::floor),
        "ceiling" => native_rounding("ceiling", args, f64::ceil),
        "round" => native_rounding("round", args, f64::round_ties_even),
        "truncate" => native_rounding("truncate", args, f64::trunc),
        "sqrt" => native_unary_float("sqrt", args, f64::sqrt),
        "expt" => native_expt(args),
        "exp" => native_unary_float("exp", args, f64::exp),
        "log" => native_unary_float("log", args, f64::ln),
        "sin" => native_unary_float("sin", args, f64::sin),
        "cos" => native_unary_float("cos", args, f64::cos),
        "tan" => native_unary_float("tan", args, f64::tan),
        "atan" => native_unary_float("atan", args, f64::atan),
        "number?" => native_type_predicate("number?", args, |v| {
            matches!(v, Value::Int(_) | Value::Float(_))
        }),
        "integer?" => native_type_predicate("integer?", args, |v| matches!(v, Value::Int(_))),
        "float?" => native_type_predicate("float?", args, |v| matches!(v, Value::Float(_))),
        "exact->inexact" => native_exact_to_inexact(args),
        "inexact->exact" => native_inexact_to_exact(args),
        "number->string" => native_number_to_string(args),
        "string->number" => native_string_to_number(args),
        "eq?" => native_binary_predicate("eq?", args, value_eqv),
        "eqv?" => native_binary_predicate("eqv?", args, value_eqv),
        "equal?" => native_binary_predicate("equal?", args, value_equal),
        "not" => match args {
            [v] => Ok(Value::Bool(!is_truthy(v))),
            _ => Err(error(format!(
                "not expects exactly 1 argument, got {}",
                args.len()
            ))),
        },
        "null?" => native_type_predicate("null?", args, is_null),
        "pair?" => native_type_predicate("pair?", args, is_pair),
        "list?" => native_type_predicate("list?", args, is_proper_list),
        "symbol?" => native_type_predicate("symbol?", args, |v| matches!(v, Value::Symbol(_))),
        "string?" => native_type_predicate("string?", args, |v| matches!(v, Value::Str(_))),
        "char?" => native_type_predicate("char?", args, |v| matches!(v, Value::Char(_))),
        "boolean?" => native_type_predicate("boolean?", args, |v| matches!(v, Value::Bool(_))),
        "procedure?" => native_type_predicate("procedure?", args, |v| {
            matches!(v, Value::Closure(..) | Value::Native(_))
        }),
        "vector?" => native_type_predicate("vector?", args, |v| matches!(v, Value::Vector(_))),
        "hash?" => native_type_predicate("hash?", args, |v| matches!(v, Value::Hash(_))),
        "make-hash" => match args {
            [] => Ok(Value::Hash(Rc::new(RefCell::new(Vec::new())))),
            _ => Err(error(format!(
                "make-hash expects no arguments, got {}",
                args.len()
            ))),
        },
        "vector" => Ok(Value::Vector(Rc::new(RefCell::new(args.to_vec())))),
        "make-vector" => match args {
            [Value::Int(n)] => make_vector(*n, Value::Int(0)),
            [Value::Int(n), fill] => make_vector(*n, fill.clone()),
            _ => Err(error(format!(
                "make-vector expects a length and an optional fill value, got {} argument(s)",
                args.len()
            ))),
        },
        "vector-ref" => {
            let [Value::Vector(items), Value::Int(idx)] = args else {
                return Err(error(format!(
                    "vector-ref expects a vector and an integer index, got {} argument(s)",
                    args.len()
                )));
            };
            usize::try_from(*idx)
                .ok()
                .and_then(|i| items.borrow().get(i).cloned())
                .ok_or_else(|| error(format!("vector-ref index {idx} is out of range")))
        }
        "vector-set!" => {
            let [Value::Vector(items), Value::Int(idx), v] = args else {
                return Err(error(format!(
                    "vector-set! expects a vector, an integer index, and a value, got {} argument(s)",
                    args.len()
                )));
            };
            let mut borrowed = items.borrow_mut();
            let len = borrowed.len();
            match usize::try_from(*idx).ok().filter(|&i| i < len) {
                Some(i) => {
                    borrowed[i] = v.clone();
                    Ok(Value::Unspecified)
                }
                None => Err(error(format!("vector-set! index {idx} is out of range"))),
            }
        }
        "vector-length" => native_unary("vector-length", args, |v| match v {
            Value::Vector(items) => Ok(Value::Int(items.borrow().len() as i64)),
            other => Err(error(format!(
                "vector-length expects a vector, found {other}"
            ))),
        }),
        "vector->list" => native_unary("vector->list", args, |v| match v {
            Value::Vector(items) => Ok(vec_to_list(items.borrow().clone())),
            other => Err(error(format!(
                "vector->list expects a vector, found {other}"
            ))),
        }),
        "list->vector" => native_unary("list->vector", args, |v| {
            Ok(Value::Vector(Rc::new(RefCell::new(list_to_vec(
                "list->vector",
                v,
            )?))))
        }),
        "vector-fill!" => {
            let [Value::Vector(items), v] = args else {
                return Err(error(format!(
                    "vector-fill! expects a vector and a fill value, got {} argument(s)",
                    args.len()
                )));
            };
            for slot in items.borrow_mut().iter_mut() {
                *slot = v.clone();
            }
            Ok(Value::Unspecified)
        }
        "hash-ref" => match args {
            [Value::Hash(entries), key] => find_hash_value(entries, key)
                .ok_or_else(|| error(format!("hash-ref: key {key} not found"))),
            [Value::Hash(entries), key, default] => {
                Ok(find_hash_value(entries, key).unwrap_or_else(|| default.clone()))
            }
            _ => Err(error(format!(
                "hash-ref expects a hash table, a key, and an optional default value, got {} argument(s)",
                args.len()
            ))),
        },
        "hash-set!" => {
            let [Value::Hash(entries), key, v] = args else {
                return Err(error(format!(
                    "hash-set! expects a hash table, a key, and a value, got {} argument(s)",
                    args.len()
                )));
            };
            let mut borrowed = entries.borrow_mut();
            match borrowed.iter_mut().find(|(k, _)| value_equal(k, key)) {
                Some(entry) => entry.1 = v.clone(),
                None => borrowed.push((key.clone(), v.clone())),
            }
            Ok(Value::Unspecified)
        }
        "hash-remove!" => {
            let [Value::Hash(entries), key] = args else {
                return Err(error(format!(
                    "hash-remove! expects a hash table and a key, got {} argument(s)",
                    args.len()
                )));
            };
            entries.borrow_mut().retain(|(k, _)| !value_equal(k, key));
            Ok(Value::Unspecified)
        }
        "hash-count" => native_unary("hash-count", args, |v| match v {
            Value::Hash(entries) => Ok(Value::Int(entries.borrow().len() as i64)),
            other => Err(error(format!(
                "hash-count expects a hash table, found {other}"
            ))),
        }),
        "hash-keys" => native_unary("hash-keys", args, |v| match v {
            Value::Hash(entries) => Ok(vec_to_list(
                entries.borrow().iter().map(|(k, _)| k.clone()).collect(),
            )),
            other => Err(error(format!(
                "hash-keys expects a hash table, found {other}"
            ))),
        }),
        "hash-has-key?" => match args {
            [Value::Hash(entries), key] => Ok(Value::Bool(
                entries.borrow().iter().any(|(k, _)| value_equal(k, key)),
            )),
            _ => Err(error(format!(
                "hash-has-key? expects a hash table and a key, got {} argument(s)",
                args.len()
            ))),
        },
        "read" => match args {
            [] => native_read(vm),
            _ => Err(error(format!(
                "read expects no arguments, got {}",
                args.len()
            ))),
        },
        "read-line" => match args {
            [] => native_read_line(vm),
            _ => Err(error(format!(
                "read-line expects no arguments, got {}",
                args.len()
            ))),
        },
        "eof-object?" => native_type_predicate("eof-object?", args, |v| matches!(v, Value::Eof)),
        "write" => {
            let value = args
                .first()
                .ok_or_else(|| error("write expects exactly 1 argument"))?;
            write!(out, "{}", write_repr(value)).map_err(|e| error(e.to_string()))?;
            Ok(Value::Unspecified)
        }
        other => Err(error(format!("unknown native procedure: {other}"))),
    }
}

/// A proper (finite, non-circular) list: either the empty-list `List`
/// value directly, or a `Pair` chain whose cdr eventually reaches one --
/// spec 3.7's `list?` is true only for well-formed, finite lists, not
/// improper (dotted) structures.
fn is_proper_list(v: &Value) -> bool {
    // Iterative, not recursive: a `Pair` chain is fully constructible at
    // runtime with unlimited length via ordinary `cons` (e.g. a tail-
    // recursive builder loop), so one native stack frame per element would
    // let an ordinary, non-malicious program crash the process outright
    // (warden security review, msg #144) -- the same class of bug this
    // project has already fixed for the reader and the VM's own call depth.
    //
    // Also tracks visited Pair addresses: a circular list (via set-cdr!)
    // is never finite, so `#f` is the semantically correct answer for it,
    // not just a hang-avoidance hack (warden security review, msg #147).
    let mut current = v.clone();
    let mut seen = HashSet::new();
    loop {
        match current {
            Value::List(_) => return true,
            Value::Pair(cell) => {
                if !seen.insert(Rc::as_ptr(&cell) as usize) {
                    return false;
                }
                current = cell.borrow().1.clone();
            }
            _ => return false,
        }
    }
}

/// A non-empty `List` is a proper list too (spec 5.1; see [`is_pair`]), so
/// `car`/`cdr` reach into one exactly as they would a `Pair` chain built
/// with `cons` -- which representation backs a given list is not
/// observable.
fn car_of(opname: &str, v: &Value) -> Result<Value, RuntimeError> {
    match v {
        Value::Pair(cell) => Ok(cell.borrow().0.clone()),
        Value::List(items) if !items.is_empty() => Ok(items[0].clone()),
        other => Err(error(format!("{opname} expects a pair, found {other}"))),
    }
}

fn cdr_of(opname: &str, v: &Value) -> Result<Value, RuntimeError> {
    match v {
        Value::Pair(cell) => Ok(cell.borrow().1.clone()),
        Value::List(items) if !items.is_empty() => Ok(Value::List(Rc::new(items[1..].to_vec()))),
        other => Err(error(format!("{opname} expects a pair, found {other}"))),
    }
}

/// Composes `car`/`cdr` per `ops` (e.g. `"ad"` for `cadr`), applying the
/// letter closest to the trailing `r` first -- Scheme's `cXXXr` naming
/// convention reads left to right but *evaluates* right to left, matching
/// how `(cadr x)` means `(car (cdr x))`.
fn cxr(opname: &str, ops: &str, v: &Value) -> Result<Value, RuntimeError> {
    let mut result = v.clone();
    for op in ops.chars().rev() {
        result = match op {
            'a' => car_of(opname, &result)?,
            'd' => cdr_of(opname, &result)?,
            _ => unreachable!("cxr ops string must only contain 'a'/'d'"),
        };
    }
    Ok(result)
}

fn native_unary(
    opname: &str,
    args: &[Value],
    f: impl Fn(&Value) -> Result<Value, RuntimeError>,
) -> Result<Value, RuntimeError> {
    let [a] = args else {
        return Err(error(format!(
            "{opname} expects exactly 1 argument, got {}",
            args.len()
        )));
    };
    f(a)
}

fn native_set_half(
    opname: &str,
    args: &[Value],
    set: impl Fn(&mut (Value, Value), Value),
) -> Result<Value, RuntimeError> {
    let [target, v] = args else {
        return Err(error(format!(
            "{opname} expects exactly 2 arguments, got {}",
            args.len()
        )));
    };
    match target {
        Value::Pair(cell) => {
            set(&mut cell.borrow_mut(), v.clone());
            Ok(Value::Unspecified)
        }
        other => Err(error(format!("{opname} expects a pair, found {other}"))),
    }
}

/// Flattens any proper list (a `Pair` chain terminating in the empty
/// `List`, or a `List` outright) into a plain `Vec` -- the shared traversal
/// behind every B9 list operation, so each one doesn't have to walk both
/// representations itself.
///
/// Tracks visited `Pair` addresses so a circular list (constructible via
/// `set-cdr!`) is a clean error instead of spinning forever at 100% CPU
/// (warden security review, msg #146) -- every caller here already treats
/// an ordinary non-list value as a clean error, so a circular one erroring
/// too is the same kind of boundary, not a new one.
fn list_to_vec(opname: &str, v: &Value) -> Result<Vec<Value>, RuntimeError> {
    let mut out = Vec::new();
    let mut current = v.clone();
    let mut seen = HashSet::new();
    loop {
        match current {
            Value::List(items) => {
                out.extend(items.iter().cloned());
                return Ok(out);
            }
            Value::Pair(cell) => {
                if !seen.insert(Rc::as_ptr(&cell) as usize) {
                    return Err(error(format!(
                        "{opname} expects an acyclic list, found a circular list"
                    )));
                }
                let (car, cdr) = {
                    let borrowed = cell.borrow();
                    (borrowed.0.clone(), borrowed.1.clone())
                };
                out.push(car);
                current = cdr;
            }
            other => {
                return Err(error(format!(
                    "{opname} expects a proper list, found {other}"
                )));
            }
        }
    }
}

/// Caps how many elements a single `make-vector` call can request. Without
/// this, `vec![fill; len]` hands an arbitrary `i64` straight to the
/// allocator as one contiguous up-front request with no loop or recursion
/// in source -- unlike building a comparably large structure via `cons`
/// (throttled by real per-element allocator overhead, and already bounded
/// by this project's own step/call-depth budgets), a single `make-vector`
/// call can request many gigabytes instantly. Worse, the OS killing the
/// process for that (SIGKILL) is not a catchable Rust panic, unlike this
/// project's other bounded-input guards, so it bypasses the panic-catching
/// defense in depth entirely (warden security review, msgs #191/#192).
const MAX_VECTOR_LENGTH: usize = 10_000_000;

fn make_vector(n: i64, fill: Value) -> Result<Value, RuntimeError> {
    let len = usize::try_from(n)
        .map_err(|_| error(format!("make-vector length {n} must not be negative")))?;
    if len > MAX_VECTOR_LENGTH {
        return Err(error(format!(
            "make-vector length {len} exceeds the maximum of {MAX_VECTOR_LENGTH}"
        )));
    }
    Ok(Value::Vector(Rc::new(RefCell::new(vec![fill; len]))))
}

/// Looks up `key` in a hash table's insertion-ordered entries by `equal?`
/// (spec 4.6), not by identity -- a separately-built but structurally
/// identical compound key (e.g. a list or string) must still find its
/// value, mirroring B9's `member`/`assoc` structural-equality rigor.
fn find_hash_value(entries: &Rc<RefCell<Vec<(Value, Value)>>>, key: &Value) -> Option<Value> {
    entries
        .borrow()
        .iter()
        .find(|(k, _)| value_equal(k, key))
        .map(|(_, v)| v.clone())
}

/// Reads one complete datum from standard input as DATA, not code -- the
/// result is never evaluated (spec 4.8), only parsed and converted the same
/// way a quoted literal is at compile time (`sexpr_to_const`, then
/// `const_to_value`), so `(+ 1 2)` on stdin reads back as the 3-element
/// list `(+ 1 2)`, not the number `3`. Consumes exactly one datum's worth
/// of `vm.stdin`, leaving the rest for a subsequent `read`/`read-line` call
/// to continue from.
fn native_read(vm: &mut Vm) -> Result<Value, RuntimeError> {
    // `read_one` re-tokenizes its whole input from scratch every call (it
    // has no state to resume from), so retrying it after every single new
    // line pulled in for a datum spread across N lines costs O(current
    // size) per line x O(N) lines = O(N^2) total (warden security review
    // msg #208, confirmed with measured quadratic scaling). Retrying only
    // once the buffer has roughly DOUBLED since the last attempt (rather
    // than on every new chunk) keeps the sizes actually re-parsed forming
    // a geometric series that sums to O(final size) -- linear overall,
    // dominated by the one full-size attempt at the end -- while still
    // resolving in a single attempt for the common single-chunk case
    // (the first attempt always runs immediately, threshold 0).
    let mut next_attempt_at = 0;
    loop {
        if vm.stdin_buffer.len() >= next_attempt_at {
            match reader::read_one(&vm.stdin_buffer) {
                Ok((Some(sexpr), remaining)) => {
                    vm.stdin_buffer = remaining;
                    let c = sexpr_to_const(&sexpr).map_err(|e| error(format!("read: {e}")))?;
                    return Ok(const_to_value(&c));
                }
                // Either nothing parseable yet (just whitespace so far) or
                // a genuine parse error (e.g. an unterminated list) -- both
                // are ambiguous until we know whether more input is still
                // coming, since a multi-line datum looks identical to
                // malformed input until its closing delimiter actually
                // arrives. Only once the stream is truly exhausted (below)
                // is either outcome treated as final.
                Ok((None, _)) | Err(_) => {
                    // The trailing `+ 1` is unobservable under this
                    // relay's own invariant that every `Some(chunk)` from
                    // `next_chunk` has `chunk.len() >= 1` (a true
                    // end-of-stream is `None`, never an empty `Some("")`):
                    // whether the threshold after an empty buffer's failed
                    // attempt is 0 or 1, the very next real chunk always
                    // satisfies it identically, since its length is never
                    // 0. Hand-verified: with this `+ 1` mutated away, the
                    // full test suite still passes. Kept anyway as a
                    // defensive margin against a future relay change that
                    // might relax that invariant.
                    next_attempt_at = vm.stdin_buffer.len() * 2 + 1;
                }
            }
        }
        match vm.stdin_channel.next_chunk() {
            Some(chunk) => vm.stdin_buffer.push_str(&chunk),
            None => {
                return match reader::read_one(&vm.stdin_buffer) {
                    Ok((Some(sexpr), _)) => {
                        let c = sexpr_to_const(&sexpr).map_err(|e| error(format!("read: {e}")))?;
                        Ok(const_to_value(&c))
                    }
                    Ok((None, _)) => Ok(Value::Eof),
                    Err(e) => Err(error(format!("read: {e}"))),
                };
            }
        }
    }
}

/// Reads one line from standard input as a string, with the line-ending
/// removed (spec 4.8) -- a final line with no trailing newline still reads
/// as that line's text; only a call with nothing at all left to read
/// returns the end-of-input marker.
fn native_read_line(vm: &mut Vm) -> Result<Value, RuntimeError> {
    loop {
        if let Some(i) = vm.stdin_buffer.find('\n') {
            let line = vm.stdin_buffer[..i].to_string();
            vm.stdin_buffer = vm.stdin_buffer[i + 1..].to_string();
            return Ok(Value::Str(Rc::new(line)));
        }
        match vm.stdin_channel.next_chunk() {
            Some(chunk) => vm.stdin_buffer.push_str(&chunk),
            None if vm.stdin_buffer.is_empty() => return Ok(Value::Eof),
            None => return Ok(Value::Str(Rc::new(std::mem::take(&mut vm.stdin_buffer)))),
        }
    }
}

/// Builds a proper list back out of a `Vec`, as a genuine `Pair` chain (not
/// a flat `List`) so the result supports `set-car!`/`set-cdr!` mutation
/// like any list a real Scheme program constructs.
fn vec_to_list(items: Vec<Value>) -> Value {
    let mut result = Value::List(Rc::new(Vec::new()));
    for item in items.into_iter().rev() {
        result = Value::Pair(Rc::new(RefCell::new((item, result))));
    }
    result
}

/// The final pair of a `Pair` chain (or the `List`-backed equivalent,
/// converted via [`vec_to_list`] first) -- spec 5.1 requires this stay
/// cons-shaped (holding the last element and the empty list), not just the
/// bare last element.
/// Iterative, not recursive, for the same reason as [`is_proper_list`]: a
/// `Pair` chain has no runtime length bound, so walking it one native stack
/// frame per element would crash on an ordinary long list.
fn last_pair(opname: &str, v: &Value) -> Result<Value, RuntimeError> {
    let mut current = match v {
        // The `!items.is_empty()` guard is unobservable on an empty items
        // Vec specifically: `vec_to_list(vec![])` and `other.clone()` both
        // produce an (Rc-distinct but Display-and-error-message-identical)
        // empty List, and this function never returns a List successfully
        // -- an empty input always falls through the loop below to the
        // `Err` arm either way, with the same message. Hand-verified: with
        // the guard forced to `true`, the full test suite still passes.
        Value::List(items) if !items.is_empty() => vec_to_list(items.to_vec()),
        other => other.clone(),
    };
    // Tracks visited Pair addresses: a circular list (via set-cdr!) has no
    // final pair, so this must error instead of spinning forever (warden
    // security review, msg #147).
    let mut seen = HashSet::new();
    loop {
        match current {
            Value::Pair(cell) => {
                if !seen.insert(Rc::as_ptr(&cell) as usize) {
                    return Err(error(format!(
                        "{opname} expects an acyclic list, found a circular list"
                    )));
                }
                let cdr = cell.borrow().1.clone();
                if matches!(cdr, Value::Pair(_)) {
                    current = cdr;
                } else {
                    return Ok(Value::Pair(cell));
                }
            }
            other => return Err(error(format!("{opname} expects a pair, found {other}"))),
        }
    }
}

/// Finds the first sublist of `haystack` whose car matches `needle` under
/// `matches` (spec 5.1's `member`/`memv`/`memq`, distinguished only by
/// which equality relation they search with), returning that sublist
/// directly -- or `#f` if the search runs off the end without a match.
fn native_member(
    opname: &str,
    args: &[Value],
    matches: fn(&Value, &Value) -> bool,
) -> Result<Value, RuntimeError> {
    let [needle, haystack] = args else {
        return Err(error(format!(
            "{opname} expects exactly 2 arguments, got {}",
            args.len()
        )));
    };
    let mut current = haystack.clone();
    // Tracks visited Pair addresses so a circular haystack (via set-cdr!)
    // is a clean "not found" instead of spinning forever (warden security
    // review, msg #146) -- once a pair repeats, every element reachable
    // from the haystack has already been checked against needle.
    let mut seen = HashSet::new();
    loop {
        match current {
            Value::List(items) => {
                let Some(pos) = items.iter().position(|item| matches(needle, item)) else {
                    return Ok(Value::Bool(false));
                };
                return Ok(vec_to_list(items[pos..].to_vec()));
            }
            Value::Pair(cell) => {
                if !seen.insert(Rc::as_ptr(&cell) as usize) {
                    return Ok(Value::Bool(false));
                }
                let (car, cdr) = {
                    let borrowed = cell.borrow();
                    (borrowed.0.clone(), borrowed.1.clone())
                };
                if matches(needle, &car) {
                    return Ok(Value::Pair(cell));
                }
                current = cdr;
            }
            other => {
                return Err(error(format!(
                    "{opname} expects a proper list, found {other}"
                )));
            }
        }
    }
}

/// Finds the first entry (a key/value pair) of `alist` whose key matches
/// `needle` under `matches` (spec 5.1's `assoc`/`assv`/`assq`, distinguished
/// only by which equality relation they search with), returning that whole
/// entry -- or `#f` if no key matches.
fn native_assoc(
    opname: &str,
    args: &[Value],
    matches: fn(&Value, &Value) -> bool,
) -> Result<Value, RuntimeError> {
    let [needle, alist] = args else {
        return Err(error(format!(
            "{opname} expects exactly 2 arguments, got {}",
            args.len()
        )));
    };
    for entry in list_to_vec(opname, alist)? {
        let key = car_of(opname, &entry)?;
        if matches(needle, &key) {
            return Ok(entry);
        }
    }
    Ok(Value::Bool(false))
}

/// Flattens `map`/`for-each`'s trailing list arguments into one `Vec` of
/// equal-length `Vec<Value>` rows, erroring if any of them differ in
/// length -- both natives call an N-ary procedure once per position across
/// all of them in parallel, so a length mismatch has no sensible pairing.
fn parallel_list_rows(opname: &str, lists: &[Value]) -> Result<Vec<Vec<Value>>, RuntimeError> {
    let columns = lists
        .iter()
        .map(|l| list_to_vec(opname, l))
        .collect::<Result<Vec<_>, _>>()?;
    let len = columns[0].len();
    if columns.iter().any(|c| c.len() != len) {
        return Err(error(format!(
            "{opname} requires all list arguments to have the same length"
        )));
    }
    Ok((0..len)
        .map(|i| columns.iter().map(|c| c[i].clone()).collect())
        .collect())
}

/// Applies `proc` to corresponding elements of one or more equal-length
/// lists in parallel, collecting the results into a new list (spec 5.1).
fn native_map(vm: &mut Vm, args: &[Value], out: &mut impl Write) -> Result<Value, RuntimeError> {
    let [proc, lists @ ..] = args else {
        return Err(error("map expects a procedure and at least one list"));
    };
    if lists.is_empty() {
        return Err(error("map expects at least one list argument"));
    }
    let rows = parallel_list_rows("map", lists)?;
    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        results.push(vm.call_value(proc, row, out)?);
    }
    Ok(vec_to_list(results))
}

/// Like [`native_map`], but for the side-effect-only variant: calls `proc`
/// for its effects and discards every result, itself evaluating to
/// `Unspecified` rather than a transformed list (spec 5.1).
fn native_for_each(
    vm: &mut Vm,
    args: &[Value],
    out: &mut impl Write,
) -> Result<Value, RuntimeError> {
    let [proc, lists @ ..] = args else {
        return Err(error("for-each expects a procedure and at least one list"));
    };
    if lists.is_empty() {
        return Err(error("for-each expects at least one list argument"));
    }
    for row in parallel_list_rows("for-each", lists)? {
        vm.call_value(proc, row, out)?;
    }
    Ok(Value::Unspecified)
}

/// Keeps only the elements of `list` for which `proc` returns a truthy
/// value (spec 5.1).
fn native_filter(vm: &mut Vm, args: &[Value], out: &mut impl Write) -> Result<Value, RuntimeError> {
    let [proc, list] = args else {
        return Err(error(format!(
            "filter expects exactly 2 arguments, got {}",
            args.len()
        )));
    };
    let mut results = Vec::new();
    for item in list_to_vec("filter", list)? {
        if is_truthy(&vm.call_value(proc, vec![item.clone()], out)?) {
            results.push(item);
        }
    }
    Ok(vec_to_list(results))
}

/// Folds `list` left to right, calling `proc` as `(proc acc elem)` starting
/// from `init` (spec 5.1) -- evaluation order matters for non-commutative
/// `proc`, unlike [`native_fold_right`].
fn native_fold_left(
    vm: &mut Vm,
    args: &[Value],
    out: &mut impl Write,
) -> Result<Value, RuntimeError> {
    let [proc, init, list] = args else {
        return Err(error(format!(
            "fold-left expects exactly 3 arguments, got {}",
            args.len()
        )));
    };
    let mut acc = init.clone();
    for item in list_to_vec("fold-left", list)? {
        acc = vm.call_value(proc, vec![acc, item], out)?;
    }
    Ok(acc)
}

/// Folds `list` right to left, calling `proc` as `(proc elem acc)` starting
/// from `init` (spec 5.1) -- e.g. `(fold-right cons '() lst)` rebuilds
/// `lst` in its original order.
fn native_fold_right(
    vm: &mut Vm,
    args: &[Value],
    out: &mut impl Write,
) -> Result<Value, RuntimeError> {
    let [proc, init, list] = args else {
        return Err(error(format!(
            "fold-right expects exactly 3 arguments, got {}",
            args.len()
        )));
    };
    let mut acc = init.clone();
    for item in list_to_vec("fold-right", list)?.into_iter().rev() {
        acc = vm.call_value(proc, vec![item, acc], out)?;
    }
    Ok(acc)
}

/// A self-seeded left-fold (spec 5.1): uses the list's own first element as
/// the seed instead of `init`, falling back to `init` only when the list is
/// empty (there's no element to seed from).
fn native_reduce(vm: &mut Vm, args: &[Value], out: &mut impl Write) -> Result<Value, RuntimeError> {
    let [proc, init, list] = args else {
        return Err(error(format!(
            "reduce expects exactly 3 arguments, got {}",
            args.len()
        )));
    };
    let items = list_to_vec("reduce", list)?;
    let Some((first, rest)) = items.split_first() else {
        return Ok(init.clone());
    };
    let mut acc = first.clone();
    for item in rest {
        acc = vm.call_value(proc, vec![acc, item.clone()], out)?;
    }
    Ok(acc)
}

/// Calls `proc` with `direct` arguments plus a final trailing list
/// flattened into one argument set (spec 5.1) -- `apply` always requires
/// that trailing list, even when it's empty.
fn native_apply(vm: &mut Vm, args: &[Value], out: &mut impl Write) -> Result<Value, RuntimeError> {
    let [proc, rest @ ..] = args else {
        return Err(error("apply expects a procedure and a trailing list"));
    };
    let Some((trailing, direct)) = rest.split_last() else {
        return Err(error("apply expects a trailing list argument"));
    };
    let mut call_args = direct.to_vec();
    call_args.extend(list_to_vec("apply", trailing)?);
    vm.call_value(proc, call_args, out)
}

/// Counts by displayed character, not by underlying UTF-8 byte -- a
/// multi-byte character still counts as exactly one position (spec 6.1's
/// own BOUNDARIES: internal string encoding isn't observable).
fn native_string_length(args: &[Value]) -> Result<Value, RuntimeError> {
    native_unary("string-length", args, |v| match v {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        other => Err(error(format!(
            "string-length expects a string, found {other}"
        ))),
    })
}

fn native_string_ref(args: &[Value]) -> Result<Value, RuntimeError> {
    let [Value::Str(s), Value::Int(idx)] = args else {
        return Err(error(format!(
            "string-ref expects a string and an integer index, got {} argument(s)",
            args.len()
        )));
    };
    usize::try_from(*idx)
        .ok()
        .and_then(|i| s.chars().nth(i))
        .map(Value::Char)
        .ok_or_else(|| error(format!("string-ref index {idx} is out of range")))
}

/// `start` inclusive, `end` exclusive, both counted by character.
fn native_substring(args: &[Value]) -> Result<Value, RuntimeError> {
    let [Value::Str(s), Value::Int(start), Value::Int(end)] = args else {
        return Err(error(format!(
            "substring expects a string and two integer indices, got {} argument(s)",
            args.len()
        )));
    };
    let (start, end) = match (usize::try_from(*start), usize::try_from(*end)) {
        (Ok(start), Ok(end)) if start <= end => (start, end),
        _ => {
            return Err(error(format!("substring range {start}..{end} is invalid")));
        }
    };
    let chars: Vec<char> = s.chars().collect();
    chars
        .get(start..end)
        .map(|slice| Value::Str(Rc::new(slice.iter().collect())))
        .ok_or_else(|| error(format!("substring range {start}..{end} is out of bounds")))
}

fn native_string_append(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(error(format!(
            "string-append expects at least 2 arguments, got {}",
            args.len()
        )));
    }
    let mut result = String::new();
    for arg in args {
        match arg {
            Value::Str(s) => result.push_str(s),
            other => {
                return Err(error(format!(
                    "string-append expects a string, found {other}"
                )));
            }
        }
    }
    Ok(Value::Str(Rc::new(result)))
}

fn native_string_compare(
    opname: &str,
    args: &[Value],
    holds: fn(&str, &str) -> bool,
) -> Result<Value, RuntimeError> {
    let [Value::Str(a), Value::Str(b)] = args else {
        return Err(error(format!(
            "{opname} expects two strings, got {} argument(s)",
            args.len()
        )));
    };
    Ok(Value::Bool(holds(a, b)))
}

fn native_symbol_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    native_unary("symbol->string", args, |v| match v {
        Value::Symbol(s) => Ok(Value::Str(Rc::new(s.clone()))),
        other => Err(error(format!(
            "symbol->string expects a symbol, found {other}"
        ))),
    })
}

fn native_string_to_symbol(args: &[Value]) -> Result<Value, RuntimeError> {
    native_unary("string->symbol", args, |v| match v {
        Value::Str(s) => Ok(Value::Symbol(s.as_str().to_string())),
        other => Err(error(format!(
            "string->symbol expects a string, found {other}"
        ))),
    })
}

fn native_list_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    native_unary("list->string", args, |v| {
        let mut s = String::new();
        for item in list_to_vec("list->string", v)? {
            match item {
                Value::Char(c) => s.push(c),
                other => {
                    return Err(error(format!(
                        "list->string expects a list of characters, found {other}"
                    )));
                }
            }
        }
        Ok(Value::Str(Rc::new(s)))
    })
}

fn native_string_to_list(args: &[Value]) -> Result<Value, RuntimeError> {
    native_unary("string->list", args, |v| match v {
        Value::Str(s) => Ok(vec_to_list(s.chars().map(Value::Char).collect())),
        other => Err(error(format!(
            "string->list expects a string, found {other}"
        ))),
    })
}

/// A non-empty list is, per real Scheme semantics, built from pairs -- so
/// it counts as a pair too, even though this codebase's `Value::List`
/// keeps a flat internal representation rather than an actual pair chain
/// (an internal detail this behaviour's own BOUNDARIES says isn't
/// observable).
fn is_pair(v: &Value) -> bool {
    match v {
        Value::Pair(_) => true,
        Value::List(items) => !items.is_empty(),
        _ => false,
    }
}

fn is_null(v: &Value) -> bool {
    matches!(v, Value::List(items) if items.is_empty())
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
        // checked_rem and checked_div overflow in lockstep (both None only
        // for this same i64::MIN/-1 pair), so checked_rem returning Some(_)
        // here already proves plain `/` cannot overflow on these operands.
        Some(0) => Ok(IntDivStep::Exact(acc / divisor)),
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

/// i64::MIN / -1 overflows in two's complement (the true magnitude exceeds
/// i64::MAX); Rust panics on plain `/` for that one input. Its true
/// mathematical quotient has no positive i64 representation, so wrapping
/// (back to i64::MIN itself) is this codebase's established convention for
/// this exact edge case — native_minus's unary negation and native_divide's
/// int_div_step both already do the same.
fn truncating_div(a: i64, b: i64) -> i64 {
    a.checked_div(b).unwrap_or_else(|| a.wrapping_neg())
}

/// i64::MIN % -1 mathematically IS 0 (MIN is evenly divisible by -1), but
/// Rust's `%` panics on this input too, sharing `/`'s overflow trap.
/// checked_rem returning None here is exactly (and only) this one case.
fn truncating_rem(a: i64, b: i64) -> i64 {
    a.checked_rem(b).unwrap_or(0)
}

fn two_ints(opname: &str, args: &[Value]) -> Result<(i64, i64), RuntimeError> {
    if args.len() != 2 {
        return Err(error(format!(
            "{opname} expects exactly 2 arguments, got {}",
            args.len()
        )));
    }
    let ints = to_ints(opname, args)?;
    Ok((ints[0], ints[1]))
}

fn native_quotient(args: &[Value]) -> Result<Value, RuntimeError> {
    let (a, b) = two_ints("quotient", args)?;
    if b == 0 {
        return Err(error("quotient by zero is a runtime error"));
    }
    Ok(Value::Int(truncating_div(a, b)))
}

fn native_remainder(args: &[Value]) -> Result<Value, RuntimeError> {
    let (a, b) = two_ints("remainder", args)?;
    if b == 0 {
        return Err(error("remainder by zero is a runtime error"));
    }
    Ok(Value::Int(truncating_rem(a, b)))
}

fn native_modulo(args: &[Value]) -> Result<Value, RuntimeError> {
    let (a, b) = two_ints("modulo", args)?;
    if b == 0 {
        return Err(error("modulo by zero is a runtime error"));
    }
    let r = truncating_rem(a, b);
    // Truncated remainder already has the quotient's sign baked in;
    // floored modulo instead follows the DIVISOR's sign, so a nonzero
    // remainder whose sign disagrees with the divisor needs nudging by one
    // divisor-width to floor it correctly.
    //
    // Both `<` comparisons below are guarded elsewhere on this same path
    // (`r != 0` right here; `b == 0` already rejected above), so mutating
    // either to `<=` is a provably equivalent, unobservable change -- a
    // `<=`, at the one value (0) it newly admits, can never actually be
    // reached. Hand-verified: the full suite still passes with either `<=`
    // mutation manually applied. Mutating `(b < 0)` to `(b == 0)`, by
    // contrast, IS observable (e.g. `(modulo 7 -2)` must be -1, not 1) and
    // is covered by a dedicated test.
    Ok(Value::Int(if r != 0 && (r < 0) != (b < 0) {
        r + b
    } else {
        r
    }))
}

fn native_abs(args: &[Value]) -> Result<Value, RuntimeError> {
    match args {
        // wrapping_abs, not abs: i64::MIN's true magnitude has no positive
        // i64 representation, same overflow class this file always handles
        // by wrapping rather than panicking.
        [Value::Int(n)] => Ok(Value::Int(n.wrapping_abs())),
        [Value::Float(n)] => Ok(Value::Float(n.abs())),
        [other] => Err(error(format!("abs expects a number, found {other}"))),
        _ => Err(error(format!(
            "abs expects exactly 1 argument, got {}",
            args.len()
        ))),
    }
}

/// Shared by `min`/`max`: `is_more_extreme(candidate, current_best)` reports
/// whether `candidate` should replace `current_best` (`<` for min, `>` for
/// max). Preserves the WINNING argument's own exactness — an all-integer
/// call returns an exact integer, matching `+`/`-`/`*`'s promotion rule
/// (float only if at least one argument is a float) rather than always
/// returning a float from the internal f64 comparison.
fn native_min_max(
    opname: &str,
    args: &[Value],
    is_more_extreme: fn(f64, f64) -> bool,
) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(error(format!("{opname} requires at least 1 argument")));
    }
    let nums = to_f64s(opname, args)?;
    let mut best = 0;
    for (i, &n) in nums.iter().enumerate().skip(1) {
        if is_more_extreme(n, nums[best]) {
            best = i;
        }
    }
    if any_float(args) {
        Ok(Value::Float(nums[best]))
    } else {
        Ok(args[best].clone())
    }
}

fn native_numeric_predicate(
    opname: &str,
    args: &[Value],
    holds: fn(f64) -> bool,
) -> Result<Value, RuntimeError> {
    let [a] = args else {
        return Err(error(format!(
            "{opname} expects exactly 1 argument, got {}",
            args.len()
        )));
    };
    let nums = to_f64s(opname, std::slice::from_ref(a))?;
    Ok(Value::Bool(holds(nums[0])))
}

fn native_int_predicate(
    opname: &str,
    args: &[Value],
    holds: fn(i64) -> bool,
) -> Result<Value, RuntimeError> {
    let [a] = args else {
        return Err(error(format!(
            "{opname} expects exactly 1 argument, got {}",
            args.len()
        )));
    };
    let ints = to_ints(opname, std::slice::from_ref(a))?;
    Ok(Value::Bool(holds(ints[0])))
}

/// Shared by floor/ceiling/round/truncate: "float in, float out ... on
/// fixnums, identity" (spec 4.1) -- an integer argument passes through
/// completely unchanged (not promoted to a float), since it's already
/// exactly its own floor/ceiling/round/truncate.
fn native_rounding(
    opname: &str,
    args: &[Value],
    round: fn(f64) -> f64,
) -> Result<Value, RuntimeError> {
    match args {
        [n @ Value::Int(_)] => Ok(n.clone()),
        [Value::Float(n)] => Ok(Value::Float(round(*n))),
        [other] => Err(error(format!("{opname} expects a number, found {other}"))),
        _ => Err(error(format!(
            "{opname} expects exactly 1 argument, got {}",
            args.len()
        ))),
    }
}

fn native_unary_float(
    opname: &str,
    args: &[Value],
    f: fn(f64) -> f64,
) -> Result<Value, RuntimeError> {
    let [a] = args else {
        return Err(error(format!(
            "{opname} expects exactly 1 argument, got {}",
            args.len()
        )));
    };
    let nums = to_f64s(opname, std::slice::from_ref(a))?;
    Ok(Value::Float(f(nums[0])))
}

/// An integer base raised to a non-negative integer exponent is exact
/// (spec 4.1); every other combination (a negative exponent, or either
/// operand already a float) produces a float via plain floating-point
/// exponentiation.
fn native_expt(args: &[Value]) -> Result<Value, RuntimeError> {
    match args {
        [Value::Int(base), Value::Int(exp)] if *exp >= 0 => {
            let exp: u32 = (*exp).try_into().unwrap_or(u32::MAX);
            Ok(Value::Int(base.wrapping_pow(exp)))
        }
        [_, _] => {
            let nums = to_f64s("expt", args)?;
            Ok(Value::Float(nums[0].powf(nums[1])))
        }
        _ => Err(error(format!(
            "expt expects exactly 2 arguments, got {}",
            args.len()
        ))),
    }
}

fn native_type_predicate(
    opname: &str,
    args: &[Value],
    holds: fn(&Value) -> bool,
) -> Result<Value, RuntimeError> {
    let [a] = args else {
        return Err(error(format!(
            "{opname} expects exactly 1 argument, got {}",
            args.len()
        )));
    };
    Ok(Value::Bool(holds(a)))
}

fn native_exact_to_inexact(args: &[Value]) -> Result<Value, RuntimeError> {
    match args {
        [Value::Int(n)] => Ok(Value::Float(*n as f64)),
        [Value::Float(n)] => Ok(Value::Float(*n)),
        [other] => Err(error(format!(
            "exact->inexact expects a number, found {other}"
        ))),
        _ => Err(error(format!(
            "exact->inexact expects exactly 1 argument, got {}",
            args.len()
        ))),
    }
}

fn native_inexact_to_exact(args: &[Value]) -> Result<Value, RuntimeError> {
    match args {
        [Value::Int(n)] => Ok(Value::Int(*n)),
        [Value::Float(n)] => {
            // Rejecting only non-finite input isn't enough: a merely-large
            // but still-finite float outside i64's representable range
            // would silently saturate to i64::MAX/MIN via `as i64` (a
            // value bearing no numerical relationship to the input)
            // instead of erroring, contradicting the whole point of this
            // guard (out-of-domain input is a clean error, not silent
            // garbage).
            //
            // The valid range is the half-open [i64::MIN, 2^63). Comparing
            // directly against `i64::MAX as f64` is a trap: i64::MAX
            // (2^63 - 1) isn't exactly representable as an f64, so that
            // cast silently rounds UP to 2^63 -- one past the true
            // boundary, which would wrongly accept 2^63 itself (an
            // out-of-range value that `as i64` would then saturate). Using
            // `-(i64::MIN as f64)` instead is exact: negating an exactly
            // representable power-of-two-magnitude value is itself exact,
            // and it equals the true 2^63 boundary directly, with no
            // rounding involved.
            let truncated = n.trunc();
            let min = i64::MIN as f64;
            let exclusive_max = -min; // 2^63, exactly -- see above
            if !(truncated >= min && truncated < exclusive_max) {
                return Err(error(format!(
                    "inexact->exact requires a number representable as an exact integer, found {}",
                    Value::Float(*n)
                )));
            }
            Ok(Value::Int(truncated as i64))
        }
        [other] => Err(error(format!(
            "inexact->exact expects a number, found {other}"
        ))),
        _ => Err(error(format!(
            "inexact->exact expects exactly 1 argument, got {}",
            args.len()
        ))),
    }
}

fn native_number_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    match args {
        [n @ (Value::Int(_) | Value::Float(_))] => Ok(Value::Str(Rc::new(n.to_string()))),
        [other] => Err(error(format!(
            "number->string expects a number, found {other}"
        ))),
        _ => Err(error(format!(
            "number->string expects exactly 1 argument, got {}",
            args.len()
        ))),
    }
}

/// Reuses the reader's own numeric-literal grammar rather than
/// reimplementing it: valid input parses to exactly one Int/Float Sexpr;
/// anything else (a read error, zero or multiple tokens, or a token that
/// parses but isn't a number) is `#f` per spec, not an error.
fn native_string_to_number(args: &[Value]) -> Result<Value, RuntimeError> {
    let s = match args {
        [Value::Str(s)] => s,
        [other] => {
            return Err(error(format!(
                "string->number expects a string, found {other}"
            )));
        }
        _ => {
            return Err(error(format!(
                "string->number expects exactly 1 argument, got {}",
                args.len()
            )));
        }
    };
    match crate::reader::read_program(s) {
        Ok(forms) => match forms.as_slice() {
            [Sexpr::Int(n)] => Ok(Value::Int(*n)),
            [Sexpr::Float(n)] => Ok(Value::Float(*n)),
            _ => Ok(Value::Bool(false)),
        },
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn native_binary_predicate(
    opname: &str,
    args: &[Value],
    holds: fn(&Value, &Value) -> bool,
) -> Result<Value, RuntimeError> {
    let [a, b] = args else {
        return Err(error(format!(
            "{opname} expects exactly 2 arguments, got {}",
            args.len()
        )));
    };
    Ok(Value::Bool(holds(a, b)))
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

    // --- upvalue error paths (qa test-design review, msg #87/#89): these
    // three defensive checks in resolve_env/upvalue_cell are only ever
    // reachable via a hand-crafted .mlbc artifact (the compiler never emits
    // an out-of-range depth/slot itself), but per this codebase's own
    // established precedent — hand-built Chunks already exercise
    // undefined-opcode and out-of-range-constant-index the same way —
    // every guarded invariant gets a direct test, not just a code comment
    // asserting it's safe. Tested as plain unit calls against Env directly,
    // since resolve_env/upvalue_cell need no VM/Chunk machinery at all.

    #[test]
    fn resolve_env_rejects_depth_zero() {
        let env = Env {
            locals: vec![],
            parent: None,
        };
        assert!(resolve_env(&env, 0).is_err());
    }

    #[test]
    fn resolve_env_rejects_a_depth_exceeding_the_captured_chain() {
        // A single-level (parentless) environment can only satisfy depth 1;
        // depth 2 would need to walk one parent link that doesn't exist.
        let env = Env {
            locals: vec![],
            parent: None,
        };
        assert!(resolve_env(&env, 2).is_err());
    }

    #[test]
    fn upvalue_cell_rejects_a_missing_captured_environment() {
        assert!(upvalue_cell(None, 1, 0).is_err());
    }

    #[test]
    fn upvalue_cell_rejects_an_out_of_range_slot() {
        let env = Rc::new(Env {
            locals: vec![],
            parent: None,
        });
        assert!(upvalue_cell(Some(&env), 1, 0).is_err());
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

    fn eval_with_stdin(src: &str, stdin: &str) -> Result<String, RuntimeError> {
        let forms = read_program(src).expect("valid source for this test");
        let module = compile_program(&forms).expect("compilable source for this test");
        let mut out = Vec::new();
        let mut input = stdin.as_bytes();
        run_with_stdin(&module, &mut out, &mut input)?;
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

    // These count-down tests wrap the recursive call as `(+ 0 (count-down
    // ...))` rather than calling it directly in tail position: since B6
    // gave the compiler tail-call optimization, a bare tail self-call would
    // compile to Op::TailCall and run in O(1) native stack regardless of
    // depth, never touching call_depth at all -- defeating the point of
    // these tests. Wrapping the call as a `+` argument keeps it in
    // non-tail position, so it still compiles to a real, stack-consuming
    // Op::Call and genuinely exercises the call_depth guard.

    #[test]
    fn recursion_up_to_the_call_depth_limit_still_succeeds() {
        // MAX_CALL_DEPTH - 1 recursive steps plus the initial call is
        // exactly MAX_CALL_DEPTH nested Value::Closure calls, the last
        // one landing on call_depth == MAX_CALL_DEPTH - 1 (checked against
        // the limit *before* incrementing), so this is the deepest
        // recursion the guard is supposed to still allow.
        let src = format!(
            "(define (count-down n) (if (= n 0) 0 (+ 0 (count-down (- n 1))))) \
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
            "(define (count-down n) (if (= n 0) 0 (+ 0 (count-down (- n 1))))) \
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
            "(define (count-down n) (if (= n 0) 0 (+ 0 (count-down (- n 1))))) \
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
        // As in the tests above, the recursive call is wrapped in `(+ 0 ...)`
        // to keep it out of tail position -- otherwise B6's tail-call
        // optimization would compile it to a stack-reusing Op::TailCall
        // that never touches call_depth at all.
        let forms =
            read_program("(define (count-down n) (if (= n 0) 0 (+ 0 (count-down (- n 1)))))")
                .unwrap();
        let module = compile_program(&forms).unwrap();
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(VM_STACK_SIZE)
                .spawn_scoped(scope, || {
                    let mut vm = Vm {
                        module: &module,
                        globals: default_globals(),
                        call_depth: 0,
                        stdin_buffer: String::new(),
                        stdin_channel: StdinChannel::none(),
                    };
                    let mut out = Vec::new();
                    let entry = &module.functions[module.entry_index as usize];
                    vm.exec(entry, Vec::new(), None, &mut out).unwrap();
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
    fn a_tail_call_with_exactly_argc_items_and_no_callee_underneath_is_a_clean_error_not_a_panic() {
        // Same shape and same reasoning as the CALL test just above, but for
        // Op::TailCall's own, separately-implemented underflow check (a
        // mutation-testing gap: TailCall's `stack.len() < argc + 1` guard is
        // never exercised by any compiler-emitted program, since the
        // compiler only ever emits it with a well-formed stack -- only a
        // hand-built chunk like this one can drive it).
        use crate::bytecode::Const;
        let mut chunk = Chunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_tail_call(1);
        chunk.emit_halt();
        let mut out = Vec::new();
        let err = run(&module_of(chunk), &mut out).unwrap_err();
        assert!(
            err.message.contains("stack underflow during TAIL_CALL"),
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

    // --- B7 E1: quotient/remainder/modulo ---

    #[test]
    fn quotient_truncates_toward_zero() {
        assert_eq!(eval("(display (quotient 7 2))").unwrap(), "3");
    }

    #[test]
    fn quotient_truncates_toward_zero_for_a_negative_dividend() {
        assert_eq!(eval("(display (quotient -7 2))").unwrap(), "-3");
    }

    #[test]
    fn remainder_truncates_toward_zero() {
        assert_eq!(eval("(display (remainder 7 2))").unwrap(), "1");
    }

    #[test]
    fn remainder_follows_the_dividends_sign_not_the_divisors() {
        // Truncated remainder: the sign of the result matches the DIVIDEND
        // (-7), distinguishing it from modulo's floored result below on the
        // exact same inputs.
        assert_eq!(eval("(display (remainder -7 2))").unwrap(), "-1");
    }

    #[test]
    fn modulo_is_floored_and_differs_from_remainder_on_a_negative_dividend() {
        // Same inputs as remainder_follows_the_dividends_sign_not_the_divisors
        // (-7, 2): remainder gives -1 (truncated, sign of dividend), modulo
        // gives 1 (floored, sign of divisor) -- this is the floor-vs-
        // truncate distinction actually being exercised, not just each
        // operation checked in isolation.
        assert_eq!(eval("(display (modulo -7 2))").unwrap(), "1");
    }

    #[test]
    fn modulo_matches_remainder_when_signs_already_agree() {
        assert_eq!(eval("(display (modulo 7 2))").unwrap(), "1");
    }

    #[test]
    fn modulo_is_floored_and_differs_from_remainder_on_a_negative_divisor() {
        // The mirror image of modulo_is_floored_and_differs_from_remainder_
        // on_a_negative_dividend above: here the DIVIDEND is positive and
        // the DIVISOR is negative. Truncated remainder(7, -2) is 1 (sign of
        // dividend); floored modulo follows the DIVISOR's sign instead, so
        // it must nudge by one divisor-width to -1.
        assert_eq!(eval("(display (remainder 7 -2))").unwrap(), "1");
        assert_eq!(eval("(display (modulo 7 -2))").unwrap(), "-1");
    }

    #[test]
    fn quotient_by_zero_is_a_runtime_error() {
        assert!(eval("(display (quotient 7 0))").is_err());
    }

    #[test]
    fn remainder_by_zero_is_a_runtime_error() {
        assert!(eval("(display (remainder 7 0))").is_err());
    }

    #[test]
    fn modulo_by_zero_is_a_runtime_error() {
        assert!(eval("(display (modulo 7 0))").is_err());
    }

    // --- B7 E2: abs/min/max/zero?/positive?/negative?/even?/odd? ---

    #[test]
    fn abs_of_a_negative_integer_is_positive() {
        assert_eq!(eval("(display (abs -5))").unwrap(), "5");
    }

    #[test]
    fn abs_of_a_positive_integer_is_unchanged() {
        assert_eq!(eval("(display (abs 5))").unwrap(), "5");
    }

    #[test]
    fn abs_of_a_negative_float_is_positive() {
        assert_eq!(eval("(display (abs -5.5))").unwrap(), "5.5");
    }

    #[test]
    fn abs_of_a_non_number_is_a_runtime_error_naming_the_bad_value() {
        // Distinguishes the "wrong type" error path from the (differently
        // worded) "wrong argument count" one -- both are reachable with
        // exactly one argument, so a mutant collapsing them together
        // wouldn't be caught by an is_err()-only check on either alone.
        let err = eval("(display (abs \"x\"))").unwrap_err();
        assert!(
            err.message.contains("expects a number"),
            "expected a wrong-type error, got: {}",
            err.message
        );
    }

    #[test]
    fn min_of_two_arguments() {
        assert_eq!(eval("(display (min 3 1))").unwrap(), "1");
    }

    #[test]
    fn min_of_more_than_two_arguments() {
        assert_eq!(eval("(display (min 5 1 3 2))").unwrap(), "1");
    }

    #[test]
    fn max_of_two_arguments() {
        assert_eq!(eval("(display (max 1 5))").unwrap(), "5");
    }

    #[test]
    fn max_of_more_than_two_arguments() {
        assert_eq!(eval("(display (max 1 5 3))").unwrap(), "5");
    }

    #[test]
    fn min_keeps_the_first_seen_candidate_on_an_exact_tie() {
        // 0.0 and -0.0 compare equal under `<`/`>` but display distinctly
        // (value.rs already establishes this distinction is observable) --
        // exactly the tool needed to prove min keeps the FIRST-seen tied
        // candidate, not the last, which is what distinguishes a strict `<`
        // comparison from a `<=` one that would let a later tie silently
        // overwrite the winner (both produce the same result on any
        // non-tied input, so only a tie can catch this).
        assert_eq!(eval("(display (min 0.0 -0.0))").unwrap(), "0.0");
    }

    #[test]
    fn max_keeps_the_first_seen_candidate_on_an_exact_tie() {
        assert_eq!(eval("(display (max 0.0 -0.0))").unwrap(), "0.0");
    }

    #[test]
    fn zero_predicate_is_true_for_zero() {
        assert_eq!(eval("(display (zero? 0))").unwrap(), "#t");
    }

    #[test]
    fn zero_predicate_is_false_for_a_nonzero_value() {
        assert_eq!(eval("(display (zero? 1))").unwrap(), "#f");
    }

    #[test]
    fn positive_predicate_is_true_for_a_positive_value() {
        assert_eq!(eval("(display (positive? 1))").unwrap(), "#t");
    }

    #[test]
    fn positive_predicate_is_false_for_a_negative_value() {
        assert_eq!(eval("(display (positive? -1))").unwrap(), "#f");
    }

    #[test]
    fn positive_predicate_is_false_for_zero() {
        assert_eq!(eval("(display (positive? 0))").unwrap(), "#f");
    }

    #[test]
    fn negative_predicate_is_true_for_a_negative_value() {
        assert_eq!(eval("(display (negative? -1))").unwrap(), "#t");
    }

    #[test]
    fn negative_predicate_is_false_for_zero() {
        assert_eq!(eval("(display (negative? 0))").unwrap(), "#f");
    }

    #[test]
    fn negative_predicate_is_false_for_a_positive_value() {
        assert_eq!(eval("(display (negative? 1))").unwrap(), "#f");
    }

    #[test]
    fn even_predicate_is_true_for_an_even_number() {
        assert_eq!(eval("(display (even? 10))").unwrap(), "#t");
    }

    #[test]
    fn even_predicate_is_false_for_an_odd_number() {
        assert_eq!(eval("(display (even? 3))").unwrap(), "#f");
    }

    #[test]
    fn odd_predicate_is_true_for_an_odd_number() {
        assert_eq!(eval("(display (odd? 3))").unwrap(), "#t");
    }

    #[test]
    fn odd_predicate_is_false_for_an_even_number() {
        assert_eq!(eval("(display (odd? 10))").unwrap(), "#f");
    }

    // --- B7 E3: floor/ceiling/round/truncate ---

    #[test]
    fn floor_of_a_positive_fraction_rounds_down() {
        assert_eq!(eval("(display (floor 2.7))").unwrap(), "2.0");
    }

    #[test]
    fn ceiling_of_a_positive_fraction_rounds_up() {
        assert_eq!(eval("(display (ceiling 2.7))").unwrap(), "3.0");
    }

    #[test]
    fn truncate_of_a_positive_fraction_drops_the_fraction() {
        assert_eq!(eval("(display (truncate 2.7))").unwrap(), "2.0");
    }

    #[test]
    fn floor_ceiling_truncate_round_all_differ_on_a_negative_fraction() {
        // -2.7: floor rounds down to -3 (away from zero); ceiling rounds up
        // to -2 (toward zero); truncate drops the fraction, also landing on
        // -2; round goes to the NEAREST integer, -3 -- a different pairing
        // than the positive-fraction case above, proving these are four
        // genuinely distinct operations, not two functions under four names.
        assert_eq!(eval("(display (floor -2.7))").unwrap(), "-3.0");
        assert_eq!(eval("(display (ceiling -2.7))").unwrap(), "-2.0");
        assert_eq!(eval("(display (truncate -2.7))").unwrap(), "-2.0");
        assert_eq!(eval("(display (round -2.7))").unwrap(), "-3.0");
    }

    #[test]
    fn round_of_two_point_five_rounds_to_the_even_neighbor() {
        assert_eq!(eval("(display (round 2.5))").unwrap(), "2.0");
    }

    #[test]
    fn round_of_three_point_five_rounds_to_the_even_neighbor() {
        assert_eq!(eval("(display (round 3.5))").unwrap(), "4.0");
    }

    #[test]
    fn floor_ceiling_round_truncate_are_identity_on_a_whole_number_input() {
        // Fixnums pass through unchanged -- NOT promoted to a float, per
        // spec's "float in, float out ... on fixnums, identity".
        assert_eq!(eval("(display (floor 5))").unwrap(), "5");
        assert_eq!(eval("(display (ceiling 5))").unwrap(), "5");
        assert_eq!(eval("(display (round 5))").unwrap(), "5");
        assert_eq!(eval("(display (truncate 5))").unwrap(), "5");
    }

    #[test]
    fn floor_of_a_non_number_is_a_runtime_error_naming_the_bad_value() {
        // Same wrong-type-vs-wrong-count distinction as abs's equivalent
        // test above, for native_rounding's own separately-implemented
        // error paths.
        let err = eval("(display (floor \"x\"))").unwrap_err();
        assert!(
            err.message.contains("expects a number"),
            "expected a wrong-type error, got: {}",
            err.message
        );
    }

    // --- B7 E4: sqrt/expt/exp/log/sin/cos/tan/atan ---

    #[test]
    fn sqrt_of_a_perfect_square_is_still_a_float() {
        assert_eq!(eval("(display (sqrt 4))").unwrap(), "2.0");
    }

    #[test]
    fn expt_with_an_integer_base_and_a_nonnegative_integer_exponent_is_exact() {
        assert_eq!(eval("(display (expt 2 10))").unwrap(), "1024");
    }

    #[test]
    fn expt_with_a_negative_exponent_is_a_float() {
        assert_eq!(eval("(display (expt 2 -1))").unwrap(), "0.5");
    }

    #[test]
    fn expt_with_a_float_operand_is_a_float() {
        assert_eq!(eval("(display (expt 2.0 2))").unwrap(), "4.0");
    }

    #[test]
    fn exp_of_zero_is_one() {
        assert_eq!(eval("(display (exp 0))").unwrap(), "1.0");
    }

    #[test]
    fn log_of_one_is_zero() {
        assert_eq!(eval("(display (log 1))").unwrap(), "0.0");
    }

    #[test]
    fn sin_of_zero_is_zero() {
        assert_eq!(eval("(display (sin 0))").unwrap(), "0.0");
    }

    #[test]
    fn cos_of_zero_is_one() {
        assert_eq!(eval("(display (cos 0))").unwrap(), "1.0");
    }

    #[test]
    fn tan_of_zero_is_zero() {
        assert_eq!(eval("(display (tan 0))").unwrap(), "0.0");
    }

    #[test]
    fn atan_of_zero_is_zero() {
        assert_eq!(eval("(display (atan 0))").unwrap(), "0.0");
    }

    // --- B7 E5: number?/integer?/float?/exact->inexact/inexact->exact ---

    #[test]
    fn number_predicate_is_true_for_an_integer() {
        assert_eq!(eval("(display (number? 5))").unwrap(), "#t");
    }

    #[test]
    fn number_predicate_is_true_for_a_float() {
        assert_eq!(eval("(display (number? 5.0))").unwrap(), "#t");
    }

    #[test]
    fn number_predicate_is_false_for_a_non_number() {
        assert_eq!(eval("(display (number? \"5\"))").unwrap(), "#f");
    }

    #[test]
    fn integer_predicate_is_true_for_an_integer() {
        assert_eq!(eval("(display (integer? 5))").unwrap(), "#t");
    }

    #[test]
    fn integer_predicate_is_false_for_a_float() {
        assert_eq!(eval("(display (integer? 5.0))").unwrap(), "#f");
    }

    #[test]
    fn float_predicate_is_true_for_a_float() {
        assert_eq!(eval("(display (float? 5.0))").unwrap(), "#t");
    }

    #[test]
    fn float_predicate_is_false_for_an_integer() {
        assert_eq!(eval("(display (float? 5))").unwrap(), "#f");
    }

    #[test]
    fn exact_to_inexact_converts_a_whole_number_to_a_float() {
        assert_eq!(eval("(display (exact->inexact 5))").unwrap(), "5.0");
    }

    #[test]
    fn exact_to_inexact_leaves_an_already_inexact_value_unchanged() {
        assert_eq!(eval("(display (exact->inexact 5.0))").unwrap(), "5.0");
    }

    #[test]
    fn inexact_to_exact_leaves_an_already_exact_value_unchanged() {
        assert_eq!(eval("(display (inexact->exact 5))").unwrap(), "5");
    }

    #[test]
    fn inexact_to_exact_truncates_a_positive_float_toward_zero() {
        assert_eq!(eval("(display (inexact->exact 5.7))").unwrap(), "5");
    }

    #[test]
    fn inexact_to_exact_truncates_a_negative_float_toward_zero() {
        assert_eq!(eval("(display (inexact->exact -5.7))").unwrap(), "-5");
    }

    #[test]
    fn inexact_to_exact_on_positive_infinity_is_a_runtime_error() {
        // qa test-design review (msg #127): the feature file's evidence
        // claims the error names the specific non-finite value -- assert
        // on the message content, not just that it failed.
        let err = eval("(display (inexact->exact (/ 1.0 0.0)))").unwrap_err();
        assert!(
            err.message.contains("+inf.0"),
            "expected the error to name +inf.0, got: {}",
            err.message
        );
    }

    #[test]
    fn inexact_to_exact_on_negative_infinity_is_a_runtime_error() {
        let err = eval("(display (inexact->exact (/ -1.0 0.0)))").unwrap_err();
        assert!(
            err.message.contains("-inf.0"),
            "expected the error to name -inf.0, got: {}",
            err.message
        );
    }

    #[test]
    fn inexact_to_exact_on_not_a_number_is_a_runtime_error() {
        let err = eval("(display (inexact->exact (/ 0.0 0.0)))").unwrap_err();
        assert!(
            err.message.contains("+nan.0"),
            "expected the error to name +nan.0, got: {}",
            err.message
        );
    }

    #[test]
    fn inexact_to_exact_on_a_finite_float_outside_i64_range_is_a_runtime_error() {
        // warden security review (msg #122): a merely-large, still-finite
        // float like 1e300 used to pass the is_finite() guard and then
        // silently saturate to i64::MAX via Rust's saturating float-to-int
        // cast -- a value bearing no numerical relationship to the input,
        // contradicting this function's own established intent (out-of-
        // domain input is a clean error, not silent garbage).
        assert!(eval("(display (inexact->exact 1e300))").is_err());
    }

    #[test]
    fn inexact_to_exact_on_a_large_negative_out_of_range_float_is_a_runtime_error() {
        assert!(eval("(display (inexact->exact -1e300))").is_err());
    }

    #[test]
    fn inexact_to_exact_succeeds_exactly_at_the_i64_min_boundary() {
        // i64::MIN (-2^63) is exactly representable as an f64 and IS a
        // valid i64 -- must succeed, not be rejected as "out of range".
        assert_eq!(
            eval(&format!("(display (inexact->exact {}.0))", i64::MIN)).unwrap(),
            i64::MIN.to_string()
        );
    }

    #[test]
    fn inexact_to_exact_rejects_the_value_exactly_one_past_the_i64_max_boundary() {
        // i64::MAX (2^63 - 1) is NOT exactly representable as an f64 (the
        // nearest representable value is 2^63, one past the true maximum);
        // this pins down that 2^63 itself -- easy to get wrong by comparing
        // against a rounded `i64::MAX as f64` constant -- is correctly
        // rejected, not silently accepted as in-range. (The literal digits
        // are written out by hand, not derived from f64's own Display,
        // which renders this exact value without a decimal point at all --
        // "9223372036854776000" -- and the reader would then misparse it
        // as an out-of-range integer literal instead of this float.)
        assert!(eval("(display (inexact->exact 9223372036854775808.0))").is_err());
    }

    #[test]
    fn exact_to_inexact_of_a_non_number_is_a_runtime_error_naming_the_bad_value() {
        let err = eval("(display (exact->inexact \"x\"))").unwrap_err();
        assert!(
            err.message.contains("expects a number"),
            "expected a wrong-type error, got: {}",
            err.message
        );
    }

    #[test]
    fn inexact_to_exact_of_a_non_number_is_a_runtime_error_naming_the_bad_value() {
        let err = eval("(display (inexact->exact \"x\"))").unwrap_err();
        assert!(
            err.message.contains("expects a number"),
            "expected a wrong-type error, got: {}",
            err.message
        );
    }

    // --- B7 E6: number->string/string->number ---

    #[test]
    fn number_to_string_converts_an_integer() {
        assert_eq!(eval("(display (number->string 5))").unwrap(), "5");
    }

    #[test]
    fn number_to_string_converts_a_float() {
        assert_eq!(eval("(display (number->string 5.5))").unwrap(), "5.5");
    }

    #[test]
    fn string_to_number_parses_a_float() {
        assert_eq!(eval("(display (string->number \"3.5\"))").unwrap(), "3.5");
    }

    #[test]
    fn string_to_number_parses_an_integer() {
        assert_eq!(eval("(display (string->number \"42\"))").unwrap(), "42");
    }

    #[test]
    fn string_to_number_returns_false_on_unparseable_input() {
        assert_eq!(eval("(display (string->number \"xyz\"))").unwrap(), "#f");
    }

    #[test]
    fn number_to_string_then_string_to_number_round_trips() {
        assert_eq!(
            eval("(display (string->number (number->string 42)))").unwrap(),
            "42"
        );
    }

    #[test]
    fn number_to_string_of_a_non_number_is_a_runtime_error_naming_the_bad_value() {
        let err = eval("(display (number->string \"x\"))").unwrap_err();
        assert!(
            err.message.contains("expects a number"),
            "expected a wrong-type error, got: {}",
            err.message
        );
    }

    #[test]
    fn string_to_number_of_a_non_string_is_a_runtime_error_naming_the_bad_value() {
        let err = eval("(display (string->number 5))").unwrap_err();
        assert!(
            err.message.contains("expects a string"),
            "expected a wrong-type error, got: {}",
            err.message
        );
    }

    // --- B8 E1: eq? ---

    #[test]
    fn eq_is_true_for_two_separately_written_same_named_symbols() {
        assert_eq!(eval("(display (eq? (quote a) (quote a)))").unwrap(), "#t");
    }

    #[test]
    fn eq_is_true_for_simple_values_that_are_the_same_value() {
        assert_eq!(eval("(display (eq? 1 1))").unwrap(), "#t");
        assert_eq!(eval("(display (eq? #t #t))").unwrap(), "#t");
        assert_eq!(eval("(display (eq? (quote ()) (quote ())))").unwrap(), "#t");
        assert_eq!(eval("(display (eq? #\\a #\\a))").unwrap(), "#t");
    }

    #[test]
    fn eq_is_true_for_the_same_native_procedure() {
        assert_eq!(eval("(display (eq? + +))").unwrap(), "#t");
    }

    #[test]
    fn eq_is_false_for_two_separately_built_non_empty_lists_with_identical_contents() {
        assert_eq!(
            eval("(display (eq? (quote (1 2 3)) (quote (1 2 3))))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn eq_is_true_for_the_same_non_empty_list_bound_to_two_different_names() {
        assert_eq!(
            eval(
                "(define lst (quote (1 2 3))) (define other lst) \
                  (display (eq? lst other))"
            )
            .unwrap(),
            "#t"
        );
    }

    #[test]
    fn eq_is_false_for_two_separately_built_vectors_with_identical_contents() {
        assert_eq!(eval("(display (eq? #(1 2) #(1 2)))").unwrap(), "#f");
    }

    #[test]
    fn eq_is_true_for_the_same_vector_bound_to_two_different_names() {
        assert_eq!(
            eval("(define v #(1 2)) (define w v) (display (eq? v w))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn eq_is_false_for_two_separately_built_hashes() {
        assert_eq!(
            eval("(display (eq? (make-hash) (make-hash)))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn eq_is_true_for_the_same_hash_bound_to_two_different_names() {
        assert_eq!(
            eval("(define h (make-hash)) (define g h) (display (eq? h g))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn eq_is_true_for_the_same_closure_bound_to_two_different_names() {
        assert_eq!(
            eval("(define f (lambda (x) x)) (define g f) (display (eq? f g))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn eq_is_false_for_two_separately_defined_closures_with_identical_bodies() {
        assert_eq!(
            eval(
                "(define f (lambda (x) x)) (define g (lambda (x) x)) \
                  (display (eq? f g))"
            )
            .unwrap(),
            "#f"
        );
    }

    #[test]
    fn eq_is_false_for_two_closures_from_the_same_lambda_captured_in_different_environments() {
        assert_eq!(
            eval(
                "(define (make-adder n) (lambda (x) (+ x n))) \
                  (display (eq? (make-adder 1) (make-adder 2)))"
            )
            .unwrap(),
            "#f"
        );
    }

    #[test]
    fn eq_is_false_between_the_empty_list_and_a_non_empty_list() {
        assert_eq!(
            eval("(display (eq? (quote ()) (quote (1 2))))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn eq_is_true_for_two_unspecified_results_from_set() {
        assert_eq!(
            eval("(define x 1) (display (eq? (set! x 2) (set! x 3)))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn eq_is_false_for_two_separately_built_pairs_with_identical_contents() {
        assert_eq!(eval("(display (eq? (cons 1 2) (cons 1 2)))").unwrap(), "#f");
    }

    #[test]
    fn eq_is_true_for_the_same_pair_bound_to_two_different_names() {
        assert_eq!(
            eval("(define p (cons 1 2)) (define q p) (display (eq? p q))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn eq_is_false_for_two_separately_built_strings_with_identical_contents() {
        assert_eq!(eval("(display (eq? \"ab\" \"ab\"))").unwrap(), "#f");
    }

    #[test]
    fn eq_is_true_for_the_same_string_bound_to_two_different_names() {
        assert_eq!(
            eval("(define s \"ab\") (define t s) (display (eq? s t))").unwrap(),
            "#t"
        );
    }

    // --- B8 E2: eqv? ---

    #[test]
    fn eqv_is_false_between_a_whole_number_and_a_float_of_the_same_magnitude() {
        assert_eq!(eval("(display (eqv? 1 1.0))").unwrap(), "#f");
    }

    #[test]
    fn eqv_is_false_between_positive_and_negative_zero() {
        assert_eq!(eval("(display (eqv? 0.0 -0.0))").unwrap(), "#f");
    }

    #[test]
    fn eqv_is_true_for_two_independently_computed_equal_floats() {
        assert_eq!(eval("(display (eqv? (+ 0.5 0.5) 1.0))").unwrap(), "#t");
    }

    #[test]
    fn eqv_is_true_for_two_nan_floats() {
        assert_eq!(
            eval("(display (eqv? (/ 0.0 0.0) (/ 0.0 0.0)))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn eqv_is_false_between_a_nan_float_and_an_ordinary_float() {
        // Distinguishes "NaN is equal to NaN specifically" from a broken
        // "either side is NaN" check that would wrongly call a NaN equal
        // to anything.
        assert_eq!(eval("(display (eqv? (/ 0.0 0.0) 5.0))").unwrap(), "#f");
    }

    // --- B8 E3: equal? ---

    #[test]
    fn equal_is_true_for_two_separately_built_lists_with_the_same_contents() {
        assert_eq!(
            eval("(display (equal? (cons 1 (cons 2 (quote ()))) (cons 1 (cons 2 (quote ())))))")
                .unwrap(),
            "#t"
        );
    }

    #[test]
    fn equal_is_true_for_two_separately_built_strings_with_the_same_characters() {
        assert_eq!(eval("(display (equal? \"ab\" \"ab\"))").unwrap(), "#t");
    }

    #[test]
    fn equal_is_true_for_two_separately_built_quoted_lists_with_the_same_contents() {
        assert_eq!(
            eval("(display (equal? (quote (1 2 3)) (quote (1 2 3))))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn equal_is_false_for_two_quoted_lists_where_one_is_a_prefix_of_the_other() {
        assert_eq!(
            eval("(display (equal? (quote (1 2)) (quote (1 2 3))))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn equal_recurses_into_a_list_containing_a_list() {
        // Proves genuine recursion, not just a top-level shallow compare:
        // the outer lists' second elements are themselves lists that must
        // be compared structurally too.
        assert_eq!(
            eval(
                "(display (equal? (cons 1 (cons (cons 2 (cons 3 (quote ()))) (quote ()))) \
                                   (cons 1 (cons (cons 2 (cons 3 (quote ()))) (quote ())))))"
            )
            .unwrap(),
            "#t"
        );
        assert_eq!(
            eval(
                "(display (equal? (cons 1 (cons (cons 2 (cons 3 (quote ()))) (quote ()))) \
                                   (cons 1 (cons (cons 2 (cons 4 (quote ()))) (quote ())))))"
            )
            .unwrap(),
            "#f"
        );
    }

    #[test]
    fn equal_recurses_into_a_vector_containing_strings() {
        assert_eq!(
            eval("(display (equal? #(\"a\" \"b\") #(\"a\" \"b\")))").unwrap(),
            "#t"
        );
        assert_eq!(
            eval("(display (equal? #(\"a\" \"b\") #(\"a\" \"c\")))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn equal_falls_back_to_eqv_for_non_container_values() {
        assert_eq!(eval("(display (equal? 1 1.0))").unwrap(), "#f");
    }

    #[test]
    fn equal_completes_without_hanging_on_a_moderately_deep_non_circular_structure() {
        let src = format!(
            "(define (build n) (if (= n 0) (quote ()) (cons n (build (- n 1))))) \
             (display (equal? (build {n}) (build {n})))",
            n = 5_000
        );
        assert_eq!(eval(&src).unwrap(), "#t");
    }

    // --- B8 E4: not ---

    #[test]
    fn not_of_false_is_true() {
        assert_eq!(eval("(display (not #f))").unwrap(), "#t");
    }

    #[test]
    fn not_of_zero_is_false() {
        assert_eq!(eval("(display (not 0))").unwrap(), "#f");
    }

    #[test]
    fn not_of_the_empty_list_is_false() {
        assert_eq!(eval("(display (not (quote ())))").unwrap(), "#f");
    }

    #[test]
    fn not_of_a_string_is_false() {
        assert_eq!(eval("(display (not \"x\"))").unwrap(), "#f");
    }

    // --- B8 E5: type predicates ---

    #[test]
    fn list_predicate_is_true_for_a_proper_finite_list() {
        assert_eq!(
            eval("(display (list? (cons 1 (cons 2 (cons 3 (quote ()))))))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn list_predicate_is_false_for_an_improper_dotted_structure() {
        assert_eq!(eval("(display (list? (cons 1 2)))").unwrap(), "#f");
    }

    #[test]
    fn list_predicate_is_true_for_a_quoted_list_literal() {
        assert_eq!(eval("(display (list? (quote (1 2 3))))").unwrap(), "#t");
    }

    #[test]
    fn null_predicate_is_true_for_the_empty_list() {
        assert_eq!(eval("(display (null? (quote ())))").unwrap(), "#t");
    }

    #[test]
    fn null_predicate_is_false_for_a_non_empty_value() {
        assert_eq!(eval("(display (null? 5))").unwrap(), "#f");
    }

    #[test]
    fn pair_predicate_is_false_for_the_empty_list() {
        assert_eq!(eval("(display (pair? (quote ())))").unwrap(), "#f");
    }

    #[test]
    fn pair_predicate_is_true_for_an_actual_pair() {
        assert_eq!(eval("(display (pair? (cons 1 2)))").unwrap(), "#t");
    }

    #[test]
    fn pair_predicate_is_true_for_a_non_empty_quoted_list() {
        assert_eq!(eval("(display (pair? (quote (1 2 3))))").unwrap(), "#t");
    }

    #[test]
    fn procedure_predicate_is_true_for_the_addition_operator() {
        assert_eq!(eval("(display (procedure? +))").unwrap(), "#t");
    }

    #[test]
    fn procedure_predicate_is_false_for_a_non_procedure() {
        assert_eq!(eval("(display (procedure? 5))").unwrap(), "#f");
    }

    #[test]
    fn procedure_predicate_is_true_for_a_user_defined_closure() {
        assert_eq!(
            eval("(define (f x) x) (display (procedure? f))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn symbol_predicate_shown_both_ways() {
        assert_eq!(eval("(display (symbol? (quote a)))").unwrap(), "#t");
        assert_eq!(eval("(display (symbol? 5))").unwrap(), "#f");
    }

    #[test]
    fn string_predicate_shown_both_ways() {
        assert_eq!(eval("(display (string? \"x\"))").unwrap(), "#t");
        assert_eq!(eval("(display (string? 5))").unwrap(), "#f");
    }

    #[test]
    fn char_predicate_shown_both_ways() {
        assert_eq!(eval("(display (char? #\\a))").unwrap(), "#t");
        assert_eq!(eval("(display (char? 5))").unwrap(), "#f");
    }

    #[test]
    fn boolean_predicate_shown_both_ways() {
        assert_eq!(eval("(display (boolean? #t))").unwrap(), "#t");
        assert_eq!(eval("(display (boolean? 5))").unwrap(), "#f");
    }

    #[test]
    fn vector_predicate_shown_both_ways() {
        assert_eq!(eval("(display (vector? #(1 2)))").unwrap(), "#t");
        assert_eq!(eval("(display (vector? 5))").unwrap(), "#f");
    }

    #[test]
    fn hash_predicate_shown_both_ways() {
        assert_eq!(eval("(display (hash? (make-hash)))").unwrap(), "#t");
        assert_eq!(eval("(display (hash? 5))").unwrap(), "#f");
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

    #[test]
    fn op_eqv_distinguishes_positive_from_negative_zero() {
        // A pre-existing bug, fixed alongside B8's eqv? implementation:
        // Op::Eqv (case's own candidate-matching opcode) used raw IEEE `==`
        // instead of eqv?'s bit-precise comparison, so it used to
        // incorrectly treat 0.0 and -0.0 as the same value -- e.g. `(case
        // -0.0 ((0.0) 'wrong-match))` would wrongly take that branch.
        let mut chunk = Chunk::new();
        let pos_zero = chunk.add_const(Const::Float(0.0));
        let neg_zero = chunk.add_const(Const::Float(-0.0));
        let display_sym = chunk.add_const(Const::Symbol("display".to_string()));
        chunk.emit_get_global(display_sym);
        chunk.emit_const(pos_zero);
        chunk.emit_const(neg_zero);
        chunk.emit_eqv();
        chunk.emit_call(1);
        chunk.emit_pop();
        chunk.emit_halt();
        assert_eq!(run_to_string(chunk).unwrap(), "#f");
    }

    // B6: tail-call trampoline. These tests exercise the VM's own O(1)-space
    // reuse of the current native frame for Op::TailCall, as distinct from
    // the compiler-level tests (in compiler.rs) that pin down *which* call
    // sites become TailCall in the first place.

    #[test]
    fn a_self_tail_recursive_loop_runs_far_beyond_max_call_depth_without_error() {
        // If TailCall recursed into exec() like an ordinary Call instead of
        // trampolining, this would either hit the MAX_CALL_DEPTH guard or
        // overflow the native stack. Running well past MAX_CALL_DEPTH
        // iterations and getting the correct answer proves the loop body's
        // self-call never grows the call stack at all.
        let src = format!(
            "(define (loop n limit) (if (= n limit) n (loop (+ n 1) limit))) \
             (display (loop 0 {}))",
            MAX_CALL_DEPTH * 5
        );
        assert_eq!(eval(&src).unwrap(), (MAX_CALL_DEPTH * 5).to_string());
    }

    #[test]
    fn mutual_tail_recursion_runs_far_beyond_max_call_depth_without_error() {
        // Same guarantee as the self-recursive case above, but for two
        // functions calling each other back and forth, each time as the
        // last action (B6's DEMO 2 shape).
        let src = format!(
            "(define (ev? n) (if (= n 0) #t (od? (- n 1)))) \
             (define (od? n) (if (= n 0) #f (ev? (- n 1)))) \
             (display (ev? {}))",
            MAX_CALL_DEPTH * 5
        );
        assert_eq!(eval(&src).unwrap(), "#t");
    }

    #[test]
    fn call_depth_does_not_grow_across_a_tail_recursive_chain() {
        // White-box check on the same guarantee as the test above, from the
        // other direction: after a self-tail-recursive loop of many more
        // iterations than MAX_CALL_DEPTH returns, call_depth must be back
        // to exactly 0 -- the single non-tail invocation from call_value
        // below is the only thing that ever incremented it; every iteration
        // inside the loop reused that one frame via Op::TailCall.
        let forms = read_program("(define (loop n limit) (if (= n limit) n (loop (+ n 1) limit)))")
            .unwrap();
        let module = compile_program(&forms).unwrap();
        let mut vm = Vm {
            module: &module,
            globals: default_globals(),
            call_depth: 0,
            stdin_buffer: String::new(),
            stdin_channel: StdinChannel::none(),
        };
        let mut out = Vec::new();
        let entry = &module.functions[module.entry_index as usize];
        vm.exec(entry, Vec::new(), None, &mut out).unwrap();
        let loop_fn = vm.globals.get("loop").cloned().unwrap();

        let limit = (MAX_CALL_DEPTH * 5) as i64;
        let result = vm
            .call_value(&loop_fn, vec![Value::Int(0), Value::Int(limit)], &mut out)
            .unwrap();

        assert_eq!(result, Value::Int(limit));
        assert_eq!(
            vm.call_depth, 0,
            "call_depth must be fully unwound after a tail-recursive call returns, \
             not left elevated by iterations that should have reused one frame"
        );
    }

    #[test]
    fn disassembly_distinguishes_tail_call_from_an_ordinary_call() {
        // (define (add a b) (+ a b)) -- add's own tail expression is a
        // TailCall; ensure the disassembler labels it distinctly rather
        // than folding it into the same mnemonic as an ordinary CALL.
        let forms = read_program("(define (add a b) (+ a b))").unwrap();
        let module = compile_program(&forms).unwrap();
        let add_chunk = &module.functions[0];
        let listing = crate::disasm::disassemble_chunk(add_chunk);
        assert!(
            listing.contains("TAIL_CALL"),
            "expected a TAIL_CALL mnemonic in: {listing}"
        );
        let has_bare_call = listing
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some("CALL"));
        assert!(
            !has_bare_call,
            "add's body has exactly one call, and it's a tail call, not a plain CALL: {listing}"
        );
    }

    // --- B9 E1: pair mutation and cxr accessors (spec 5.1) ---

    #[test]
    fn a_freshly_constructed_pair_retrieves_both_halves_back_out() {
        assert_eq!(eval("(display (car (cons 1 2)))").unwrap(), "1");
        assert_eq!(eval("(display (cdr (cons 1 2)))").unwrap(), "2");
    }

    #[test]
    fn set_car_replaces_the_first_half_in_place_and_it_is_observed_afterward() {
        assert_eq!(
            eval("(define p (cons 1 2)) (set-car! p 99) (display (car p))").unwrap(),
            "99"
        );
    }

    #[test]
    fn set_cdr_replaces_the_second_half_in_place_and_it_is_observed_afterward() {
        assert_eq!(
            eval("(define p (cons 1 2)) (set-cdr! p 99) (display (cdr p))").unwrap(),
            "99"
        );
    }

    #[test]
    fn set_car_and_set_cdr_mutate_the_same_pair_independently() {
        assert_eq!(
            eval(
                "(define p (cons 1 2)) (set-car! p 10) (set-cdr! p 20) \
                  (display (car p)) (display (cdr p))"
            )
            .unwrap(),
            "1020"
        );
    }

    #[test]
    fn mutating_a_pair_through_one_binding_is_observed_through_an_aliased_binding() {
        // qa test-design review (msg #145): pair-mutation tests only
        // verified through the same binding that performed the mutation --
        // this checks the mutation is observed via a SEPARATE binding to
        // the same underlying pair (the same "shared vs. copied" property
        // this project has verified since B5's closures).
        assert_eq!(
            eval("(define p (cons 1 2)) (define q p) (set-car! p 99) (display (car q))").unwrap(),
            "99"
        );
        assert_eq!(
            eval("(define p (cons 1 2)) (define q p) (set-cdr! q 99) (display (cdr p))").unwrap(),
            "99"
        );
    }

    #[test]
    fn cadr_reaches_the_second_element() {
        assert_eq!(eval("(display (cadr (cons 1 (cons 2 3))))").unwrap(), "2");
    }

    #[test]
    fn cddr_drops_the_first_element_twice() {
        assert_eq!(eval("(display (cddr (cons 1 (cons 2 3))))").unwrap(), "3");
    }

    #[test]
    fn caar_reaches_the_first_of_the_first() {
        assert_eq!(eval("(display (caar (cons (cons 1 2) 3)))").unwrap(), "1");
    }

    #[test]
    fn cdar_reaches_the_rest_of_the_first() {
        assert_eq!(eval("(display (cdar (cons (cons 1 2) 3)))").unwrap(), "2");
    }

    #[test]
    fn caddr_reaches_three_levels_deep() {
        assert_eq!(
            eval("(display (caddr (cons 1 (cons 2 (cons 3 4)))))").unwrap(),
            "3"
        );
    }

    #[test]
    fn composed_cxr_accessors_are_clean_runtime_errors_on_malformed_input() {
        // qa test-design review (msg #145): only the base car/cdr had an
        // error-path test; cadr/caar/cdar/cddr/caddr compose car_of/cdr_of
        // but hadn't been checked to fail cleanly (not panic) when an
        // intermediate step isn't a pair.
        assert!(eval("(display (cadr (cons 1 2)))").is_err());
        assert!(eval("(display (caar (cons 1 2)))").is_err());
        assert!(eval("(display (cdar (cons 1 2)))").is_err());
        assert!(eval("(display (cddr (cons 1 2)))").is_err());
        assert!(eval("(display (caddr (cons 1 2)))").is_err());
    }

    #[test]
    fn car_and_cdr_also_reach_into_a_quoted_list_literal() {
        assert_eq!(eval("(display (car (quote (1 2 3))))").unwrap(), "1");
        assert_eq!(eval("(display (cdr (quote (1 2 3))))").unwrap(), "(2 3)");
    }

    #[test]
    fn car_and_cdr_of_the_empty_list_literal_are_clean_runtime_errors_not_a_crash() {
        // Asserts on the specific message, not just is_err(): an indexing
        // panic inside car_of/cdr_of would also surface as an Err (VM
        // panics are caught and converted at the thread join), but with a
        // generic "VM thread panicked" message rather than this one -- so
        // only checking the message distinguishes a clean, intentional
        // error path from an accidental out-of-bounds panic.
        let car_err = eval("(display (car (quote ())))").unwrap_err();
        assert_eq!(car_err.message, "car expects a pair, found ()");
        let cdr_err = eval("(display (cdr (quote ())))").unwrap_err();
        assert_eq!(cdr_err.message, "cdr expects a pair, found ()");
    }

    #[test]
    fn car_of_a_non_pair_is_a_clean_runtime_error() {
        assert!(eval("(display (car 5))").is_err());
    }

    #[test]
    fn set_car_of_a_non_pair_is_a_clean_runtime_error() {
        assert!(eval("(set-car! 5 1)").is_err());
    }

    // --- B9 E2: list construction and inspection (spec 5.1) ---

    #[test]
    fn list_constructs_a_proper_list_from_a_sequence_of_values() {
        assert_eq!(eval("(display (list 1 2 3))").unwrap(), "(1 2 3)");
    }

    #[test]
    fn list_with_no_arguments_is_the_empty_list() {
        assert_eq!(eval("(display (list))").unwrap(), "()");
    }

    #[test]
    fn length_of_a_quoted_list_literal() {
        assert_eq!(eval("(display (length (quote (a b c))))").unwrap(), "3");
    }

    #[test]
    fn length_of_the_empty_list_is_zero() {
        assert_eq!(eval("(display (length (quote ())))").unwrap(), "0");
    }

    #[test]
    fn append_concatenates_two_lists() {
        assert_eq!(
            eval("(display (append (list 1 2) (list 3 4)))").unwrap(),
            "(1 2 3 4)"
        );
    }

    #[test]
    fn reverse_a_list() {
        assert_eq!(eval("(display (reverse (list 1 2 3)))").unwrap(), "(3 2 1)");
    }

    #[test]
    fn list_ref_at_a_middle_position() {
        assert_eq!(
            eval("(display (list-ref (list 10 20 30) 1))").unwrap(),
            "20"
        );
    }

    #[test]
    fn list_ref_at_the_last_valid_position() {
        assert_eq!(
            eval("(display (list-ref (list 10 20 30) 2))").unwrap(),
            "30"
        );
    }

    #[test]
    fn list_tail_at_position_zero_is_the_identity() {
        assert_eq!(
            eval("(display (list-tail (list 1 2 3) 0))").unwrap(),
            "(1 2 3)"
        );
    }

    #[test]
    fn list_tail_at_a_position_beyond_zero() {
        assert_eq!(eval("(display (list-tail (list 1 2 3) 2))").unwrap(), "(3)");
    }

    #[test]
    fn last_pair_of_a_multi_element_list_is_cons_shaped_holding_the_last_element_and_empty() {
        assert_eq!(eval("(display (last-pair (list 1 2 3)))").unwrap(), "(3)");
        assert_eq!(
            eval("(display (pair? (last-pair (list 1 2 3))))").unwrap(),
            "#t"
        );
        assert_eq!(
            eval("(display (cdr (last-pair (list 1 2 3))))").unwrap(),
            "()"
        );
    }

    #[test]
    fn last_pair_also_works_on_a_quoted_list_literal_not_just_a_cons_built_list() {
        // (list 1 2 3) already builds a genuine Pair chain, so it never
        // exercises last-pair's separate List-to-Pair conversion path; a
        // quoted literal is backed by the flat List representation instead.
        assert_eq!(
            eval("(display (last-pair (quote (1 2 3))))").unwrap(),
            "(3)"
        );
    }

    // --- B9 E3: member/memv/memq at the three equality strictness levels ---

    #[test]
    fn member_finds_the_first_sublist_starting_with_a_matching_element() {
        assert_eq!(eval("(display (member 2 (list 1 2 3)))").unwrap(), "(2 3)");
    }

    #[test]
    fn member_returns_false_when_nothing_matches() {
        assert_eq!(eval("(display (member 5 (list 1 2 3)))").unwrap(), "#f");
    }

    #[test]
    fn memv_finds_the_first_sublist_starting_with_a_matching_element() {
        assert_eq!(eval("(display (memv 2 (list 1 2 3)))").unwrap(), "(2 3)");
    }

    #[test]
    fn memq_finds_the_first_sublist_starting_with_a_matching_element() {
        assert_eq!(eval("(display (memq 2 (list 1 2 3)))").unwrap(), "(2 3)");
    }

    #[test]
    fn member_finds_a_separately_built_compound_value_that_memq_cannot() {
        assert_eq!(
            eval("(display (member (list 1 2) (list (list 1 2) 3)))").unwrap(),
            "((1 2) 3)"
        );
        assert_eq!(
            eval("(display (memq (list 1 2) (list (list 1 2) 3)))").unwrap(),
            "#f"
        );
    }

    // --- B9 E4: assoc/assv/assq at the three equality strictness levels ---

    #[test]
    fn assoc_finds_the_first_entry_whose_key_matches() {
        assert_eq!(
            eval("(display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))").unwrap(),
            "(2 . b)"
        );
    }

    #[test]
    fn assoc_returns_false_when_no_key_matches() {
        assert_eq!(
            eval("(display (assoc 5 (list (cons 1 (quote a)) (cons 2 (quote b)))))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn assv_finds_the_first_entry_whose_key_matches() {
        assert_eq!(
            eval("(display (assv 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))").unwrap(),
            "(2 . b)"
        );
    }

    #[test]
    fn assq_finds_the_first_entry_whose_key_matches() {
        assert_eq!(
            eval("(display (assq 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))").unwrap(),
            "(2 . b)"
        );
    }

    #[test]
    fn assoc_finds_a_separately_built_compound_key_that_assq_cannot() {
        assert_eq!(
            eval("(display (assoc (list 1 2) (list (cons (list 1 2) (quote a)))))").unwrap(),
            "((1 2) . a)"
        );
        assert_eq!(
            eval("(display (assq (list 1 2) (list (cons (list 1 2) (quote a)))))").unwrap(),
            "#f"
        );
    }

    // --- B9 E5: map/for-each/filter (spec 5.1) ---

    #[test]
    fn map_squares_every_element_of_a_single_list() {
        assert_eq!(
            eval(
                "(define (square x) (* x x)) \
                  (display (map square (list 1 2 3)))"
            )
            .unwrap(),
            "(1 4 9)"
        );
    }

    #[test]
    fn map_over_two_equal_length_lists_in_parallel() {
        assert_eq!(
            eval("(display (map + (list 1 2 3) (list 10 20 30)))").unwrap(),
            "(11 22 33)"
        );
    }

    #[test]
    fn filter_keeps_only_elements_satisfying_the_predicate() {
        assert_eq!(
            eval("(display (filter odd? (list 1 2 3 4 5)))").unwrap(),
            "(1 3 5)"
        );
    }

    #[test]
    fn for_each_performs_a_side_effect_and_its_own_value_is_not_a_transformed_list() {
        assert_eq!(
            eval(
                "(define (square x) (* x x)) \
                  (for-each (lambda (x) (display (square x))) (list 1 2 3))"
            )
            .unwrap(),
            "149"
        );
    }

    #[test]
    fn for_each_and_map_contrasted_on_the_same_input() {
        assert_eq!(
            eval(
                "(define (square x) (* x x)) \
                  (display (map square (list 1 2 3))) (newline) \
                  (for-each (lambda (x) (display (square x))) (list 1 2 3))"
            )
            .unwrap(),
            "(1 4 9)\n149"
        );
    }

    #[test]
    fn map_filter_and_for_each_on_the_empty_list() {
        // qa test-design review (msg #145): only reduce covered the
        // empty-list edge case among this iteration's higher-order
        // procedures.
        assert_eq!(
            eval("(display (map (lambda (x) (* x x)) (quote ())))").unwrap(),
            "()"
        );
        assert_eq!(eval("(display (filter odd? (quote ())))").unwrap(), "()");
        assert_eq!(
            eval("(for-each (lambda (x) (display x)) (quote ()))").unwrap(),
            ""
        );
    }

    #[test]
    fn map_and_filter_on_a_single_element_list() {
        assert_eq!(
            eval("(display (map (lambda (x) (* x x)) (list 5)))").unwrap(),
            "(25)"
        );
        assert_eq!(eval("(display (filter odd? (list 5)))").unwrap(), "(5)");
        assert_eq!(eval("(display (filter odd? (list 4)))").unwrap(), "()");
    }

    // --- B9 E6: fold-left/fold-right/reduce (spec 5.1) ---

    #[test]
    fn fold_left_sums_from_a_given_initial_value() {
        assert_eq!(
            eval("(display (fold-left + 0 (list 1 2 3 4)))").unwrap(),
            "10"
        );
    }

    #[test]
    fn fold_right_builds_the_list_back_up_via_cons_preserving_order() {
        assert_eq!(
            eval("(display (fold-right cons (quote ()) (list 1 2 3)))").unwrap(),
            "(1 2 3)"
        );
    }

    #[test]
    fn fold_left_and_fold_right_diverge_on_a_non_commutative_operation() {
        assert_eq!(
            eval("(display (fold-left - 0 (list 1 2 3)))").unwrap(),
            "-6"
        );
        assert_eq!(
            eval("(display (fold-right - 0 (list 1 2 3)))").unwrap(),
            "2"
        );
    }

    #[test]
    fn fold_left_and_fold_right_on_the_empty_list_return_the_initial_value() {
        assert_eq!(eval("(display (fold-left + 0 (quote ())))").unwrap(), "0");
        assert_eq!(
            eval("(display (fold-right cons (quote ()) (quote ())))").unwrap(),
            "()"
        );
    }

    #[test]
    fn reduce_self_seeds_from_the_lists_own_first_element() {
        assert_eq!(eval("(display (reduce + 0 (list 1 2 3 4)))").unwrap(), "10");
    }

    #[test]
    fn reduce_falls_back_to_the_given_initial_value_on_an_empty_list() {
        assert_eq!(eval("(display (reduce + 99 (quote ())))").unwrap(), "99");
    }

    // --- B9 E7: apply flattens direct arguments plus a trailing list ---

    #[test]
    fn apply_flattens_direct_arguments_plus_a_trailing_list() {
        assert_eq!(eval("(display (apply + 1 2 (list 3 4)))").unwrap(), "10");
    }

    #[test]
    fn apply_with_zero_direct_arguments_is_just_the_trailing_list() {
        assert_eq!(eval("(display (apply + (list 1 2 3)))").unwrap(), "6");
    }

    #[test]
    fn apply_with_an_empty_trailing_list_is_just_the_direct_arguments() {
        assert_eq!(eval("(display (apply + 1 2 (list)))").unwrap(), "3");
    }

    // --- B9 E8: quoted list literals read to exactly the structure written ---

    #[test]
    fn a_nested_list_literal_is_structurally_equal_to_an_independently_built_equivalent() {
        assert_eq!(
            eval(
                "(display (equal? (quote (1 (2 3) 4)) \
                                  (cons 1 (cons (cons 2 (cons 3 (quote ()))) (cons 4 (quote ()))))))"
            )
            .unwrap(),
            "#t"
        );
    }

    #[test]
    fn a_nested_list_literal_is_reachable_via_accessors() {
        assert_eq!(
            eval("(display (car (cadr (quote (1 (2 3) 4)))))").unwrap(),
            "2"
        );
    }

    #[test]
    fn a_simple_dotted_pair_literal_reads_as_a_genuine_dotted_structure() {
        assert_eq!(eval("(display (quote (a . b)))").unwrap(), "(a . b)");
        assert_eq!(eval("(display (car (quote (a . b))))").unwrap(), "a");
        assert_eq!(eval("(display (cdr (quote (a . b))))").unwrap(), "b");
    }

    #[test]
    fn a_longer_improper_list_literal_is_not_silently_coerced_into_a_proper_list() {
        assert_eq!(eval("(display (quote (1 2 . 3)))").unwrap(), "(1 2 . 3)");
        assert_eq!(eval("(display (list? (quote (1 2 . 3))))").unwrap(), "#f");
    }

    // --- B9 E9: integration: all fourteen demo expressions in one program ---

    #[test]
    fn all_fourteen_demo_expressions_produce_exactly_the_prescribed_output() {
        assert_eq!(
            eval(
                "(display (car (quote (1 2 3)))) (newline) \
                 (display (cadr (quote (1 2 3)))) (newline) \
                 (display (length (quote (a b c)))) (newline) \
                 (display (append (list 1 2) (list 3 4))) (newline) \
                 (display (reverse (list 1 2 3))) (newline) \
                 (display (map (lambda (x) (* x x)) (list 1 2 3))) (newline) \
                 (display (map + (list 1 2 3) (list 10 20 30))) (newline) \
                 (display (filter odd? (list 1 2 3 4 5))) (newline) \
                 (display (fold-left + 0 (list 1 2 3 4))) (newline) \
                 (display (fold-right cons (quote ()) (list 1 2 3))) (newline) \
                 (display (reduce + 0 (list 1 2 3 4))) (newline) \
                 (display (apply + 1 2 (list 3 4))) (newline) \
                 (display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b))))) (newline) \
                 (display (member 2 (list 1 2 3))) (newline)"
            )
            .unwrap(),
            "1\n2\n3\n(1 2 3 4)\n(3 2 1)\n(1 4 9)\n(11 22 33)\n(1 3 5)\n\
             10\n(1 2 3)\n10\n10\n(2 . b)\n(2 3)\n"
        );
    }

    // --- qa test-review (msg #143): equal? must terminate on a self-
    // referential pair built via set-cdr!, now that pairs are mutable ---

    #[test]
    fn equal_terminates_on_a_pair_made_self_referential_via_set_cdr() {
        assert_eq!(
            eval("(define p (cons 1 2)) (set-cdr! p p) (display (equal? p p))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn equal_terminates_comparing_two_separately_built_self_referential_pairs() {
        assert_eq!(
            eval(
                "(define p (cons 1 2)) (set-cdr! p p) \
                  (define q (cons 1 2)) (set-cdr! q q) \
                  (display (equal? p q))"
            )
            .unwrap(),
            "#t"
        );
    }

    #[test]
    #[ignore = "heavy: ~60s and ~2.7GB RSS in a debug build (qa test-design \
                review msg #148) -- run explicitly with `cargo test -- --ignored` \
                rather than in the default fast suite. Deliberately this large: \
                a smaller size (qa suggested 200-500k) wouldn't actually \
                reproduce the pre-fix crash, which only manifested at \
                8,000,000 elements (confirmed via manual reversion)."]
    fn value_equal_list_predicate_and_last_pair_complete_on_a_multi_million_element_pair_chain() {
        // Regression test for warden security review msg #144: equal?'s
        // Pair-chain walk, list?, and last-pair must each be iterative, not
        // recursive -- an ordinary tail-recursive builder loop constructs a
        // Pair chain with no runtime length bound, and one native stack
        // frame per element previously crashed the process outright on a
        // long enough chain (confirmed at 8,000,000 elements pre-fix).
        let src = "(define (build-loop n acc) \
                     (if (= n 0) acc (build-loop (- n 1) (cons n acc)))) \
                   (define a (build-loop 8000000 (quote ()))) \
                   (define b (build-loop 8000000 (quote ()))) \
                   (display (equal? a b)) (newline) \
                   (display (list? a)) (newline) \
                   (display (car (last-pair a)))";
        assert_eq!(eval(src).unwrap(), "#t\n#t\n8000000");
    }

    // --- qa/warden msg #146: list operations must not hang forever on a
    // circular list (constructible via set-cdr! since this iteration) ---

    #[test]
    fn length_on_a_circular_list_is_a_clean_error_not_a_hang() {
        assert!(
            eval("(define p (list 1 2 3)) (set-cdr! (last-pair p) p) (display (length p))")
                .is_err()
        );
    }

    #[test]
    fn member_on_a_circular_list_with_no_match_returns_false_not_a_hang() {
        assert_eq!(
            eval("(define p (list 1 2 3)) (set-cdr! (last-pair p) p) (display (member 99 p))")
                .unwrap(),
            "#f"
        );
    }

    #[test]
    fn displaying_a_circular_list_terminates_with_an_ellipsis_not_a_hang() {
        assert_eq!(
            eval("(define p (list 1 2 3)) (set-cdr! (last-pair p) p) (display p)").unwrap(),
            "(1 2 3 ...)"
        );
    }

    #[test]
    fn list_predicate_on_a_trivially_short_cyclic_pair_is_false_not_a_hang() {
        // warden security review, msg #147: a self-referential pair is
        // never a finite list, so #f is the correct answer, not just a
        // hang-avoidance fallback.
        assert_eq!(
            eval("(define p (cons 1 2)) (set-cdr! p p) (display (list? p))").unwrap(),
            "#f"
        );
    }

    #[test]
    fn last_pair_on_a_trivially_short_cyclic_pair_is_a_clean_error_not_a_hang() {
        assert!(eval("(define p (cons 1 2)) (set-cdr! p p) (display (last-pair p))").is_err());
    }

    // --- B10 E1: string length, ref, substring, append (spec 6.1) ---

    #[test]
    fn string_length_of_hello_is_five() {
        assert_eq!(eval("(display (string-length \"hello\"))").unwrap(), "5");
    }

    #[test]
    fn string_ref_at_position_one_of_hello_is_e() {
        assert_eq!(eval("(display (string-ref \"hello\" 1))").unwrap(), "e");
    }

    #[test]
    fn substring_from_one_to_four_of_hello_is_ell() {
        assert_eq!(eval("(display (substring \"hello\" 1 4))").unwrap(), "ell");
    }

    #[test]
    fn string_append_joins_three_or_more_strings() {
        assert_eq!(
            eval("(display (string-append \"foo\" \"bar\" \"baz\"))").unwrap(),
            "foobarbaz"
        );
    }

    #[test]
    fn string_append_joins_two_strings() {
        assert_eq!(
            eval("(display (string-append \"foo\" \"bar\"))").unwrap(),
            "foobar"
        );
    }

    #[test]
    fn string_ref_out_of_bounds_is_a_clean_runtime_error() {
        assert!(eval("(display (string-ref \"hello\" 5))").is_err());
    }

    #[test]
    fn substring_out_of_bounds_is_a_clean_runtime_error() {
        assert!(eval("(display (substring \"hello\" 1 10))").is_err());
    }

    #[test]
    fn substring_from_start_to_itself_is_the_empty_string() {
        assert_eq!(eval("(display (substring \"hello\" 2 2))").unwrap(), "");
    }

    #[test]
    fn substring_to_the_length_is_the_suffix_to_the_end() {
        assert_eq!(eval("(display (substring \"hello\" 2 5))").unwrap(), "llo");
    }

    #[test]
    fn substring_with_start_after_end_is_a_clean_runtime_error_naming_it_invalid() {
        let err = eval("(display (substring \"hello\" 3 1))").unwrap_err();
        assert_eq!(err.message, "substring range 3..1 is invalid");
    }

    #[test]
    fn string_length_counts_a_multi_byte_character_as_one_position_not_two_bytes() {
        // qa test-design review (msg #167): all of E1's original tests were
        // pure ASCII, where char-count and byte-count are numerically
        // identical -- none could fail if `.chars().count()` regressed to
        // `.len()` (byte count). FIVE_CHAR_ACCENTED is 5 characters but 6
        // UTF-8 bytes (é is 2 bytes), so this genuinely distinguishes the two.
        use crate::unicode_fixtures::FIVE_CHAR_ACCENTED;
        assert_eq!(
            eval(&format!(
                "(display (string-length \"{FIVE_CHAR_ACCENTED}\"))"
            ))
            .unwrap(),
            "5"
        );
    }

    #[test]
    fn string_ref_reaches_a_multi_byte_character_by_position_not_byte_offset() {
        use crate::unicode_fixtures::FIVE_CHAR_ACCENTED;
        assert_eq!(
            eval(&format!(
                "(display (string-ref \"{FIVE_CHAR_ACCENTED}\" 1))"
            ))
            .unwrap(),
            "é"
        );
        assert_eq!(
            eval(&format!(
                "(display (string-ref \"{FIVE_CHAR_ACCENTED}\" 2))"
            ))
            .unwrap(),
            "l"
        );
    }

    // --- B10 E2: string=?/string<?/string>? (spec 6.1) ---

    #[test]
    fn string_equal_is_true_for_two_equal_strings() {
        assert_eq!(eval("(display (string=? \"abc\" \"abc\"))").unwrap(), "#t");
    }

    #[test]
    fn string_equal_is_false_for_two_unequal_strings() {
        assert_eq!(eval("(display (string=? \"abc\" \"abd\"))").unwrap(), "#f");
    }

    #[test]
    fn string_less_than_is_true_when_the_first_string_comes_before() {
        assert_eq!(eval("(display (string<? \"abc\" \"abd\"))").unwrap(), "#t");
    }

    #[test]
    fn string_less_than_is_false_with_reversed_operands() {
        assert_eq!(eval("(display (string<? \"abd\" \"abc\"))").unwrap(), "#f");
    }

    #[test]
    fn string_less_than_is_false_for_two_equal_strings() {
        assert_eq!(eval("(display (string<? \"abc\" \"abc\"))").unwrap(), "#f");
    }

    #[test]
    fn string_greater_than_is_shown_both_true_and_false() {
        assert_eq!(eval("(display (string>? \"abd\" \"abc\"))").unwrap(), "#t");
        assert_eq!(eval("(display (string>? \"abc\" \"abd\"))").unwrap(), "#f");
    }

    #[test]
    fn string_greater_than_is_false_for_two_equal_strings() {
        assert_eq!(eval("(display (string>? \"abc\" \"abc\"))").unwrap(), "#f");
    }

    // --- B10 E3: string/symbol/char-list conversions (spec 6.1, 6.2) ---

    #[test]
    fn symbol_to_string_converts_a_symbol_to_its_name() {
        assert_eq!(
            eval("(display (symbol->string (quote hello)))").unwrap(),
            "hello"
        );
    }

    #[test]
    fn string_to_symbol_converts_a_string_to_a_symbol() {
        assert_eq!(
            eval("(display (string->symbol \"world\"))").unwrap(),
            "world"
        );
    }

    #[test]
    fn list_to_string_builds_a_string_from_a_character_list() {
        assert_eq!(
            eval("(display (list->string (list #\\h #\\i)))").unwrap(),
            "hi"
        );
    }

    #[test]
    fn string_to_list_converts_a_string_to_a_character_list() {
        assert_eq!(eval("(display (string->list \"ab\"))").unwrap(), "(a b)");
    }

    #[test]
    fn symbol_string_round_trip_reproduces_the_original_string() {
        assert_eq!(
            eval("(display (symbol->string (string->symbol \"round-trip\")))").unwrap(),
            "round-trip"
        );
    }

    #[test]
    fn string_list_round_trip_reproduces_the_original_string() {
        assert_eq!(
            eval("(display (list->string (string->list \"hello\")))").unwrap(),
            "hello"
        );
    }

    // --- B10 E4: string-upcase/string-downcase (spec 6.1) ---

    #[test]
    fn string_upcase_converts_to_all_uppercase() {
        assert_eq!(eval("(display (string-upcase \"abc\"))").unwrap(), "ABC");
    }

    #[test]
    fn string_downcase_converts_to_all_lowercase() {
        assert_eq!(eval("(display (string-downcase \"ABC\"))").unwrap(), "abc");
    }

    #[test]
    fn string_upcase_handles_a_unicode_case_conversion_that_changes_length() {
        // qa test-design review (msg #170): an ASCII-only test ("abc" ->
        // "ABC") can't catch a length-changing Unicode case conversion --
        // confirmed reproducible: German sharp-s uppercases to "STRASSE"
        // (6 characters become 7, since sharp-s has no single-character
        // uppercase form).
        use crate::unicode_fixtures::GERMAN_SHARP_S;
        assert_eq!(
            eval(&format!("(display (string-upcase \"{GERMAN_SHARP_S}\"))")).unwrap(),
            "STRASSE"
        );
    }

    // --- B10 E5: char<->integer, char=?/char<?, char predicates (spec 6.2) ---

    #[test]
    fn char_to_integer_of_a_gives_its_code_point() {
        assert_eq!(eval("(display (char->integer #\\A))").unwrap(), "65");
    }

    #[test]
    fn integer_to_char_of_66_gives_b() {
        assert_eq!(eval("(display (integer->char 66))").unwrap(), "B");
    }

    #[test]
    fn char_equal_is_shown_both_true_and_false() {
        assert_eq!(eval("(display (char=? #\\a #\\a))").unwrap(), "#t");
        assert_eq!(eval("(display (char=? #\\a #\\b))").unwrap(), "#f");
    }

    #[test]
    fn char_less_than_is_shown_both_true_and_false() {
        assert_eq!(eval("(display (char<? #\\a #\\b))").unwrap(), "#t");
        assert_eq!(eval("(display (char<? #\\b #\\a))").unwrap(), "#f");
    }

    #[test]
    fn char_alphabetic_is_shown_both_true_and_false() {
        assert_eq!(eval("(display (char-alphabetic? #\\a))").unwrap(), "#t");
        assert_eq!(eval("(display (char-alphabetic? #\\5))").unwrap(), "#f");
    }

    #[test]
    fn char_alphabetic_is_true_for_a_non_ascii_letter() {
        // qa test-design review (msg #171): ASCII-only coverage of
        // char-alphabetic?/char-numeric?/char-whitespace? can't catch a
        // regression to ASCII-only predicates, since Rust's underlying
        // is_alphabetic()/is_numeric()/is_whitespace() are Unicode-aware.
        use crate::unicode_fixtures::ACCENTED_LETTER;
        assert_eq!(
            eval(&format!(
                "(display (char-alphabetic? #\\{ACCENTED_LETTER}))"
            ))
            .unwrap(),
            "#t"
        );
    }

    #[test]
    fn char_numeric_is_shown_both_true_and_false() {
        assert_eq!(eval("(display (char-numeric? #\\5))").unwrap(), "#t");
        assert_eq!(eval("(display (char-numeric? #\\a))").unwrap(), "#f");
    }

    #[test]
    fn char_whitespace_is_shown_both_true_and_false() {
        assert_eq!(eval("(display (char-whitespace? #\\space))").unwrap(), "#t");
        assert_eq!(eval("(display (char-whitespace? #\\a))").unwrap(), "#f");
    }

    // --- B10 E6: character literals read correctly from source (spec 6.2) ---

    #[test]
    fn an_individual_character_literal_reads_as_itself() {
        assert_eq!(eval("(display (char->integer #\\a))").unwrap(), "97");
    }

    #[test]
    fn the_named_space_literal_has_code_point_thirty_two() {
        assert_eq!(eval("(display (char->integer #\\space))").unwrap(), "32");
    }

    #[test]
    fn the_named_newline_literal_has_code_point_ten() {
        assert_eq!(eval("(display (char->integer #\\newline))").unwrap(), "10");
    }

    #[test]
    fn the_named_tab_literal_has_code_point_nine() {
        assert_eq!(eval("(display (char->integer #\\tab))").unwrap(), "9");
    }

    // --- B10 E7: length/indexing count by character, not byte (spec 6.1) ---

    #[test]
    fn a_plain_letter_plus_one_accented_character_is_length_two() {
        use crate::unicode_fixtures::TWO_CHAR_ACCENTED;
        assert_eq!(
            eval(&format!(
                "(display (string-length \"{TWO_CHAR_ACCENTED}\"))"
            ))
            .unwrap(),
            "2"
        );
    }

    #[test]
    fn position_zero_is_the_plain_letter_and_position_one_is_the_accented_character() {
        // Confirms the two are at their correct respective positions, not
        // swapped -- position 0 is the single-byte plain letter, position 1
        // is the two-byte accented character.
        use crate::unicode_fixtures::TWO_CHAR_ACCENTED;
        assert_eq!(
            eval(&format!("(display (string-ref \"{TWO_CHAR_ACCENTED}\" 0))")).unwrap(),
            "a"
        );
        assert_eq!(
            eval(&format!("(display (string-ref \"{TWO_CHAR_ACCENTED}\" 1))")).unwrap(),
            "é"
        );
    }

    // --- B10 E8: integration: all seventeen demo expressions in one program ---

    #[test]
    fn all_seventeen_demo_expressions_produce_exactly_the_prescribed_output() {
        use crate::unicode_fixtures::TWO_CHAR_ACCENTED;
        assert_eq!(
            eval(&format!(
                "(display (string-length \"hello\")) (newline) \
                 (display (string-ref \"hello\" 1)) (newline) \
                 (display (substring \"hello\" 1 4)) (newline) \
                 (display (string-append \"foo\" \"bar\")) (newline) \
                 (display (string=? \"abc\" \"abc\")) (newline) \
                 (display (string<? \"abc\" \"abd\")) (newline) \
                 (display (string-upcase \"abc\")) (newline) \
                 (display (symbol->string (quote hello))) (newline) \
                 (display (string->symbol \"world\")) (newline) \
                 (display (char->integer #\\A)) (newline) \
                 (display (integer->char 66)) (newline) \
                 (display (char-alphabetic? #\\a)) (newline) \
                 (display (char-numeric? #\\5)) (newline) \
                 (display (list->string (list #\\h #\\i))) (newline) \
                 (display (string->list \"ab\")) (newline) \
                 (display (string-length \"{TWO_CHAR_ACCENTED}\")) (newline) \
                 (display (string-ref \"{TWO_CHAR_ACCENTED}\" 1)) (newline)"
            ))
            .unwrap(),
            "5\ne\nell\nfoobar\n#t\n#t\nABC\nhello\nworld\n65\nB\n#t\n#t\nhi\n(a b)\n2\né\n"
        );
    }

    // --- B11 E1: vector construction, indexing, and bounds errors (spec 4.5) ---

    #[test]
    fn a_vector_built_from_a_sequence_reads_back_each_position() {
        assert_eq!(
            eval("(display (vector-ref (vector 1 2 3) 1))").unwrap(),
            "2"
        );
    }

    #[test]
    fn vector_set_replaces_a_position_in_place_and_is_observed_afterward() {
        assert_eq!(
            eval("(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector-ref v 1))")
                .unwrap(),
            "99"
        );
    }

    #[test]
    fn vector_length_counts_its_elements() {
        assert_eq!(
            eval("(display (vector-length (vector 1 2 3)))").unwrap(),
            "3"
        );
    }

    #[test]
    fn make_vector_with_no_fill_defaults_to_zero() {
        assert_eq!(eval("(display (make-vector 3))").unwrap(), "#(0 0 0)");
    }

    #[test]
    fn make_vector_with_an_explicit_non_default_fill_uses_it_for_every_position() {
        assert_eq!(eval("(display (make-vector 3 7))").unwrap(), "#(7 7 7)");
    }

    #[test]
    fn vector_ref_past_the_end_is_a_clean_runtime_error() {
        let err = eval("(display (vector-ref (vector 1 2 3) 3))").unwrap_err();
        assert_eq!(err.message, "vector-ref index 3 is out of range");
    }

    #[test]
    fn vector_set_past_the_end_is_a_clean_runtime_error_distinct_from_the_read_case() {
        let err = eval("(vector-set! (vector 1 2 3) 3 99)").unwrap_err();
        assert_eq!(err.message, "vector-set! index 3 is out of range");
    }

    // --- B11 E2: vector/list conversion and whole-vector fill (spec 4.5) ---

    #[test]
    fn vector_to_list_reflects_prior_mutation() {
        assert_eq!(
            eval("(define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector->list v))")
                .unwrap(),
            "(1 99 3)"
        );
    }

    #[test]
    fn list_to_vector_converts_a_list_and_displays_as_a_vector() {
        assert_eq!(
            eval("(display (list->vector (list 1 2)))").unwrap(),
            "#(1 2)"
        );
    }

    #[test]
    fn vector_fill_overwrites_every_position_not_just_one() {
        assert_eq!(
            eval("(define v (vector 1 2 3)) (vector-fill! v 9) (display v)").unwrap(),
            "#(9 9 9)"
        );
    }

    #[test]
    fn a_list_to_vector_to_list_round_trip_reproduces_the_original() {
        assert_eq!(
            eval("(display (vector->list (list->vector (list 1 2 3))))").unwrap(),
            "(1 2 3)"
        );
    }

    // --- B11 E3: vector literals read and evaluate correctly (spec 3.1, 4.5) ---

    #[test]
    fn a_vector_literal_displays_as_itself() {
        assert_eq!(eval("(display #(1 2 3))").unwrap(), "#(1 2 3)");
    }

    #[test]
    fn a_vector_literal_is_genuinely_a_vector_not_just_text_that_displays_right() {
        assert_eq!(eval("(display (vector? #(1 2 3)))").unwrap(), "#t");
        assert_eq!(eval("(display (vector-ref #(1 2 3) 2))").unwrap(), "3");
    }

    // --- B11 E4: hash table create/store/retrieve/remove (spec 4.6) ---

    #[test]
    fn a_stored_value_is_retrieved_by_its_key() {
        assert_eq!(
            eval(
                "(define h (make-hash)) (hash-set! h (quote a) 1) (display (hash-ref h (quote a)))"
            )
            .unwrap(),
            "1"
        );
    }

    #[test]
    fn hash_count_reports_the_number_of_stored_entries() {
        assert_eq!(
            eval(
                "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
                 (display (hash-count h))"
            )
            .unwrap(),
            "2"
        );
    }

    #[test]
    fn hash_ref_with_a_fallback_returns_it_for_a_missing_key() {
        assert_eq!(
            eval("(display (hash-ref (make-hash) (quote c) \"nope\"))").unwrap(),
            "nope"
        );
    }

    #[test]
    fn hash_ref_without_a_fallback_on_a_missing_key_is_a_clean_error_distinct_from_the_fallback_case()
     {
        let err = eval("(hash-ref (make-hash) (quote c))").unwrap_err();
        assert!(err.message.contains("not found"));
    }

    #[test]
    fn hash_ref_error_message_formats_a_cross_type_cyclic_missing_key_without_crashing() {
        // qa test-design warning (msg #200): "key not found" error messages
        // Display-format the missing key -- a second, previously
        // undisclosed entry point to the cross-type Pair/Vector cycle
        // crash, distinct from a direct `display` call in the user's own
        // source. Regression-pins that this path stays cycle-safe.
        let err = eval(
            "(define p (cons 1 2)) (define v (vector p)) (set-cdr! p v) \
             (hash-ref (make-hash) v)",
        )
        .unwrap_err();
        assert!(err.message.contains("not found"));
    }

    #[test]
    fn hash_has_key_reflects_removal() {
        assert_eq!(
            eval(
                "(define h (make-hash)) (hash-set! h (quote a) 1) \
                 (hash-remove! h (quote a)) (display (hash-has-key? h (quote a)))"
            )
            .unwrap(),
            "#f"
        );
    }

    #[test]
    fn hash_keys_are_compared_by_deep_structural_equality_not_identity() {
        // Two SEPARATELY-built but structurally identical compound keys
        // (lists, not symbols) must find the same entry -- mirroring B9's
        // member/assoc rigor for equal?-based lookup.
        assert_eq!(
            eval(
                "(define h (make-hash)) (hash-set! h (list 1 2) 42) \
                 (display (hash-ref h (list 1 2)))"
            )
            .unwrap(),
            "42"
        );
    }

    // --- B11 E5: hash-keys returns deterministic insertion order (spec 4.6) ---

    #[test]
    fn hash_keys_come_back_in_insertion_order_for_two_entries() {
        assert_eq!(
            eval(
                "(define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
                 (display (hash-keys h))"
            )
            .unwrap(),
            "(a b)"
        );
    }

    #[test]
    fn hash_keys_reflect_a_removal_followed_by_a_re_insertion_going_to_the_end() {
        assert_eq!(
            eval(
                "(define h (make-hash)) \
                 (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) (hash-set! h (quote c) 3) \
                 (hash-remove! h (quote a)) (hash-set! h (quote a) 99) \
                 (display (hash-keys h))"
            )
            .unwrap(),
            "(b c a)"
        );
    }

    // --- B11 E6: integration: all twelve demo expressions (spec 4.5, 4.6) ---

    #[test]
    fn all_twelve_demo_expressions_produce_exactly_the_prescribed_output() {
        assert_eq!(
            eval(
                "(define v (vector 1 2 3)) (display (vector-ref v 1)) (newline) \
                 (vector-set! v 1 99) (display (vector-ref v 1)) (newline) \
                 (display (vector-length v)) (newline) \
                 (display (vector->list v)) (newline) \
                 (display (make-vector 3 0)) (newline) \
                 (display (list->vector (cons 1 (cons 2 (quote ()))))) (newline) \
                 (display #(1 2 3)) (newline) \
                 (define h (make-hash)) (hash-set! h (quote a) 1) (hash-set! h (quote b) 2) \
                 (display (hash-count h)) (newline) \
                 (display (hash-keys h)) (newline) \
                 (display (hash-ref h (quote c) \"nope\")) (newline) \
                 (display (hash-has-key? h (quote a))) (newline) \
                 (hash-remove! h (quote a)) (display (hash-has-key? h (quote a))) (newline)"
            )
            .unwrap(),
            "2\n99\n3\n(1 99 3)\n#(0 0 0)\n#(1 2)\n#(1 2 3)\n2\n(a b)\nnope\n#t\n#f\n"
        );
    }

    // --- qa test-design warning (msg #189): vectors became mutable via
    // vector-set! in the same behaviour that added them, so a self-
    // referential vector is constructible from ordinary source text --
    // equal? must terminate (not hang) and display must print an ellipsis
    // (not crash the process with a native stack overflow), mirroring the
    // identical cycle-safety fixes this project already made for pairs. ---

    #[test]
    fn equal_terminates_on_a_vector_made_self_referential_via_vector_set() {
        assert_eq!(
            eval("(define v (vector 1 2 3)) (vector-set! v 0 v) (display (equal? v v))").unwrap(),
            "#t"
        );
    }

    #[test]
    fn equal_terminates_comparing_two_separately_built_self_referential_vectors() {
        assert_eq!(
            eval(
                "(define v (vector 1 2 3)) (vector-set! v 0 v) \
                 (define w (vector 1 2 3)) (vector-set! w 0 w) \
                 (display (equal? v w))"
            )
            .unwrap(),
            "#t"
        );
    }

    #[test]
    fn displaying_a_self_referential_vector_terminates_with_an_ellipsis_not_a_crash() {
        assert_eq!(
            eval("(define v (vector 1 2 3)) (vector-set! v 0 v) (display v)").unwrap(),
            "#(#(...) 2 3)"
        );
    }

    #[test]
    fn hash_has_key_terminates_on_a_self_referential_vector_key_not_a_hang() {
        // The same unguarded value_equal call underlies hash-ref/hash-set!/
        // hash-has-key?'s key comparison, so this bug was transparently
        // reachable through hash tables too, with no equal? call visible in
        // the user's source (qa test-design warning msg #189).
        assert_eq!(
            eval(
                "(define v (vector 1 2 3)) (vector-set! v 0 v) \
                 (define h (make-hash)) (hash-set! h v 1) \
                 (display (hash-has-key? h v))"
            )
            .unwrap(),
            "#t"
        );
    }

    // --- qa test-design warning (msg #189): coverage gaps, lower severity ---

    #[test]
    fn a_vector_mutation_through_one_binding_is_visible_through_an_aliased_binding() {
        // Mirrors the same aliasing gap already found and fixed once for
        // pairs (qa msg #145): every existing vector-mutation test read
        // back through the same binding that performed the write.
        assert_eq!(
            eval(
                "(define v (vector 1 2 3)) (define alias v) \
                 (vector-set! alias 0 99) (display (vector-ref v 0))"
            )
            .unwrap(),
            "99"
        );
    }

    #[test]
    fn make_vector_with_a_negative_length_is_a_clean_runtime_error() {
        assert!(eval("(display (make-vector -1))").is_err());
    }

    // --- warden security review (msgs #191/#192): make-vector must reject
    // an unbounded length with a clean error, not hand an arbitrary i64
    // straight to the allocator as one uncontrolled up-front request ---

    #[test]
    fn make_vector_past_the_maximum_length_is_a_clean_runtime_error_not_an_allocation() {
        // Exactly one past the limit, matching this codebase's own
        // established boundary-test convention (MAX_CALL_DEPTH,
        // MAX_NESTING_DEPTH) -- not just some arbitrarily huge number,
        // which would leave the true boundary itself unverified (qa
        // test-design review, msg #203).
        let err = eval(&format!(
            "(display (make-vector {}))",
            MAX_VECTOR_LENGTH + 1
        ))
        .unwrap_err();
        assert!(err.message.contains("exceeds the maximum"));
    }

    #[test]
    fn make_vector_at_exactly_the_maximum_length_still_succeeds() {
        assert_eq!(
            eval(&format!(
                "(display (vector-length (make-vector {MAX_VECTOR_LENGTH})))"
            ))
            .unwrap(),
            MAX_VECTOR_LENGTH.to_string()
        );
    }

    // --- warden security review (msgs #191/#192): a cross-type cycle
    // alternating through a mutable Pair and a mutable Vector must not
    // crash display with a native stack overflow, the same way a same-
    // type (pure Pair or pure Vector) cycle already doesn't ---

    #[test]
    fn displaying_a_cross_type_pair_and_vector_cycle_terminates_instead_of_crashing() {
        assert_eq!(
            eval("(define p (cons 1 2)) (define v (vector p)) (set-cdr! p v) (display p)").unwrap(),
            "(1 . #((...)))"
        );
    }

    #[test]
    fn displaying_a_vector_containing_a_pair_that_contains_that_same_vector_terminates() {
        assert_eq!(
            eval("(define v (vector 1)) (define p (cons v 2)) (vector-set! v 0 p) (display v)")
                .unwrap(),
            "#((#(...) . 2))"
        );
    }

    #[test]
    fn a_shared_but_non_cyclic_sub_list_referenced_twice_still_prints_in_full_both_times() {
        // Confirms the fix's ancestors set tracks only the CURRENT print
        // path (popped again once a subtree finishes), not every address
        // ever seen -- a DAG (the same sub-list reachable from two places,
        // but not forming a cycle) must not be mistaken for a cycle and
        // truncated on its second occurrence.
        assert_eq!(
            eval("(define x (list 1 2)) (display (list x x))").unwrap(),
            "((1 2) (1 2))"
        );
    }

    #[test]
    fn equal_between_two_distinct_content_identical_hash_tables_is_false() {
        // Hash tables compare by eq?-style reference identity for the table
        // itself, not by deep content comparison (documented design) --
        // this was implemented correctly but had no test confirming it.
        assert_eq!(
            eval(
                "(define h1 (make-hash)) (hash-set! h1 (quote a) 1) \
                 (define h2 (make-hash)) (hash-set! h2 (quote a) 1) \
                 (display (equal? h1 h2))"
            )
            .unwrap(),
            "#f"
        );
    }

    // --- B12 E1: read returns data unevaluated, advances, EOF both ways
    // (spec 4.8) ---

    #[test]
    fn read_returns_data_unevaluated_not_the_computed_result() {
        let out = eval_with_stdin(
            "(define d (read)) (write d) (newline) (display (+ 1 2))",
            "(+ 1 2)",
        )
        .unwrap();
        assert_eq!(out, "(+ 1 2)\n3");
    }

    #[test]
    fn read_advances_across_two_consecutive_calls_not_single_shot() {
        let out = eval_with_stdin("(display (read)) (display (read))", "1 2").unwrap();
        assert_eq!(out, "12");
    }

    #[test]
    fn read_parses_a_single_datum_spread_across_many_lines() {
        let stdin: String = (0..20).map(|i| format!("{i}\n")).collect();
        let stdin = format!("(\n{stdin})");
        let out = eval_with_stdin("(display (length (read)))", &stdin).unwrap();
        assert_eq!(out, "20");
    }

    #[test]
    fn read_on_a_datum_spread_across_many_lines_completes_quickly_not_quadratically() {
        // Regression test for warden security review msg #208: native_read
        // used to re-tokenize its whole accumulated buffer from scratch
        // after every single new line, making a datum spread across N
        // lines cost O(N^2) -- confirmed pre-fix to still be running after
        // 60s at N=100,000. The exponential-backoff retry fix keeps total
        // re-parse work linear in the final size; this completing well
        // within the default test timeout (rather than hanging) is the
        // actual regression signal, not a precise timing assertion.
        let stdin: String = (0..100_000).map(|i| format!("{i}\n")).collect();
        let stdin = format!("(\n{stdin})");
        let out = eval_with_stdin("(display (length (read)))", &stdin).unwrap();
        assert_eq!(out, "100000");
    }

    #[test]
    fn eof_object_predicate_is_true_for_the_eof_marker_and_false_for_an_ordinary_value() {
        let out = eval_with_stdin(
            "(display (eof-object? (read))) (display (eof-object? (read)))",
            "1",
        )
        .unwrap();
        assert_eq!(out, "#f#t");
    }

    // --- B12 E2: read-line, terminator genuinely stripped (spec 4.8) ---

    #[test]
    fn read_line_reads_successive_lines_then_the_eof_marker() {
        let out = eval_with_stdin(
            "(display (read-line)) (newline) (display (read-line)) (newline) \
             (display (eof-object? (read-line))) (newline)",
            "hello\nworld\n",
        )
        .unwrap();
        assert_eq!(out, "hello\nworld\n#t\n");
    }

    #[test]
    fn read_line_strips_the_line_terminator_genuinely_not_just_invisibly() {
        // 5, not 6 -- confirms the '\n' itself was removed, not merely
        // unprinted at the end of an otherwise-untouched 6-character string.
        assert_eq!(
            eval_with_stdin("(display (string-length (read-line)))", "hello\n").unwrap(),
            "5"
        );
    }

    #[test]
    fn read_line_returns_a_final_line_with_no_trailing_newline_then_eof_next() {
        // Distinguishes "nothing left to read" (EOF) from "one more line,
        // just not newline-terminated" -- both look like "no '\n' found in
        // the buffer" internally, but must produce different results.
        let out = eval_with_stdin(
            "(display (read-line)) (newline) (display (eof-object? (read-line)))",
            "last",
        )
        .unwrap();
        assert_eq!(out, "last\n#t");
    }

    // --- B12 E3: display prints raw text (spec 3.2) ---

    #[test]
    fn display_prints_a_strings_embedded_newline_as_a_real_line_break() {
        assert_eq!(eval("(display \"a\\nb\")").unwrap(), "a\nb");
    }

    #[test]
    fn display_prints_a_character_as_the_bare_character_itself() {
        assert_eq!(eval("(display #\\a)").unwrap(), "a");
        assert_eq!(eval("(display #\\space)").unwrap(), " ");
    }

    // --- B12 E4: write prints machine-readable, re-readable text; ordinary
    // values look identical under both styles (spec 3.2) ---

    #[test]
    fn write_prints_a_strings_embedded_newline_as_a_literal_backslash_n() {
        assert_eq!(eval("(write \"a\\nb\")").unwrap(), "\"a\\nb\"");
    }

    #[test]
    fn write_prints_a_symbol_bare_with_no_quoting() {
        assert_eq!(eval("(write (quote sym))").unwrap(), "sym");
    }

    #[test]
    fn write_prints_a_non_printing_character_named_contrasted_with_displays_bare_form() {
        assert_eq!(eval("(write #\\space)").unwrap(), "#\\space");
        assert_eq!(eval("(display #\\space)").unwrap(), " ");
    }

    #[test]
    fn write_and_display_produce_identical_output_for_ordinary_values() {
        assert_eq!(eval("(write 42)").unwrap(), eval("(display 42)").unwrap());
        assert_eq!(
            eval("(write (list 1 2 3))").unwrap(),
            eval("(display (list 1 2 3))").unwrap()
        );
    }

    // --- B12 E5: all output flushed, none dropped, even interleaved with
    // reads (spec 4.8) ---

    #[test]
    fn all_output_is_present_and_in_order_when_reads_and_writes_are_interleaved() {
        let out = eval_with_stdin(
            "(display \"start\") (newline) (display (read-line)) (newline) (display \"end\")",
            "middle\n",
        )
        .unwrap();
        assert_eq!(out, "start\nmiddle\nend");
    }

    // --- B12 E6: integration, all three DEMO scenarios verbatim ---

    #[test]
    fn case_a_read_returns_unevaluated_data_alongside_the_separately_computed_result() {
        let out = eval_with_stdin(
            "(define d (read)) (write d) (newline) (display (+ 1 2)) (newline)",
            "(+ 1 2)\n",
        )
        .unwrap();
        assert_eq!(out, "(+ 1 2)\n3\n");
    }

    #[test]
    fn case_b_two_lines_then_an_eof_check_matches_the_spec_exactly() {
        let out = eval_with_stdin(
            "(display (read-line)) (newline) (display (read-line)) (newline) \
             (display (eof-object? (read-line))) (newline)",
            "hello\nworld\n",
        )
        .unwrap();
        assert_eq!(out, "hello\nworld\n#t\n");
    }

    #[test]
    fn case_c_write_versus_display_matches_the_spec_exactly() {
        let out = eval(
            "(write \"a\\nb\") (newline) (display \"a\\nb\") (newline) \
             (write (quote sym)) (newline)",
        )
        .unwrap();
        assert_eq!(out, "\"a\\nb\"\na\nb\nsym\n");
    }

    // --- B13 E1: a template with no markers is literal data, not code
    // (spec 3.4) ---

    #[test]
    fn a_quasiquote_with_no_markers_is_literal_data_not_evaluated_as_code() {
        assert_eq!(eval("(display `(+ 1 2))").unwrap(), "(+ 1 2)");
    }

    // --- B13 E2: unquote inserts a single evaluated value in place ---

    #[test]
    fn unquote_inserts_a_single_evaluated_value_in_place() {
        assert_eq!(
            eval("(define x 10) (display `(a ,x c))").unwrap(),
            "(a 10 c)"
        );
    }

    #[test]
    fn two_separate_unquote_markers_are_each_independently_evaluated() {
        assert_eq!(
            eval("(define x 1) (define y 2) (display `(,x mid ,y))").unwrap(),
            "(1 mid 2)"
        );
    }

    #[test]
    fn unquoting_a_list_valued_expression_inserts_it_as_one_single_element() {
        // The critical distinguishing case versus E3's splicing: the list
        // value must appear NESTED as one element, not flattened in.
        assert_eq!(
            eval("(define mid (list 2 3 4)) (display `(1 ,mid 5))").unwrap(),
            "(1 (2 3 4) 5)"
        );
    }

    // --- B13 E3: unquote-splicing flattens a list value's elements in ---

    #[test]
    fn unquote_splicing_flattens_a_list_values_elements_directly_in() {
        assert_eq!(
            eval("(define mid (list 2 3 4)) (display `(1 ,@mid 5))").unwrap(),
            "(1 2 3 4 5)"
        );
    }

    #[test]
    fn unquote_splicing_an_inline_list_expression() {
        assert_eq!(eval("(display `(1 ,@(list 2 3) 4))").unwrap(), "(1 2 3 4)");
    }

    #[test]
    fn unquote_splicing_an_empty_list_contributes_zero_elements() {
        assert_eq!(eval("(display `(1 ,@(list) 2))").unwrap(), "(1 2)");
    }

    #[test]
    fn unquote_splicing_with_elements_on_both_sides() {
        assert_eq!(
            eval("(display `(0 1 ,@(list 2 3) 4 5))").unwrap(),
            "(0 1 2 3 4 5)"
        );
    }

    // --- B13 E4: nested quasiquote raises the level, matching markers
    // lower it, only a marker reaching level 0 is evaluated (spec 3.4) ---

    #[test]
    fn a_doubly_unquoted_value_inside_a_nested_quasiquote_reaches_level_zero_and_is_evaluated() {
        assert_eq!(
            eval("(define y 5) (display `(a `(b ,,y)))").unwrap(),
            "(a (quasiquote (b (unquote 5))))"
        );
    }

    #[test]
    fn a_singly_unquoted_value_inside_a_nested_quasiquote_never_reaches_zero_and_stays_literal() {
        // Contrasts directly with the doubly-unquoted case above: a single
        // comma only lowers the level from 2 to 1, not to 0, so y itself
        // (the symbol) is never substituted.
        assert_eq!(
            eval("(define y 5) (display `(a `(b ,y)))").unwrap(),
            "(a (quasiquote (b (unquote y))))"
        );
    }

    #[test]
    fn unquote_splicing_nested_inside_a_second_backquote_does_not_prematurely_splice() {
        // The splicing counterpart to the singly-unquoted case above: at
        // level 2, a single `,@` doesn't reach level 0 either, so it must
        // be reconstructed as literal (unquote-splicing ...) data instead
        // of actually splicing in at this (wrong) level.
        assert_eq!(
            eval("(display `(a `(b ,@c)))").unwrap(),
            "(a (quasiquote (b (unquote-splicing c))))"
        );
    }

    #[test]
    fn unquote_splicings_own_operand_correctly_lowers_to_the_next_level_down() {
        // Distinguishes a correctly-computed "one level down" for
        // unquote-splicing's operand (when the splice itself doesn't fire,
        // since level != 1) from an incorrectly-computed one: only with
        // the right level does the operand's OWN nested unquote (,@,w) in
        // turn reach level 0 and get evaluated to 99, rather than staying
        // literal as the unevaluated symbol w.
        assert_eq!(
            eval("(define w 99) (display `(a `(b ,@,w)))").unwrap(),
            "(a (quasiquote (b (unquote-splicing 99))))"
        );
    }

    #[test]
    fn quasiquoting_an_empty_list_produces_the_empty_list() {
        assert_eq!(eval("(display `())").unwrap(), "()");
    }

    // --- B13 E5: both markers work inside a vector template too ---

    #[test]
    fn unquote_works_inside_a_vector_template() {
        assert_eq!(
            eval("(define x 10) (display `#(1 ,x 3))").unwrap(),
            "#(1 10 3)"
        );
    }

    #[test]
    fn unquote_splicing_works_inside_a_vector_template() {
        assert_eq!(
            eval("(display `#(1 ,@(list 2 3) 4))").unwrap(),
            "#(1 2 3 4)"
        );
    }

    // --- B13 E6: integration, all five verbatim demo expressions ---

    #[test]
    fn all_five_demo_expressions_produce_exactly_the_prescribed_output() {
        let out = eval(
            "(define mid (list 2 3 4)) (write `(1 ,@mid 5)) (newline) \
             (define x 10) (display `(a ,x c)) (newline) \
             (display `(1 ,@(list 2 3) 4)) (newline) \
             (display `#(1 ,x 3)) (newline) \
             (define y 5) (display `(a `(b ,,y))) (newline)",
        )
        .unwrap();
        assert_eq!(
            out,
            "(1 2 3 4 5)\n(a 10 c)\n(1 2 3 4)\n#(1 10 3)\n(a (quasiquote (b (unquote 5))))\n"
        );
    }
}
