//! Command-line argument parsing and verb dispatch.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::{bytecode, compiler, disasm, exitcode, reader, repl, vm};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Compile { input: PathBuf, output: PathBuf },
    Run { artifact: PathBuf },
    Eval { input: PathBuf },
    Disasm { artifact: PathBuf },
    Repl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageError(pub String);

const KNOWN_VERBS: [&str; 5] = ["compile", "run", "eval", "disasm", "repl"];

pub fn parse_args(args: &[String]) -> Result<Command, UsageError> {
    let Some((verb, rest)) = args.split_first() else {
        return Ok(Command::Repl);
    };

    match verb.as_str() {
        "repl" if rest.is_empty() => Ok(Command::Repl),
        "repl" => Err(UsageError("usage: magiclisp repl".to_string())),
        "eval" => match rest {
            [file] => Ok(Command::Eval { input: file.into() }),
            _ => Err(UsageError("usage: magiclisp eval <file>".to_string())),
        },
        "run" => match rest {
            [artifact] => Ok(Command::Run {
                artifact: artifact.into(),
            }),
            _ => Err(UsageError("usage: magiclisp run <artifact>".to_string())),
        },
        "disasm" => match rest {
            [artifact] => Ok(Command::Disasm {
                artifact: artifact.into(),
            }),
            _ => Err(UsageError("usage: magiclisp disasm <artifact>".to_string())),
        },
        "compile" => match rest {
            [file, flag, out] if flag == "-o" => Ok(Command::Compile {
                input: file.into(),
                output: out.into(),
            }),
            _ => Err(UsageError(
                "usage: magiclisp compile <file> -o <output>".to_string(),
            )),
        },
        other => Err(UsageError(format!(
            "unknown verb '{other}' (expected one of: {})",
            KNOWN_VERBS.join(", ")
        ))),
    }
}

pub fn execute(
    command: Command,
    input: impl BufRead,
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    match command {
        Command::Eval { input: path } => run_eval(&path, out, err),
        Command::Compile {
            input: path,
            output,
        } => run_compile(&path, &output, err),
        Command::Run { artifact } => run_run(&artifact, out, err),
        Command::Disasm { artifact } => run_disasm(&artifact, out, err),
        Command::Repl => repl::run(input, out, err),
    }
}

fn compile_source(src: &str) -> Result<bytecode::Chunk, String> {
    let forms = reader::read_program(src).map_err(|e| e.to_string())?;
    compiler::compile_program(&forms).map_err(|e| e.to_string())
}

fn run_eval(path: &Path, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "error: cannot read {}: {e}", path.display());
            return exitcode::SOURCE_ERROR;
        }
    };
    let chunk = match compile_source(&src) {
        Ok(c) => c,
        Err(message) => {
            let _ = writeln!(err, "error: {message}");
            return exitcode::SOURCE_ERROR;
        }
    };
    match vm::run(&chunk, out) {
        Ok(()) => exitcode::SUCCESS,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            exitcode::RUNTIME_ERROR
        }
    }
}

fn run_compile(input: &Path, output: &Path, err: &mut impl Write) -> i32 {
    let src = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "error: cannot read {}: {e}", input.display());
            return exitcode::SOURCE_ERROR;
        }
    };
    let chunk = match compile_source(&src) {
        Ok(c) => c,
        Err(message) => {
            let _ = writeln!(err, "error: {message}");
            return exitcode::SOURCE_ERROR;
        }
    };
    let module = bytecode::Module {
        entry_index: 0,
        functions: vec![chunk],
    };
    match std::fs::write(output, bytecode::encode(&module)) {
        Ok(()) => exitcode::SUCCESS,
        Err(e) => {
            let _ = writeln!(err, "error: cannot write {}: {e}", output.display());
            exitcode::SOURCE_ERROR
        }
    }
}

fn load_artifact(artifact: &Path, err: &mut impl Write) -> Result<bytecode::Module, i32> {
    let bytes = std::fs::read(artifact).map_err(|e| {
        let _ = writeln!(err, "error: cannot read {}: {e}", artifact.display());
        exitcode::BAD_ARTIFACT
    })?;
    bytecode::decode(&bytes).map_err(|e| {
        let _ = writeln!(err, "error: {e}");
        exitcode::BAD_ARTIFACT
    })
}

fn run_run(artifact: &Path, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let module = match load_artifact(artifact, err) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let chunk = &module.functions[module.entry_index as usize];
    match vm::run(chunk, out) {
        Ok(()) => exitcode::SUCCESS,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            exitcode::RUNTIME_ERROR
        }
    }
}

