//! MLBC bytecode container format: opcodes, chunk, encode/decode.

pub const MAGIC: [u8; 4] = *b"MLBC";
pub const VERSION_MAJOR: u8 = 1;
pub const VERSION_MINOR: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    Const = 0,
    GetGlobal = 1,
    Call = 2,
    Pop = 3,
    Halt = 4,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i64),
    Bool(bool),
    Str(String),
    Symbol(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub constants: Vec<Const>,
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

    pub fn emit_call(&mut self, argc: u8) {
        self.code.push(Op::Call as u8);
        self.code.push(argc);
    }

    pub fn emit_pop(&mut self) {
        self.code.push(Op::Pop as u8);
    }

    pub fn emit_halt(&mut self) {
        self.code.push(Op::Halt as u8);
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

pub fn encode(module: &Module) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.push(VERSION_MAJOR);
    out.push(VERSION_MINOR);
    out.extend_from_slice(&0u16.to_le_bytes()); // flags, reserved
    out.extend_from_slice(&module.entry_index.to_le_bytes());
    out.extend_from_slice(&(module.functions.len() as u32).to_le_bytes());
    for chunk in &module.functions {
        out.extend_from_slice(&(chunk.code.len() as u32).to_le_bytes());
        out.extend_from_slice(&chunk.code);
        out.extend_from_slice(&(chunk.constants.len() as u32).to_le_bytes());
        for c in &chunk.constants {
            match c {
                Const::Int(n) => {
                    out.push(0);
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
            }
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
        if self.pos + n > self.bytes.len() {
            return Err(BytecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
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

    fn bytes_owned(&mut self, n: usize) -> Result<Vec<u8>, BytecodeError> {
        Ok(self.take(n)?.to_vec())
    }
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

    let mut functions = Vec::with_capacity(fn_count as usize);
    for _ in 0..fn_count {
        let code_len = r.u32()? as usize;
        let code = r.bytes_owned(code_len)?;
        let const_count = r.u32()?;
        let mut constants = Vec::with_capacity(const_count as usize);
        for _ in 0..const_count {
            let tag = r.u8()?;
            let c = match tag {
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
                _ => return Err(BytecodeError::Truncated),
            };
            constants.push(c);
        }
        functions.push(Chunk { code, constants });
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
        chunk.emit_get_global(plus);
        chunk.emit_const(one);
        chunk.emit_const(two);
        chunk.emit_call(2);
        chunk.emit_pop();
        chunk.emit_const(greeting);
        chunk.emit_pop();
        chunk.emit_const(flag);
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
                minor: 0
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
}
