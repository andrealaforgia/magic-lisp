# MagicLisp

MagicLisp is a small Lisp — in the same family as Scheme — that you can write real
programs in. It comes as one command-line tool, `magiclisp`, that can run your code
directly, compile it to a portable bytecode file, run that file later, take it apart
with a disassembler, or drop you into an interactive REPL to play around.

It's a hobby project built from scratch in Rust, with no dependency on any other Lisp
or Scheme implementation.

## Quick start

```sh
cargo build --release
```

Then save this to `hello.ml`:

```scheme
(define (fact n)
  (if (= n 0) 1 (* n (fact (- n 1)))))

(display (fact 10))
(newline)
```

And run it:

```sh
target/release/magiclisp eval hello.ml
# 3628800
```

That's it — no separate build step needed. `eval` reads, compiles, and runs a
program in one go.

## What you can do with it

- **Closures that share state.** A function can return another function that
  remembers variables from its birthplace — and if two functions capture the *same*
  variable, changing it through one is visible through the other.
- **Loops that never blow the stack.** Tail calls (including mutual recursion between
  two functions calling each other) run in constant memory, however many times they
  loop — millions of iterations are fine.
- **The usual data you'd expect**: whole numbers and decimals, strings, characters,
  pairs and lists, vectors, and hash tables.
- **Macros.** Write your own syntax with `define-macro` and `gensym`, expanded before
  your program ever runs.
- **A full toolbox in one binary**:

  | Command | What it does |
  |---|---|
  | `magiclisp eval file.ml` | Compile and run a program in one step |
  | `magiclisp compile file.ml -o file.mlbc` | Compile to a portable bytecode file |
  | `magiclisp run file.mlbc` | Run a compiled bytecode file |
  | `magiclisp disasm file.mlbc` | Print human-readable bytecode |
  | `magiclisp repl` | Interactive prompt |

- **It doesn't crash.** Feed it garbage — a broken program, a corrupted bytecode
  file — and it reports a clean error with a distinct exit code instead of panicking
  or segfaulting.

## How it manages memory

MagicLisp doesn't have a general-purpose garbage collector. Most values are cleaned
up the simple way — reference counting — as soon as nothing points to them anymore.

There's one shape reference counting can't handle on its own: two things that end up
pointing at each other in a loop (for example, a function that captures a variable
which is then set to hold that very function). Nothing outside the loop points to it,
but the pieces inside still point to each other, so plain reference counting never
reaches zero and the memory would otherwise leak forever.

To catch that, MagicLisp runs a small, focused cleanup pass every so often, only over
the kind of values that could form such a loop. It checks whether each one is still
reachable from somewhere outside the loop; if not, it's a genuine piece of garbage and
gets cleared away. Anything still legitimately in use is left completely alone. This
pass runs occasionally rather than constantly, so it stays cheap even in long-running
programs.

## See it run something real

`examples/huffman/` is a full Huffman compressor and decompressor written entirely in
MagicLisp — not a toy snippet. See `examples/huffman/README.md` to try it on a real
file.

## Want the details?

- **`OVERVIEW.md`** — a guided tour of everything the project delivers, written for
  someone who hasn't read anything else yet.
- **`SPEC.md`** — the full, normative language and implementation specification
  everything here was built and tested against.
