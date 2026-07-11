# MagicLisp — Language & Implementation Specification

> **Status: frozen.** This document is the normative specification the MagicLisp
> implementation is built and tested against. Every behaviour in the delivery
> roadmap (B1–B22) cites a section here, and the committed test and feature
> suites enforce these requirements. Where a section says a requirement is
> *normative*, conformance is mandatory; where it says an aspect is
> *implementation-defined*, the implementation may choose freely.
>
> **Provenance.** This file was reconstructed and frozen from the requirements
> the project was actually developed against — the observable acceptance
> criteria of behaviours B1–B22 and the committed conformance tests — after it
> was discovered that the cited `SPEC.md` had never itself been committed to the
> repository. It codifies exactly those requirements; no requirement was
> invented. Sections that were only cited generically, with no recoverable
> normative content, are marked **(reserved)**.

---

## 1. Overview

MagicLisp is a small Scheme-like Lisp with a compiler to a bytecode container
format (**MLBC**) and a stack-based virtual machine that executes it. The
implementation reads source, expands macros, compiles to bytecode, serialises
that bytecode to a portable on-disk container, and executes it. The same tool
disassembles bytecode and hosts an interactive REPL.

The surface language covers definitions and lambdas, lexical closures with
mutable captured variables, proper tail calls, a numeric tower of fixnums and
floats, strings, characters, pairs/lists, vectors, hash tables, quasiquotation,
procedural macros, and a standard library of first-class procedures.

---

## 2. Implementation constraints

### 2.1 Stack, safety, and the heap model

- The implementation targets **stable Rust**, uses **only the standard library**
  for its runtime dependencies, and **forbids `unsafe` code** — the crate carries
  `#![forbid(unsafe_code)]`. There is a single binary target, `magiclisp`.
- The expected heap shape is an **index-based arena with typed handles**: values
  live in arenas and refer to one another by index/handle rather than by raw
  pointer. An implementation may use an equivalent ownership model (see §7.3).
- **Cycle hazard (normative note).** Because there is no host garbage collector,
  the `closure → upvalue → closure` reference cycle is the known memory-leak
  hazard of this design, and staying stable under it is part of what the
  benchmark measures. Naive reference counting alone **leaks** on such cycles;
  see §10.2 for the mandatory cycle-safety requirement.

---

## 3. The language

### 3.1 Reader / lexical syntax

The reader turns source text into data. Malformed input is a **read error**
(see §9.1 exit code 65). The reader accepts:

- **Booleans** `#t` and `#f`.
- **Integers**: signed decimal, plus radix-prefixed forms `#x` (hex), `#b`
  (binary), `#o` (octal). An integer literal outside the signed 64-bit range is
  a **read error**.
- **Floats**: `digit+.digit*`, `.digit+`, and exponent forms.
- **Strings**: double-quoted, with escapes `\n`, `\t`, `\r`, `\"`, `\\`. A raw
  (literal) newline inside a string is a **read error**.
- **Characters**: `#\a`, and named forms `#\space`, `#\newline`, `#\tab`.
- **Symbols**, **proper lists** `(...)`, **dotted pairs** `(1 . 2)`, and
  **vector literals** `#(...)`.
- **Comments**: `;` to end of line; `#| ... |#` block comments **that nest**;
  and `#;` datum comments that remove **exactly the next datum**.

### 3.2 Value printing / external representation

- **Fixnums** print in decimal; **booleans** as `#t` / `#f`; **symbols** as their
  name.
- **Floats** print as the **shortest decimal string that round-trips** to the
  same value. A float with an integral value prints with a trailing `.0`
  (e.g. `1.0`, `-3.0`). Plain, non-exponent notation is used for magnitudes in
  the range **[1e-3, 1e15]**. The special values print as `+inf.0`, `-inf.0`,
  `+nan.0`. Negative zero prints as `-0.0`.
- **write vs display.** *write* produces machine-readable, re-readable output:
  strings are quoted with escapes, characters use `#\` form. *display* produces
  human-readable output: strings and characters print as raw text. Numbers,
  booleans, symbols, pairs/lists, and vectors print per this section in both
  forms.

### 3.3 Evaluation model, scope, and mutation

- Evaluation is **applicative-order**: operands are evaluated **left-to-right**,
  and the operator is evaluated first.
- **Truthiness**: **only `#f` is false**; every other value is true.
- Top-level `(define name expr)` creates or **overwrites** a global binding.
- **Internal defines**: a `lambda` / `let` / `let*` / `letrec` body may begin
  with a run of `(define ...)` forms, behaving as `letrec*`.
