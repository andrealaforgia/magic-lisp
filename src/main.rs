#![forbid(unsafe_code)]

use std::io::{self, Write};

use magiclisp::cli::{execute, parse_args};
use magiclisp::exitcode;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let command = match parse_args(&args) {
        Ok(command) => command,
        Err(usage_error) => {
            eprintln!("error: {}", usage_error.0);
            std::process::exit(exitcode::USAGE_ERROR);
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    let code = execute(command, stdin.lock(), &mut stdout, &mut stderr);
    let _ = stdout.flush();
    let _ = stderr.flush();
    std::process::exit(code);
}