fn run_disasm(artifact: &Path, out: &mut impl Write, err: &mut impl Write) -> i32 {
    let module = match load_artifact(artifact, err) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let chunk = &module.functions[module.entry_index as usize];
    let _ = write!(out, "{}", disasm::disassemble(chunk));
    exitcode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_means_repl() {
        assert_eq!(parse_args(&args(&[])).unwrap(), Command::Repl);
    }

    #[test]
    fn repl_verb_explicitly_means_repl() {
        assert_eq!(parse_args(&args(&["repl"])).unwrap(), Command::Repl);
    }

    #[test]
    fn eval_requires_exactly_one_file_argument() {
        assert_eq!(
            parse_args(&args(&["eval", "prog.ml"])).unwrap(),
            Command::Eval {
                input: "prog.ml".into()
            }
        );
        assert!(parse_args(&args(&["eval"])).is_err());
        assert!(parse_args(&args(&["eval", "a.ml", "b.ml"])).is_err());
    }

    #[test]
    fn run_requires_exactly_one_artifact_argument() {
        assert_eq!(
            parse_args(&args(&["run", "out.mlbc"])).unwrap(),
            Command::Run {
                artifact: "out.mlbc".into()
            }
        );
        assert!(parse_args(&args(&["run"])).is_err());
    }

    #[test]
    fn disasm_requires_exactly_one_artifact_argument() {
        assert_eq!(
            parse_args(&args(&["disasm", "out.mlbc"])).unwrap(),
            Command::Disasm {
                artifact: "out.mlbc".into()
            }
        );
        assert!(parse_args(&args(&["disasm"])).is_err());
    }

    #[test]
    fn compile_requires_a_file_and_dash_o_and_an_output_path() {
        assert_eq!(
            parse_args(&args(&["compile", "prog.ml", "-o", "out.mlbc"])).unwrap(),
            Command::Compile {
                input: "prog.ml".into(),
                output: "out.mlbc".into(),
            }
        );
        assert!(parse_args(&args(&["compile", "prog.ml"])).is_err());
        assert!(parse_args(&args(&["compile", "prog.ml", "out.mlbc"])).is_err());
        assert!(parse_args(&args(&["compile", "prog.ml", "--wrong-flag", "out.mlbc"])).is_err());
    }

    #[test]
    fn an_unknown_verb_is_a_usage_error() {
        assert!(parse_args(&args(&["frobnicate", "x"])).is_err());
    }

    #[test]
    fn repl_takes_no_extra_arguments() {
        assert!(parse_args(&args(&["repl", "extra"])).is_err());
    }
}

#[cfg(test)]
mod execute_tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "magiclisp-cli-test-{}-{n}-{label}",
            std::process::id()
        ))
    }

    fn write_source(label: &str, content: &str) -> PathBuf {
        let path = temp_path(label);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn run_cmd(command: Command) -> (String, String, i32) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = execute(command, Cursor::new(&b""[..]), &mut out, &mut err);
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
            code,
        )
    }

    #[test]
    fn eval_a_valid_program_prints_its_output_and_exits_success() {
        let input = write_source("eval-ok.ml", "(display (+ 1 2)) (newline)");
        let (out, err, code) = run_cmd(Command::Eval { input });
        assert_eq!(out, "3\n");
        assert!(err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn eval_a_source_with_a_read_error_exits_with_source_error() {
        let input = write_source("eval-read-err.ml", "\"unterminated");
        let (_, err, code) = run_cmd(Command::Eval { input });
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::SOURCE_ERROR);
    }

    #[test]
    fn eval_a_program_that_fails_at_runtime_exits_with_runtime_error() {
        let input = write_source("eval-runtime-err.ml", "(this-is-undefined 1 2)");
        let (_, err, code) = run_cmd(Command::Eval { input });
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::RUNTIME_ERROR);
    }

    #[test]
    fn eval_a_missing_file_exits_with_source_error() {
        let (_, err, code) = run_cmd(Command::Eval {
            input: temp_path("does-not-exist.ml"),
        });
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::SOURCE_ERROR);
    }

    #[test]
    fn compile_then_run_reproduces_the_same_output_as_eval() {
        let input = write_source("pipeline.ml", "(display (+ 40 2)) (newline)");
        let artifact = temp_path("pipeline.mlbc");

        let (_, compile_err, compile_code) = run_cmd(Command::Compile {
            input,
            output: artifact.clone(),
        });
        assert!(compile_err.is_empty());
        assert_eq!(compile_code, exitcode::SUCCESS);

        let (run_out, run_err, run_code) = run_cmd(Command::Run { artifact });
        assert_eq!(run_out, "42\n");
        assert!(run_err.is_empty());
        assert_eq!(run_code, exitcode::SUCCESS);
    }

    #[test]
    fn disasm_of_a_compiled_artifact_prints_a_legible_listing() {
        let input = write_source("disasm.ml", "(display (+ 1 2)) (newline)");
        let artifact = temp_path("disasm.mlbc");
        run_cmd(Command::Compile {
            input,
            output: artifact.clone(),
        });

        let (out, err, code) = run_cmd(Command::Disasm { artifact });
        assert!(out.contains("CALL"));
        assert!(err.is_empty());
        assert_eq!(code, exitcode::SUCCESS);
    }

    #[test]
    fn run_on_a_missing_artifact_exits_with_bad_artifact() {
        let (_, err, code) = run_cmd(Command::Run {
            artifact: temp_path("missing.mlbc"),
        });
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::BAD_ARTIFACT);
    }

    #[test]
    fn run_on_a_corrupt_artifact_exits_with_bad_artifact() {
        let artifact = temp_path("corrupt.mlbc");
        std::fs::write(&artifact, b"not an mlbc file at all").unwrap();
        let (_, err, code) = run_cmd(Command::Run { artifact });
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::BAD_ARTIFACT);
    }

    #[test]
    fn disasm_on_a_corrupt_artifact_exits_with_bad_artifact() {
        let artifact = temp_path("corrupt-disasm.mlbc");
        std::fs::write(&artifact, b"not an mlbc file at all").unwrap();
        let (_, err, code) = run_cmd(Command::Disasm { artifact });
        assert!(!err.is_empty());
        assert_eq!(code, exitcode::BAD_ARTIFACT);
    }

    #[test]
    fn repl_command_evaluates_lines_from_the_given_input() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = execute(
            Command::Repl,
            Cursor::new(b"(display (+ 1 2))\n".as_slice()),
            &mut out,
            &mut err,
        );
        assert_eq!(String::from_utf8(out).unwrap(), "3");
        assert_eq!(code, exitcode::SUCCESS);
    }
}
