# MagicLisp — project overview

MagicLisp is a small Scheme-like Lisp, implemented from scratch in Rust: its own reader,
macro expander, compiler to a portable bytecode container format (**MLBC**), and a
stack-based virtual machine that executes it. The same `magiclisp` binary also
disassembles compiled bytecode and hosts an interactive REPL. This document is the
project's closing account — what was actually delivered, and how to run it — written for
a newcomer who hasn't read anything else yet. `SPEC.md` is the normative specification
everything below was built and tested against; `README.md` covers build/run commands and
the memory model in more technical depth.

## What was delivered

### The language

The surface language (SPEC.md §3) covers definitions and lambdas; lexical closures with
mutable captured variables (a variable `set!` through one closure is visible through
every other closure that captured the same variable — they share one storage cell, not a
copy); proper tail calls, required rather than best-effort (both self-tail-recursive and
mutually-recursive loops run to a depth of 10,000,000 in constant stack space); a numeric
tower of 64-bit fixnums (wrapping on overflow) and floats; immutable, Unicode-scalar-
indexed strings; characters; pairs and lists; vectors; hash tables; quasiquotation nested
to at least depth 2; and an unhygienic procedural macro system (`define-macro`, `gensym`)
that expands to a fixed point before compilation. The special forms include `if`,
`define`, `lambda`, `begin`, `let`/`let*`/`letrec`/named `let`, `cond`/`case`, `and`/`or`,
`when`/`unless`, and `do`. The standard library is a substantial first-class-procedure
set spanning numeric operations, type predicates, pair/list operations (including
higher-order `map`/`for-each`/`filter` and folds), string/character operations, vectors,
hash tables, and `error`/`exit`/`read`/`read-line`.

### The pipeline

The implementation is organised exactly as its own architecture: **reader → expander →
compiler → VM** (SPEC.md §5), with the module boundaries reflecting those stages. Source
text is read into data, macros are expanded to a fixed point, the result is compiled to
bytecode, and the VM executes it directly — or the bytecode is first serialised to the
MLBC container and executed later, identically. Compiling the same source twice produces
a byte-identical MLBC file (SPEC.md §5.6), and a container written and then read back is
identical to what was written (SPEC.md §8) — genuine round-trip guarantees, not just "it
usually works," each backed by its own dedicated test.

### The MLBC container and tooling

MLBC (SPEC.md §8) is a little-endian container: a header (magic, version, flags, entry
index, function count), a function table, and a constant encoding. A malformed or foreign
file — bad magic, wrong version, non-zero flags, truncated, or a bad entry index — is
rejected cleanly with exit code 66, never a crash. On top of the compiler and VM, the same
binary provides:

- A **disassembler** that prints each function's index, name, arity, variadic flag,
  upvalue count, constant pool (in write form), and one instruction per line with
  absolute (not relative) jump targets.
- An **interactive REPL** (the CLI's no-argument default) that reads, expands, compiles,
  and executes one datum at a time, printing each result in write form, persisting
  definitions across entries, and surviving a runtime error by printing the error line and
  returning to the prompt rather than exiting.
- A **five-verb CLI** — `compile`, `run`, `eval`, `disasm`, `repl` — with a normative exit
  code contract: `0` success, `64` usage error, `65` read/compile error, `66` malformed
  bytecode, `70` runtime error. Every one of these outcomes, for every verb, is exercised
  by a real subprocess test asserting the actual exit code, not an inference from output.

### The conformance suite

The implementation ships its own test suite, runnable via a single documented command
(`cargo test`), spanning unit tests across every module, process-level tests that spawn
the real compiled binary and assert on its actual stdout/stderr/exit code, and Gherkin
`.feature` files whose scenarios are executed by a real step-definition runner against
that same real binary — no mocks anywhere in the acceptance layer. All ten of SPEC.md's
published conformance sample programs (§9.5 — factorial with redefinition, TCO to ten
million, independent closures, a shared-upvalue getter/setter pair, a `gensym`-based
macro, quasiquotation, the four division-result rules, an error path, `(read)`, and hash
tables) produce exactly their specified stdout, exit code, and stderr prefix.

### Non-functional guarantees

- **Performance floors** (SPEC.md §10.1, measured on an optimised release build): the
  10,000,000-iteration tail-call loop completes in at most 10 seconds; naive doubly-
  recursive `(fib 27)` completes in at most 20 seconds; compiling a 2,000-line source file
  completes in at most 5 seconds.
- **Memory and cycle-safety** (SPEC.md §10.2, mandatory): there is no host garbage
  collector, and plain reference counting alone leaks on a `closure → upvalue → closure`
  reference cycle. MagicLisp closes that gap with a small trial-deletion cycle collector,
  scoped to exactly the values that can participate in such a cycle, running periodically
  as captured environments accumulate. `README.md`'s "Memory and cycle-safety" section
  explains the mechanism in full; both mandatory cyclic shapes (a closure whose captured
  cell is `set!` back to itself, and two closures whose captured cells each hold the
  other) stay memory-bounded under a sustained soak, and so does the ordinary acyclic
  case.
- **Crash-free robustness** (SPEC.md §10.3): no input, however malformed, may ever crash
  the process — no segfault, no uncaught panic, no backtrace. Every outcome maps to
  exactly one exit code. This is enforced structurally: the crate carries
  `#![forbid(unsafe_code)]`, and every recursive structure reachable from untrusted input
  (the reader, the bytecode decoder, macro expansion, VM call recursion) has an explicit,
  tested depth or step budget.

## How to run it

From the repository root, with a release build available:

```sh
cargo build --release
magiclisp eval program.ml            # compile and run in one step
magiclisp compile program.ml -o program.mlbc
magiclisp run program.mlbc
magiclisp disasm program.mlbc
magiclisp repl
```

### The Huffman worked example

Beyond the language itself, `examples/huffman/` is a genuine Huffman compressor and
decompressor written entirely in MagicLisp (`compress.ml` / `decompress.ml`) and run
through this same CLI — no separate Rust implementation of the algorithm. It exists to
prove the language can express a real, non-trivial algorithm end to end, not just short
snippets. See `examples/huffman/README.md` for its exact run commands (compression bridges
through hex text at the shell level via `xxd`, since MagicLisp itself has no file I/O or
raw-byte string handling; the bit-packing is 100% MagicLisp arithmetic).

## Where this stands

Every behaviour above — the full language surface, the compilation pipeline and MLBC
round-trip guarantees, the disassembler and REPL, the CLI's exit-code contract, the
conformance suite, and the performance and memory/cycle-safety guarantees — is backed by
a real, committed, currently-passing test or feature scenario, or a guarantee `SPEC.md`
itself states normatively; nothing here claims more than that. Taken together with the
Huffman worked example, this is MagicLisp as it stands today: a small language with a
complete, tested implementation pipeline from source text to running bytecode, usable for
programs a newcomer would recognise as real work, not just toy snippets.