- `(set! name expr)` mutates an **existing** binding. `set!` on an unbound
  variable is a runtime error (exit code 70).
- **Closures capture variables, not values.** A lambda captures the surrounding
  lexical variables and may use them after the enclosing call has returned. A
  variable mutated via `set!` through one closure is visible through another
  closure that captured the same variable — they **share one storage cell**
  (see §7.3).

### 3.4 Special forms

- `(quote d)` / `'d` yield the literal datum.
- `(if c t)` and `(if c t e)`. A missing else branch with a false test yields
  the **unspecified value**, which prints as nothing (see §9.1).
- `(define name expr)` and the procedure shorthand
  `(define (name . args) body...)`, including variadic/dotted parameter lists.
- `(lambda formals body...)`, where `formals` may be a fixed list `(a b)`, a
  dotted list `(a b . rest)`, or a bare rest symbol.
- `(begin e...)`.
- `(let ...)`, `(let* ...)`, `(letrec ...)` with standard scoping, and
  **named let** `(let loop ((i 0)) ...)`.
- `(cond (test e...) ... (else e...))`, including `(test => proc)` clauses.
- `(case key ((d1 d2) e...) ... (else e...))`, selecting by **eqv?**.
- `(and e...)` / `(or e...)`, short-circuiting and returning the last / first
  significant value.
- `(when c e...)` / `(unless c e...)`.
- `(do ((var init step)...) (test result...) body...)` — standard iteration.
- **Quasiquotation.** A backquote template `` `t `` is literal except for `,e`
  (unquote) and `,@e` (unquote-splicing, which must yield a list). Nesting is
  supported to **at least depth 2**. Quasiquotation works inside both list and
  vector templates.

### 3.5 Macro scoping

A macro name that is **shadowed by a local variable binding** refers to the
variable within that scope. (The macro definition/expansion mechanism itself is
§6.)

### 3.6 Error signalling

An uncaught runtime error (including `set!` on an unbound variable, and errors
raised by `error`) prints **exactly one line to STDERR** of the form:

```
Error: <message>
```

with any irritants appended when present. The `Error: ` prefix, the use of the
**stderr** stream, and the **exit code 70** are all normative. The REPL reuses
this same error line (see §9.1).

### 3.7 Equality

- **eq?** — identity. Fixnums, booleans, the empty list, characters, and
  interned symbols compare by value; pairs, strings, vectors, hashes, and
  procedures compare by reference.
- **eqv?** — as `eq?`, plus floats compared by numeric value **and bit-class**:
  `(eqv? +nan.0 +nan.0)` is `#t`, `(eqv? 0.0 -0.0)` is `#f`, `(eqv? 1 1.0)` is
  `#f`.
- **equal?** — recurses structurally over pairs, vectors, and strings, otherwise
  falls back to `eqv?`. It **must terminate on acyclic data**.

### 3.8 Proper tail calls

- **Proper tail calls are required.** A call in tail position reuses the current
  activation and runs in **constant stack space**.
- Tail positions are exactly: the body-final expression of
  `lambda`/`let`/`let*`/`letrec`/`begin`/`when`/`unless`; **both branches** of
  `if`; the result expressions of `cond`/`case` clauses; the final expression of
  `and`/`or`; and the application in a `=>` clause.
- Both self-tail-recursive and mutually-recursive loops driven to a depth of
  **10,000,000** complete without exhausting memory.

---

## 4. Standard library

All standard-library procedures are **first-class global bindings**.

### 4.1 Numbers

- Variadic `+ - * /` with identities: `(+)` is `0`, `(*)` is `1`; `(-)` and
  `(/)` with no arguments are errors; `(- x)` negates; `(/ x)` is `(/ 1 x)`.
