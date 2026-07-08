//! MLBC bytecode container format: opcodes, chunk, encode/decode.

pub const MAGIC: [u8; 4] = *b"MLBC";
pub const VERSION_MAJOR: u8 = 1;
pub const VERSION_MINOR: u8 = 1;

/// Caps recursion depth when decoding nested `Const::List` constants from an
/// MLBC file, so a maliciously crafted artifact with deeply nested list
/// constants can't overflow the native stack during decode.
const MAX_CONST_NESTING_DEPTH: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    Const = 0,
    GetGlobal = 1,
    Call = 2,
    Pop = 3,
    Halt = 4,
    DefGlobal = 5,
    GetLocal = 6,
    Jump = 7,
    JumpIfFalse = 8,
    Return = 9,
    MakeFunction = 10,
    PushLocal = 11,
    SetLocal = 12,
    SetGlobal = 13,
    Dup = 14,
    Swap = 15,
    Eqv = 16,
    GetUpvalue = 17,
    SetUpvalue = 18,
    /// Same operand format as `Call` (one `u8` argc), but tells the VM this
    /// call is in tail position: reuse the current frame instead of
    /// recursing, so a chain of tail calls runs in O(1) native stack space
    /// regardless of length.
    TailCall = 19,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    Symbol(String),
    List(Vec<Const>),
    Vector(Vec<Const>),
    /// A dotted (improper) pair literal (spec 5.1), e.g. `(a . b)` or the
    /// tail of `(1 2 . 3)` -- distinct from `List`, which only ever
    /// represents a proper list.
    Pair(Box<Const>, Box<Const>),
    Unspecified,
}

