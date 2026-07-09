#![forbid(unsafe_code)]

//! Test-only support binary, not part of the product surface: builds a
//! hand-built AST bypassing the reader entirely (the reader's own
//! MAX_NESTING_DEPTH guard keeps every real source text far too shallow
//! to ever exercise this) and calls `compile_program` on it directly, on
//! whatever stack this process's main thread happens to have.
//!
//! Exists so `tests/cli_integration` can spawn this as a real, separate
//! OS process under an externally shrunk stack (`ulimit -s`) and observe
//! the outcome via its exit status -- a genuine native stack overflow
//! aborts the process outright, which no amount of catching inside a
//! single test binary (`.join()` on a spawned thread cannot catch a
//! stack overflow at all, since Rust aborts rather than unwinds for one)
//! can turn into a clean, targeted assertion failure. A separate process
//! makes that abort an ordinary, observable exit status instead of taking
//! the whole test suite down with it (warden security review msg #245).

use magiclisp::compiler::compile_program;
use magiclisp::reader::Sexpr;

fn nested_quasiquoted_list(depth: usize) -> Sexpr {
    let mut expr = Sexpr::Int(1);
    for _ in 0..depth {
        expr = Sexpr::List(vec![expr]);
    }
    Sexpr::List(vec![Sexpr::Symbol("quasiquote".to_string()), expr])
}

fn main() {
    let depth: usize = std::env::args()
        .nth(1)
        .expect("usage: quasiquote_list_stack_probe <depth>")
        .parse()
        .expect("depth must be a non-negative integer");
    let program = [nested_quasiquoted_list(depth)];
    // Either outcome (Ok or Err) is "the process survived" -- that's all
    // the driving test cares about. A stack overflow, if it happens,
    // terminates the process by signal before this line is ever reached,
    // which is the actual condition under test. Printing the outcome
    // (rather than just exiting 0 unconditionally) matters for the
    // driving test's own precision: without it, a probe that never
    // actually called `compile_program` at all would look identical --
    // clean exit, no signal -- to one that genuinely ran it.
    match compile_program(&program) {
        Ok(_) => println!("compiled ok"),
        Err(e) => println!("compiled err: {e}"),
    }
    // Not a normal return: dropping `program` (the hand-built, tens-of-
    // thousands-deep `Sexpr` tree) would recurse via Rust's default
    // recursive `Drop` glue on THIS process's own main thread -- the very
    // same severely constrained stack this probe exists to keep out of
    // compile_program's way -- crashing regardless of anything the fix
    // under test actually does. `process::exit` skips destructors
    // entirely, so only `compile_program`'s own behavior is observed.
    std::process::exit(0);
}