- Variadic comparisons `= < <= > >=`.
- **Integer overflow wraps** (two's complement, signed 64-bit).
- **Division rule.** If any argument is a float, the result is a float. If all
  arguments are fixnums and the division is exact at every step, the result is a
  fixnum; otherwise it is a float. Exact `(/ n 0)` is a runtime error (exit 70);
  float division by zero follows IEEE 754 and is **not** an error.
- Extended: `quotient` / `remainder` (truncated division), `modulo` (floored) —
  a zero second argument is an error; `abs`, `min`, `max`; predicates `zero?`,
  `positive?`, `negative?`, `even?`, `odd?`; `floor`, `ceiling`, `round`,
  `truncate` (float→float, round-half-to-even, identity on fixnums); `sqrt`,
  `expt`, `exp`, `log`, `sin`, `cos`, `tan`, `atan` (`(sqrt 4)` → `2.0`; an
  integer base with a non-negative integer exponent is exact, `(expt 2 10)` →
  `1024`); `number?`, `integer?`, `float?`, `exact->inexact`, `inexact->exact`
  (truncates toward zero, errors on non-finite); `number->string`,
  `string->number` (returns `#f` on unparseable input).

### 4.2 Type predicates

- `not` (`#t` only for `#f`).
- `null?`, `pair?`, `list?`, `symbol?`, `string?`, `char?`, `boolean?`,
  `procedure?`, `vector?`, `hash?`. `list?` is `#t` **only** for a proper, finite
  list.

### 4.3 Pairs and lists

- `cons`, `car`, `cdr`, `set-car!`, `set-cdr!`, and composed accessors `caar`,
  `cadr`, `cdar`, `cddr`, `caddr`, `cdddr`, `cadar`, `cddar`.
- `list`, `length`, `append`, `reverse`, `list-ref`, `list-tail`, `last-pair`.
- Searches: `memq` / `memv` / `member` (using `eq?` / `eqv?` / `equal?`) and
  `assq` / `assv` / `assoc`.
- Higher-order: `map` / `for-each` (over 1..n equal-length lists), `filter`
  (one list).
- **Folds**: `(fold-left f init lst)` = `(f (f init x1) x2)...`;
  `(fold-right f init lst)` = `(f x1 (f x2 ... init))`;
  `(reduce f init lst)` = `init` for the empty list, otherwise `fold-left` over
  the tail using `car` as the seed.
- `apply` (final argument must be a proper list).
- `list->vector`, `list->string`.

### 4.4 Strings and characters

- **Strings are immutable and indexed by Unicode scalar value** (not bytes).
- `string-length`, `string-ref`, `substring` (half-open `[start, end)`,
  bounds-checked), `string-append`.
- `string=?`, `string<?`, `string>?` — lexicographic by scalar value.
- `string->symbol`, `symbol->string`, `string->list`, `list->string`.
- `string-upcase`, `string-downcase` (ASCII at minimum).
- `char->integer`, `integer->char`, `char=?`, `char<?`, `char-alphabetic?`,
  `char-numeric?`, `char-whitespace?`.

### 4.5 Vectors

- `vector`, `make-vector` (`(make-vector n)` or `(make-vector n fill)`, default
  fill `0`), `vector-ref`, `vector-set!`, `vector-length`, `vector->list`,
  `vector-fill!`, `list->vector`.
- An out-of-range index is a runtime error. `#(1 2 3)` literals read (see §3.1).

### 4.6 Hash tables

- `make-hash`, `hash-ref`, `hash-set!`, `hash-remove!`, `hash-count`,
  `hash-keys`, `hash-has-key?`.
- Keys are compared with **equal?**. `(hash-ref h k)` errors when the key is
  missing; `(hash-ref h k default)` returns `default` instead.
- **`hash-keys` returns keys in insertion order** — this is normative, so output
  is deterministic.

### 4.7 Errors and exit

- `(error msg irritant...)` — `msg` in display form, irritants in write form,
  space-separated (see §3.6).
- `(exit)` → exit code 0; `(exit n)` → exit code `n`.

### 4.8 Input

- `(read)` reads one datum from stdin and returns it **unevaluated**; at
  end-of-file it returns the **end-of-file object** (`eof-object?` → `#t`).
- `(read-line)` reads one line (without the terminator) as a string; at
  end-of-file it returns the end-of-file object.

---

## 5. Compilation pipeline

The implementation is organised as a pipeline: **reader → expander → compiler →
vm**, producing and consuming the **MLBC** bytecode container, with a
**disassembler**, a **REPL**, and a **CLI** on top. Module boundaries reflect
these stages (see §10.4).

### 5.6 Determinism / reproducibility

Compiling the same source twice is **byte-identical**. A source file with no
macros is **bit-reproducible**.

---

## 6. Macros

- `(define-macro (name . args) body...)` defines an **unhygienic procedural
  macro**: at expansion time the body is evaluated with the **unevaluated**
  argument forms bound to the parameters; the returned datum replaces the call
  site and is itself expanded recursively.
- A macro is usable after its definition, including inside the bodies of
  procedures defined later. Macro bodies may use the full language and standard
  library.
- `gensym` returns a fresh, uninterned-name symbol on each call, distinct from
  source symbols and from other `gensym` results.
- **Expansion happens before compilation** and is iterated to a **fixed point**,
  with a recursion limit of **at least 1000 expansions per top-level form**;
  exceeding it is a compile error.

---

## 7. Virtual machine

The VM is stack-based: a value stack plus activation frames. It executes
bytecode including `CONST`, `GET_GLOBAL` / `SET_GLOBAL` / `DEF_GLOBAL`, `CALL`
(handling native and bytecode callees uniformly), `RETURN`, `HALT`, and `POP`.
An undefined opcode or an out-of-range index is a runtime error, never a host
crash.

### 7.1 Frames and stack limits

- Deep **non-tail** recursion to **100,000** nested activations completes
  successfully.
- Exceeding the implementation's frame limit is reported as a graceful runtime
  **"stack overflow"** error (exit code 70) — never a host crash or segfault
  (see §10.3).

### 7.2 (reserved)

*Not cited with normative content.*

### 7.3 Closure / upvalue reference model

Closures capture variables that **share one storage cell** across all closures
capturing them (see §3.3). This section describes the reference model but
**permits equivalents** — any ownership model with the same observable sharing
and mutation semantics conforms.

### 7.4 Opcodes / mnemonics

Codegen strategy is **free**. The attested mnemonics include: `CONST`,
`GET_GLOBAL`, `SET_GLOBAL`, `DEF_GLOBAL`, `CALL`, `TAIL_CALL`, `RETURN`, `HALT`,
`POP`, `CLOSURE`, `GET_LOCAL`, `GET_UPVALUE`, `JUMP_IF_FALSE`. Structurally
invalid bytecode (an undefined opcode, an out-of-range index, or a `RETURN` in
the entry point where `HALT` is required) must be **detected** and must never
crash the process (see §8.2, §10.3).

### 7.5 Disassembler output format

For each function the disassembler prints: its index; its name, or `<anonymous>`
/ `<toplevel>` for the entry; its arity, variadic flag, and upvalue count; its
constant pool (index, type, and value in **write** form); and then one
instruction per line as `OFFSET MNEMONIC operands`, using exactly the §7.4
mnemonics. **Jump targets are shown as absolute offsets**, not raw relative
displacements. Exact column widths and spacing are implementation-defined; the
field set, the mnemonics, the offsets, and the absolute jump targets are
normative.

---

## 8. MLBC bytecode container format

The MLBC container consists of a **header** (magic `MLBC`, version 1.0, flags 0,
entry index, function count), a **function table**, and a **constant encoding**
supporting at least `INT`, `STRING`, and `SYMBOL`. The format is
**little-endian** and **round-trips byte-for-byte**: a program written and then
re-read is identical, and compiling the same source twice yields a byte-identical
container (see §5.6).

### 8.1 (reserved)

*Not cited with normative content.*

### 8.2 Loader rejection

A malformed or foreign file — bad magic, wrong version, non-zero flags,
truncated, or a bad entry index — is **rejected with exit code 66**.

---

## 9. Tooling and conformance

### 9.1 CLI, exit codes, REPL

- **Five verbs**: `compile`, `run`, `eval`, `disasm`, `repl`. **`repl` is the
  no-argument default.**
- **Exit codes (normative):** `0` success, `64` usage, `65` read/compile error,
  `66` file/format error, `70` runtime error.
- The **unspecified value** prints as nothing.
- **REPL.** The prompt is `> ` (a greater-than sign and a single space, no
  trailing newline) before each read. Each datum is expanded, compiled, and
  executed; the result is printed in **write** form followed by a newline, unless
  it is the unspecified value, in which case nothing is printed. Definitions
  persist across entries. A runtime error prints the §3.6 `Error:` line to STDERR
  and returns to the prompt (it does **not** exit). EOF (Ctrl-D) exits with code
  0.

### 9.2 (reserved)

*Not cited with normative content.*

### 9.3 Self-test suite

The implementation ships its own test suite, runnable via a **single documented
command**, covering at minimum: the reader (including comments and dotted pairs);
constant serialisation round-tripped through a **real bytecode file on disk**;
closure upvalue sharing; tail-call depth; and **at least one error path per exit
code** (0 / 64 / 65 / 66 / 70).

### 9.4 (reserved)

*Not cited with normative content.*

### 9.5 Conformance sample programs

All ten published sample cases must produce exactly their specified stdout, exit
code, and (for 080) stderr prefix.

| Sample | Program essence | Official output |
|---|---|---|
| 010-factorial | `fact` with top-level redefinition, `(fact 10)` | stdout `3628800\n`, exit 0 |
| 020-tco | self-tail loop to 10,000,000 | `10000000\n` |
| 030-closures | two independent counters | `1\n2\n1\n` |
| 040-shared-upvalue | shared captured cell via a getter/setter pair | `10\n` |
| 050-macro | `swap!` via `gensym` | `(2 1)\n` |
| 060-quasi | `` `(1 ,@mid 5) `` | `(1 2 3 4 5)\n` |
| 070-division | `(/ 6 3)`, `(/ 7 2)`, `(/ 6 3.0)`, `1.0` | `2\n3.5\n2.0\n1.0\n` |
| 080-error | `(error "boom" 42)` | stdout empty; exit 70; stderr first line `Error: boom 42` |
| 090-read | `(read)` then write + `(+ 1 2)`, stdin `(+ 1 2)\n` | `(+ 1 2)\n3\n` |
| 100-hash | make-hash; set `a`,`b`; count; keys; ref-default; has-key; remove | `2\n(a b)\nnope\n#t\n#f\n` |

---

## 10. Non-functional requirements

### 10.1 Performance floors

Measured on an **optimised release build**. These floors are generous — meant to
catch only pathological implementations:

- the 020-tco loop (count to **10,000,000**) completes in **≤ 10 seconds**;
- naive doubly-recursive **`(fib 27)`** (expected `196418`) completes in
  **≤ 20 seconds**;
- compiling a **2,000-line** source file completes in **≤ 5 seconds**.

### 10.2 Memory and cycle-safety (mandatory)

- Tail-call tests run in **O(1) frame space** (constant, not growing with
  iteration count).
- A 60-second soak that repeatedly exercises the counter/closure test **must not
  grow memory without bound**.
- **Cycle-safety mandate.** Because there is no host garbage collector and naive
  reference-counted graphs **leak** on `closure → upvalue → closure` cycles
  (see §2.1), the implementation **must** use a cycle-safe strategy: either a
  tracing collector **or** a documented cycle-safe ownership strategy (for
  example, weak back-edges). Memory must stay **bounded** under a sustained
  (~60-second) soak that repeatedly **creates and discards genuine closure /
  upvalue cycles**, exercised in **two distinct cyclic shapes**:
  1. **self-reference** — a captured cell that is `set!` to hold the very closure
     that captured it; and
  2. **mutual reference** — two closures whose captured cells each hold the other
     closure.
  Passing the acyclic soak alone does not demonstrate conformance; the cyclic
  soak is required. The chosen mechanism must be describable in plain language
  for the README.

### 10.3 Crash-free robustness

No input, however malformed, may ever crash the process — no segfault, no
uncaught host-level panic, no backtrace. Every outcome must map to exactly one
of the defined exit codes: malformed source → **65**; malformed or foreign
bytecode → **66**; CLI misuse such as an unknown verb or a missing argument →
**64**; a runtime fault → **70**; success → **0**.

### 10.4 Module boundaries

Internal module boundaries reflect the compilation-pipeline stages of §5.

---

## 11. (reserved)

*No section 11 is cited; reserved.*

## 12. Benchmark posture

The implementation is driven autonomously, surfacing a reviewable result at each
shippable increment. *(Cited only as a posture; no further normative content.)*

---

## Appendix A. Worked example

A compilation / disassembly worked example built on the closure-returning
procedure:

```scheme
(define (add-n n) (lambda (x) (+ x n)))
(display ((add-n 4) 3))
(newline)
```

which evaluates to `7`. Used to demonstrate the disassembler (§7.5).
