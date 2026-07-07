//! Bytecode virtual machine.

use std::collections::HashMap;
use std::io::Write;

use crate::bytecode::{Chunk, Const, Op};
use crate::value::Value;

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

const NATIVE_NAMES: [&str; 3] = ["display", "newline", "+"];

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

pub fn run(chunk: &Chunk, out: &mut impl Write) -> Result<(), RuntimeError> {
    let globals = default_globals();
    let mut stack: Vec<Value> = Vec::new();
    let code = &chunk.code;
    let mut ip = 0usize;

    loop {
        let opcode = *code
            .get(ip)
            .ok_or_else(|| error("ran off the end of the instruction stream without HALT"))?;
        ip += 1;

        match opcode {
            op if op == Op::Const as u8 => {
                let idx = read_u32(code, &mut ip)?;
                let value = const_to_value(constant_at(chunk, idx)?);
                stack.push(value);
            }
            op if op == Op::GetGlobal as u8 => {
                let idx = read_u32(code, &mut ip)?;
                let name = match constant_at(chunk, idx)? {
                    Const::Symbol(s) => s.clone(),
                    other => {
                        return Err(error(format!(
                            "GET_GLOBAL requires a symbol constant, found {other:?}"
                        )));
                    }
                };
                let value = globals
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| error(format!("unbound global: {name}")))?;
                stack.push(value);
            }
            op if op == Op::Call as u8 => {
                let argc = read_u8(code, &mut ip)? as usize;
                if stack.len() < argc + 1 {
                    return Err(error("stack underflow during CALL"));
                }
                let args = stack.split_off(stack.len() - argc);
                let callee = stack.pop().unwrap();
                let result = call_value(&callee, &args, out)?;
                stack.push(result);
            }
            op if op == Op::Pop as u8 => {
                stack
                    .pop()
                    .ok_or_else(|| error("stack underflow during POP"))?;
            }
            op if op == Op::Halt as u8 => break,
            other => return Err(error(format!("undefined opcode: {other}"))),
        }
    }

    out.flush().map_err(|e| error(e.to_string()))?;
    Ok(())
}

fn call_value(callee: &Value, args: &[Value], out: &mut impl Write) -> Result<Value, RuntimeError> {
    match callee {
        Value::Native(name) => call_native(name, args, out),
        other => Err(error(format!("cannot call a non-procedure value: {other}"))),
    }
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
        other => Err(error(format!("unknown native procedure: {other}"))),
    }
}

fn native_plus(args: &[Value]) -> Result<Value, RuntimeError> {
    let mut sum: i64 = 0;
    for arg in args {
        match arg {
            Value::Int(n) => {
                sum = sum
                    .checked_add(*n)
                    .ok_or_else(|| error("+ overflowed a 64-bit integer"))?;
            }
            other => return Err(error(format!("+ expects integer arguments, found {other}"))),
        }
    }
    Ok(Value::Int(sum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile_program;
    use crate::reader::read_program;

    fn eval(src: &str) -> Result<String, RuntimeError> {
        let forms = read_program(src).expect("valid source for this test");
        let chunk = compile_program(&forms).expect("compilable source for this test");
        let mut out = Vec::new();
        run(&chunk, &mut out)?;
        Ok(String::from_utf8(out).unwrap())
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
        use crate::bytecode::{Chunk as RawChunk, Const};
        let mut chunk = RawChunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_call(0);
        chunk.emit_pop();
        chunk.emit_halt();
        let mut out = Vec::new();
        assert!(run(&chunk, &mut out).is_err());
    }

    #[test]
    fn an_undefined_opcode_is_a_runtime_error_not_a_panic() {
        use crate::bytecode::Chunk as RawChunk;
        let mut chunk = RawChunk::new();
        chunk.code.push(255); // no opcode is numbered 255
        let mut out = Vec::new();
        assert!(run(&chunk, &mut out).is_err());
    }

    #[test]
    fn an_out_of_range_constant_index_is_a_runtime_error_not_a_panic() {
        use crate::bytecode::{Chunk as RawChunk, Op};
        let mut chunk = RawChunk::new();
        chunk.code.push(Op::Const as u8);
        chunk.code.extend_from_slice(&99u32.to_le_bytes());
        let mut out = Vec::new();
        assert!(run(&chunk, &mut out).is_err());
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
        use crate::bytecode::{Chunk as RawChunk, Const};
        let mut chunk = RawChunk::new();
        let one = chunk.add_const(Const::Int(1));
        chunk.emit_const(one);
        chunk.emit_call(1);
        chunk.emit_halt();
        let mut out = Vec::new();
        assert!(run(&chunk, &mut out).is_err());
    }
}
