# MagicLisp

MagicLisp is a small Scheme-like Lisp: a reader, a macro expander, a compiler to a
portable bytecode container format (**MLBC**), and a stack-based virtual machine that
executes it. The same `magiclisp` binary also disassembles bytecode and hosts an
interactive REPL. See `SPEC.md` for the full language and implementation specification.

## Building and running

```sh
cargo build --release
magiclisp eval program.ml            # compile and run in one step
magiclisp compile program.ml -o program.mlbc
magiclisp run program.mlbc
magiclisp disasm program.mlbc
magiclisp repl
```

## Memory and cycle-safety

The heap is index/handle-based in spirit but implemented with `Rc`/`RefCell`: values
share ownership through reference counting rather than a host garbage collector, and
`#![forbid(unsafe_code)]` holds throughout. Reference counting alone reclaims everything
*except* one shape: a closure's captured environment holding a local variable cell that
itself, directly or through another closure, holds a reference back to that same closure
— whether that reference sits directly in the cell or is mediated through a pair,
vector, hash table, or list the cell holds instead. Plain `Rc` counts never reach zero
for a cycle like that, no matter how unreachable it becomes from the rest of the
program.

MagicLisp closes this gap with a small, targeted **trial-deletion cycle collector**,
scoped to the kinds of object that can participate in this cycle: a closure's captured
environment, and every mutable value — a local cell itself, or any pair/vector/hash
table nested inside one, however deep, transparently through any list along the way (a
list is immutable, so it can never itself close a cycle, but its elements still need to
stay visible). Every newly captured environment is registered with the collector; once
enough have accumulated, a sweep runs:

1. For every tracked object, count how many references to it come from *other tracked
   objects* (a captured-locals slot, a parent-environment link, a cell/pair/vector/hash
   holding a closure over — or another pair/vector/hash nested inside — a tracked
   object).
2. Subtract that count from the object's real, total reference count. Whatever is left
   over must be coming from *outside* the tracked set — an active call frame's own
   locals, the operand stack, a global binding, anything at all. Any object with
   something left over is definitely still reachable, and so is anything reachable from
   it.
3. Anything never reached this way is, by construction, not reachable from any real
   owner anywhere in the running program — a genuine garbage cycle. The closure
   references responsible are cleared, breaking the cycle so ordinary `Rc` drop glue
   reclaims the rest exactly as it already does for everything acyclic.

Because step 2 works directly off real reference counts, this needs no separate
tracking of the interpreter's call stack or operand stack as a root set: an object
still legitimately in use — even one still being read through the very cycle that
would otherwise leak it, such as a self-referential closure being called before its
defining `let` has returned — always has a real reference counted against it, so it is
never mistaken for garbage.

How often a sweep runs is itself amortized rather than fixed: after each one, the next
is scheduled at twice however many tracked objects actually survived it (never less
than a small minimum). A workload that's mostly short-lived cycles keeps the survivor
count — and so the sweep interval — small and frequent; a workload that legitimately
accumulates many long-lived closures (nothing cyclic to reclaim) sees the interval
grow with it, so total sweep work across the whole run stays proportional to the work
done, not to its square.

This keeps a self-referential closure (a cell `set!` to hold the very closure that
captured it), a mutually-referencing pair of closures (two cells that each hold the
other's closure), and the same shapes mediated through an intervening pair, vector, or
hash table, all memory-bounded under sustained, repeated creation and discard — on top
of the ordinary reference counting that already handles every acyclic closure pattern.