impl Drop for Const {
    /// Without this, dropping a long dotted-list literal's `Const::Pair`
    /// chain would recurse once per element via Rust's default field-drop
    /// glue -- a chain length with no bound (it's the literal's element
    /// count, not nesting depth) -- crashing on the *compiling* thread's
    /// ordinary-sized stack even though the literal is a single flat form
    /// (warden security review, msg #146; confirmed to crash a plain
    /// `eval` at ~500,000 elements despite `encode_const`/`const_to_value`
    /// already being iterative, since the actual crash was here, at drop
    /// time, not during either of those). Detaches and drops the cdr chain
    /// with an explicit loop instead.
    fn drop(&mut self) {
        let Const::Pair(_, cdr) = self else {
            return;
        };
        let mut pending = vec![std::mem::replace(cdr.as_mut(), Const::Unspecified)];
        while let Some(mut current) = pending.pop() {
            if let Const::Pair(_, cdr) = &mut current {
                pending.push(std::mem::replace(cdr.as_mut(), Const::Unspecified));
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub constants: Vec<Const>,
    /// Number of fixed (non-rest) parameters. Zero for the top-level entry
    /// chunk, which takes no arguments.
    pub arity: u32,
    /// Whether the last parameter collects any extra arguments into a list.
    pub has_rest: bool,
}

impl Chunk {
    pub fn new() -> Self {
        Chunk::default()
    }

    pub fn add_const(&mut self, c: Const) -> u32 {
        self.constants.push(c);
        (self.constants.len() - 1) as u32
    }

    pub fn emit_const(&mut self, idx: u32) {
        self.code.push(Op::Const as u8);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    pub fn emit_get_global(&mut self, idx: u32) {
        self.code.push(Op::GetGlobal as u8);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    pub fn emit_def_global(&mut self, idx: u32) {
        self.code.push(Op::DefGlobal as u8);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    pub fn emit_get_local(&mut self, slot: u8) {
        self.code.push(Op::GetLocal as u8);
        self.code.push(slot);
    }

    pub fn emit_set_local(&mut self, slot: u8) {
        self.code.push(Op::SetLocal as u8);
        self.code.push(slot);
    }

    /// `depth` counts how many captured-environment links to walk up from
    /// the closure's own environment (1 = the immediately enclosing
    /// function's locals); `slot` is the position within that ancestor's
    /// locals once reached.
    pub fn emit_get_upvalue(&mut self, depth: u8, slot: u8) {
        self.code.push(Op::GetUpvalue as u8);
        self.code.push(depth);
        self.code.push(slot);
    }

    pub fn emit_set_upvalue(&mut self, depth: u8, slot: u8) {
        self.code.push(Op::SetUpvalue as u8);
        self.code.push(depth);
        self.code.push(slot);
    }

    pub fn emit_set_global(&mut self, idx: u32) {
        self.code.push(Op::SetGlobal as u8);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    pub fn emit_push_local(&mut self) {
        self.code.push(Op::PushLocal as u8);
    }

    pub fn emit_dup(&mut self) {
        self.code.push(Op::Dup as u8);
    }

    pub fn emit_swap(&mut self) {
        self.code.push(Op::Swap as u8);
    }

    pub fn emit_eqv(&mut self) {
        self.code.push(Op::Eqv as u8);
    }

    pub fn emit_make_function(&mut self, fn_index: u32) {
        self.code.push(Op::MakeFunction as u8);
        self.code.extend_from_slice(&fn_index.to_le_bytes());
    }

    pub fn emit_call(&mut self, argc: u8) {
        self.code.push(Op::Call as u8);
        self.code.push(argc);
    }

    pub fn emit_tail_call(&mut self, argc: u8) {
        self.code.push(Op::TailCall as u8);
        self.code.push(argc);
    }

    pub fn emit_pop(&mut self) {
        self.code.push(Op::Pop as u8);
    }

    pub fn emit_halt(&mut self) {
        self.code.push(Op::Halt as u8);
    }

    pub fn emit_return(&mut self) {
        self.code.push(Op::Return as u8);
    }

    /// Emits a jump opcode with a placeholder target and returns the byte
    /// offset of that 4-byte operand, to be patched later via [`Chunk::patch_jump`]
    /// once the real target address is known.
    pub fn emit_jump(&mut self, op: Op) -> usize {
        debug_assert!(matches!(op, Op::Jump | Op::JumpIfFalse));
        self.code.push(op as u8);
        let operand_pos = self.code.len();
        self.code.extend_from_slice(&0u32.to_le_bytes());
        operand_pos
    }

    /// Patches a jump operand (previously emitted at `operand_pos` by
    /// [`Chunk::emit_jump`]) to target the chunk's current end.
    pub fn patch_jump(&mut self, operand_pos: usize) {
        let target = (self.code.len() as u32).to_le_bytes();
        self.code[operand_pos..operand_pos + 4].copy_from_slice(&target);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Module {
    pub entry_index: u32,
    pub functions: Vec<Chunk>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BytecodeError {
    BadMagic,
    UnsupportedVersion { major: u8, minor: u8 },
    Truncated,
    OutOfRange(String),
}

impl std::fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytecodeError::BadMagic => write!(f, "not a MagicLisp bytecode file (bad magic)"),
            BytecodeError::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported MLBC version {major}.{minor}")
            }
            BytecodeError::Truncated => write!(f, "MLBC file is truncated or corrupted"),
            BytecodeError::OutOfRange(what) => {
                write!(f, "MLBC file has an invalid pointer: {what}")
            }
        }
    }
}

fn encode_const(out: &mut Vec<u8>, c: &Const) {
    match c {
        Const::Int(n) => {
            out.push(0);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Const::Float(n) => {
            out.push(6);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Const::Bool(b) => {
            out.push(1);
            out.push(if *b { 1 } else { 0 });
        }
        Const::Str(s) => {
            out.push(2);
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        Const::Symbol(s) => {
            out.push(3);
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        Const::List(items) => {
            out.push(4);
            out.extend_from_slice(&(items.len() as u32).to_le_bytes());
            for item in items {
                encode_const(out, item);
            }
        }
        Const::Unspecified => out.push(5),
        Const::Char(c) => {
            out.push(7);
            out.extend_from_slice(&(*c as u32).to_le_bytes());
        }
        Const::Vector(items) => {
            out.push(8);
            out.extend_from_slice(&(items.len() as u32).to_le_bytes());
            for item in items {
                encode_const(out, item);
            }
        }
        Const::Pair(car, cdr) => {
            // Walks the cdr spine iteratively instead of recursing: a
            // dotted-list literal `(1 2 3 ... N . tail)` is one flat pair
            // of parens (nesting depth 1, never touching the reader's
            // MAX_NESTING_DEPTH), but produces a `Const::Pair` chain N
            // deep -- chain length is program data, not nesting depth, so
            // it has no bound. Only each car (bounded by ordinary nesting
            // depth, like `List`/`Vector` items) still recurses.
            out.push(9);
            encode_const(out, car);
            let mut tail: &Const = cdr;
            loop {
                match tail {
                    Const::Pair(next_car, next_cdr) => {
                        out.push(9);
                        encode_const(out, next_car);
                        tail = next_cdr;
                    }
                    other => {
                        encode_const(out, other);
                        break;
                    }
                }
            }
        }
    }
}

pub fn encode(module: &Module) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.push(VERSION_MAJOR);
    out.push(VERSION_MINOR);
    out.extend_from_slice(&0u16.to_le_bytes()); // flags, reserved
    out.extend_from_slice(&module.entry_index.to_le_bytes());
    out.extend_from_slice(&(module.functions.len() as u32).to_le_bytes());
    for chunk in &module.functions {
        out.extend_from_slice(&chunk.arity.to_le_bytes());
        out.push(if chunk.has_rest { 1 } else { 0 });
        out.extend_from_slice(&(chunk.code.len() as u32).to_le_bytes());
        out.extend_from_slice(&chunk.code);
        out.extend_from_slice(&(chunk.constants.len() as u32).to_le_bytes());
        for c in &chunk.constants {
            encode_const(&mut out, c);
        }
    }
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], BytecodeError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(BytecodeError::Truncated)?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, BytecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, BytecodeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, BytecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, BytecodeError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, BytecodeError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn bytes_owned(&mut self, n: usize) -> Result<Vec<u8>, BytecodeError> {
        Ok(self.take(n)?.to_vec())
    }
}

fn decode_const(r: &mut Reader, depth: usize) -> Result<Const, BytecodeError> {
    if depth > MAX_CONST_NESTING_DEPTH {
        return Err(BytecodeError::Truncated);
    }
    let tag = r.u8()?;
    Ok(match tag {
        0 => Const::Int(r.i64()?),
        1 => Const::Bool(r.u8()? != 0),
        2 => {
            let len = r.u32()? as usize;
            let raw = r.bytes_owned(len)?;
            Const::Str(String::from_utf8(raw).map_err(|_| BytecodeError::Truncated)?)
        }
        3 => {
            let len = r.u32()? as usize;
            let raw = r.bytes_owned(len)?;
            Const::Symbol(String::from_utf8(raw).map_err(|_| BytecodeError::Truncated)?)
        }
        4 => {
            let count = r.u32()?;
            // Same unchecked-count discipline as fn_count/const_count below:
            // Vec::new() only grows as far as bytes actually taken from `r`.
            let mut items = Vec::new();
            for _ in 0..count {
                items.push(decode_const(r, depth + 1)?);
            }
            Const::List(items)
        }
        5 => Const::Unspecified,
        6 => Const::Float(r.f64()?),
        7 => {
            let scalar = r.u32()?;
            Const::Char(char::from_u32(scalar).ok_or(BytecodeError::Truncated)?)
        }
        8 => {
            let count = r.u32()?;
            let mut items = Vec::new();
            for _ in 0..count {
                items.push(decode_const(r, depth + 1)?);
            }
            Const::Vector(items)
        }
        9 => {
            let car = decode_const(r, depth + 1)?;
            let cdr = decode_const(r, depth + 1)?;
            Const::Pair(Box::new(car), Box::new(cdr))
        }
        _ => return Err(BytecodeError::Truncated),
    })
}

pub fn decode(bytes: &[u8]) -> Result<Module, BytecodeError> {
    let mut r = Reader::new(bytes);

    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err(BytecodeError::BadMagic);
    }
    let major = r.u8()?;
    let minor = r.u8()?;
    if major != VERSION_MAJOR || minor != VERSION_MINOR {
        return Err(BytecodeError::UnsupportedVersion { major, minor });
    }
    let _flags = r.u16()?;
    let entry_index = r.u32()?;
    let fn_count = r.u32()?;

    // fn_count/const_count come straight off the file, unchecked against the
    // remaining byte count: pre-sizing on them would let a tiny crafted file
    // request an arbitrarily large allocation. Vec::new() grows only as far
    // as bytes actually taken from `r` (which is itself bounds-checked), so a
    // bogus count still fails fast on the first out-of-range read.
    let mut functions = Vec::new();
    for _ in 0..fn_count {
        let arity = r.u32()?;
        let has_rest = r.u8()? != 0;
        let code_len = r.u32()? as usize;
        let code = r.bytes_owned(code_len)?;
        let const_count = r.u32()?;
        let mut constants = Vec::new();
        for _ in 0..const_count {
            constants.push(decode_const(&mut r, 0)?);
        }
        functions.push(Chunk {
            code,
            constants,
            arity,
            has_rest,
        });
    }

    if entry_index as usize >= functions.len() {
        return Err(BytecodeError::OutOfRange(format!(
            "entry_index {entry_index} does not refer to a function in a table of {}",
            functions.len()
        )));
    }

    Ok(Module {
        entry_index,
        functions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_module() -> Module {
        let mut chunk = Chunk::new();
        let plus = chunk.add_const(Const::Symbol("+".to_string()));
        let one = chunk.add_const(Const::Int(1));
        let two = chunk.add_const(Const::Int(2));
        let greeting = chunk.add_const(Const::Str("hi\n".to_string()));
        let flag = chunk.add_const(Const::Bool(true));
        let list = chunk.add_const(Const::List(vec![
            Const::Symbol("+".to_string()),
            Const::Int(1),
            Const::List(vec![Const::Int(2), Const::Unspecified]),
        ]));
        chunk.emit_get_global(plus);
        chunk.emit_const(one);
        chunk.emit_const(two);
        chunk.emit_call(2);
        chunk.emit_pop();
        chunk.emit_const(greeting);
        chunk.emit_pop();
        chunk.emit_const(flag);
        chunk.emit_pop();
        chunk.emit_const(list);
        chunk.emit_pop();
        chunk.emit_halt();
        Module {
            entry_index: 0,
            functions: vec![chunk],
        }
    }

    #[test]
    fn round_trips_a_module_byte_for_byte_through_encode_and_decode() {
        let module = sample_module();
        let bytes = encode(&module);
        let decoded = decode(&bytes).expect("valid module should decode");
        assert_eq!(decoded, module);
    }

    #[test]
    fn round_trips_a_float_constant_byte_for_byte() {
        let mut chunk = Chunk::new();
        let idx = chunk.add_const(Const::Float(3.5));
        chunk.emit_const(idx);
        chunk.emit_halt();
        let module = Module {
            entry_index: 0,
            functions: vec![chunk],
        };
        let bytes = encode(&module);
        let decoded = decode(&bytes).expect("valid module should decode");
        assert_eq!(decoded, module);
    }

    #[test]
    fn round_trips_a_char_constant_byte_for_byte() {
        let mut chunk = Chunk::new();
        let idx = chunk.add_const(Const::Char('a'));
        chunk.emit_const(idx);
        chunk.emit_halt();
        let module = Module {
            entry_index: 0,
            functions: vec![chunk],
        };
        let bytes = encode(&module);
        let decoded = decode(&bytes).expect("valid module should decode");
        assert_eq!(decoded, module);
    }

    #[test]
    fn round_trips_a_vector_constant_byte_for_byte() {
        let mut chunk = Chunk::new();
        let idx = chunk.add_const(Const::Vector(vec![
            Const::Int(1),
            Const::Str("x".to_string()),
            Const::Vector(vec![Const::Int(2)]),
        ]));
        chunk.emit_const(idx);
        chunk.emit_halt();
        let module = Module {
            entry_index: 0,
            functions: vec![chunk],
        };
        let bytes = encode(&module);
        let decoded = decode(&bytes).expect("valid module should decode");
        assert_eq!(decoded, module);
    }

    #[test]
    fn encoding_starts_with_the_mlbc_magic_and_version() {
        let bytes = encode(&sample_module());
        assert_eq!(&bytes[0..4], b"MLBC");
        assert_eq!(bytes[4], VERSION_MAJOR);
        assert_eq!(bytes[5], VERSION_MINOR);
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut bytes = encode(&sample_module());
        bytes[0..4].copy_from_slice(b"NOPE");
        assert_eq!(decode(&bytes), Err(BytecodeError::BadMagic));
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut bytes = encode(&sample_module());
        bytes[4] = 99;
        assert_eq!(
            decode(&bytes),
            Err(BytecodeError::UnsupportedVersion {
                major: 99,
                minor: VERSION_MINOR,
            })
        );
    }

    #[test]
    fn rejects_truncated_content() {
        let bytes = encode(&sample_module());
        let truncated = &bytes[..bytes.len() - 3];
        assert_eq!(decode(truncated), Err(BytecodeError::Truncated));
    }

    #[test]
    fn rejects_a_header_shorter_than_the_minimum_size() {
        let truncated = &MAGIC[..];
        assert_eq!(decode(truncated), Err(BytecodeError::Truncated));
    }

    #[test]
    fn rejects_an_out_of_range_entry_index() {
        let mut module = sample_module();
        module.entry_index = 7; // only one function exists, index 0
        let bytes = encode(&module);
        match decode(&bytes) {
            Err(BytecodeError::OutOfRange(_)) => {}
            other => panic!("expected OutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_huge_declared_function_count_without_over_allocating() {
        // A ~14-byte file claiming 4 billion functions must fail fast on the
        // first bounds check, not attempt a multi-gigabyte allocation.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.push(VERSION_MAJOR);
        bytes.push(VERSION_MINOR);
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        bytes.extend_from_slice(&0u32.to_le_bytes()); // entry_index
        bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // fn_count: absurd
        assert_eq!(decode(&bytes), Err(BytecodeError::Truncated));
    }

    #[test]
    fn rejects_a_huge_declared_constant_count_without_over_allocating() {
        let mut chunk = Chunk::new();
        chunk.emit_halt();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.push(VERSION_MAJOR);
        bytes.push(VERSION_MINOR);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // entry_index
        bytes.extend_from_slice(&1u32.to_le_bytes()); // fn_count = 1
        bytes.extend_from_slice(&(chunk.code.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&chunk.code);
        bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // const_count: absurd
        assert_eq!(decode(&bytes), Err(BytecodeError::Truncated));
    }

    #[test]
    fn every_bytecode_error_variant_displays_a_non_empty_message() {
        assert_eq!(
            BytecodeError::BadMagic.to_string(),
            "not a MagicLisp bytecode file (bad magic)"
        );
        assert_eq!(
            BytecodeError::UnsupportedVersion { major: 9, minor: 9 }.to_string(),
            "unsupported MLBC version 9.9"
        );
        assert_eq!(
            BytecodeError::Truncated.to_string(),
            "MLBC file is truncated or corrupted"
        );
        assert_eq!(
            BytecodeError::OutOfRange("entry_index 5".to_string()).to_string(),
            "MLBC file has an invalid pointer: entry_index 5"
        );
    }

    fn nested_list_const(depth: usize) -> Const {
        let mut c = Const::Int(0);
        for _ in 0..depth {
            c = Const::List(vec![c]);
        }
        c
    }

    fn module_with_const(c: Const) -> Module {
        let mut chunk = Chunk::new();
        let idx = chunk.add_const(c);
        chunk.emit_const(idx);
        chunk.emit_pop();
        chunk.emit_halt();
        Module {
            entry_index: 0,
            functions: vec![chunk],
        }
    }

    #[test]
    fn round_trips_a_constant_list_nested_to_exactly_the_configured_maximum_depth() {
        let module = module_with_const(nested_list_const(MAX_CONST_NESTING_DEPTH));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Ok(module));
    }

    #[test]
    fn rejects_a_constant_list_nested_one_deeper_than_the_configured_maximum() {
        let module = module_with_const(nested_list_const(MAX_CONST_NESTING_DEPTH + 1));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Err(BytecodeError::Truncated));
    }

    fn nested_vector_const(depth: usize) -> Const {
        let mut c = Const::Int(0);
        for _ in 0..depth {
            c = Const::Vector(vec![c]);
        }
        c
    }

    #[test]
    fn round_trips_a_constant_vector_nested_to_exactly_the_configured_maximum_depth() {
        let module = module_with_const(nested_vector_const(MAX_CONST_NESTING_DEPTH));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Ok(module));
    }

    #[test]
    fn rejects_a_constant_vector_nested_one_deeper_than_the_configured_maximum() {
        // Pins down that decode_const's recursion depth counter is
        // genuinely incremented per nested Vector level, the same as for
        // List above -- a `depth` that's read but never actually advanced
        // for this variant would let pathologically deep vector literals
        // blow the native stack instead of failing cleanly.
        let module = module_with_const(nested_vector_const(MAX_CONST_NESTING_DEPTH + 1));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Err(BytecodeError::Truncated));
    }

    #[test]
    fn round_trips_a_dotted_pair_constant_byte_for_byte() {
        let mut chunk = Chunk::new();
        let idx = chunk.add_const(Const::Pair(
            Box::new(Const::Symbol("a".to_string())),
            Box::new(Const::Symbol("b".to_string())),
        ));
        chunk.emit_const(idx);
        chunk.emit_halt();
        let module = Module {
            entry_index: 0,
            functions: vec![chunk],
        };
        let bytes = encode(&module);
        let decoded = decode(&bytes).expect("valid module should decode");
        assert_eq!(decoded, module);
    }

    fn nested_pair_const(depth: usize) -> Const {
        let mut c = Const::Int(0);
        for _ in 0..depth {
            c = Const::Pair(Box::new(c), Box::new(Const::Int(0)));
        }
        c
    }

    #[test]
    fn round_trips_a_constant_pair_nested_to_exactly_the_configured_maximum_depth() {
        let module = module_with_const(nested_pair_const(MAX_CONST_NESTING_DEPTH));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Ok(module));
    }

    #[test]
    fn rejects_a_constant_pair_nested_one_deeper_than_the_configured_maximum() {
        let module = module_with_const(nested_pair_const(MAX_CONST_NESTING_DEPTH + 1));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Err(BytecodeError::Truncated));
    }

    /// Chains via cdr, not car -- the shape a real dotted-list literal like
    /// `(1 2 3 ... . tail)` actually produces, unlike `nested_pair_const`
    /// above (which only exercises the car side's own depth counting).
    fn cdr_nested_pair_const(depth: usize) -> Const {
        let mut c = Const::Int(0);
        for _ in 0..depth {
            c = Const::Pair(Box::new(Const::Int(0)), Box::new(c));
        }
        c
    }

    #[test]
    fn round_trips_a_cdr_chained_constant_pair_nested_to_exactly_the_configured_maximum_depth() {
        let module = module_with_const(cdr_nested_pair_const(MAX_CONST_NESTING_DEPTH));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Ok(module));
    }

    #[test]
    fn rejects_a_cdr_chained_constant_pair_nested_one_deeper_than_the_configured_maximum() {
        let module = module_with_const(cdr_nested_pair_const(MAX_CONST_NESTING_DEPTH + 1));
        let bytes = encode(&module);
        assert_eq!(decode(&bytes), Err(BytecodeError::Truncated));
    }

    #[test]
    fn patch_jump_writes_the_current_end_of_code_as_the_absolute_target() {
        let mut chunk = Chunk::new();
        chunk.emit_pop(); // 1 byte of unrelated code before the jump
        let operand_pos = chunk.emit_jump(Op::Jump);
        chunk.emit_pop();
        chunk.emit_pop(); // more code after the jump, so target != the placeholder 0
        chunk.patch_jump(operand_pos);

        let expected_target = chunk.code.len() as u32;
        let written =
            u32::from_le_bytes(chunk.code[operand_pos..operand_pos + 4].try_into().unwrap());
        assert_eq!(written, expected_target);
        assert_ne!(written, 0, "the placeholder must actually be overwritten");
    }
}
