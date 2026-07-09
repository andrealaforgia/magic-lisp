//! Compiles reader output into bytecode.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::bytecode::{Chunk, Const, Module, Op};
use crate::reader::Sexpr;
use crate::vm::eval_top_level_function;

#[derive(Debug, Clone, PartialEq)]
pub struct CompileError {
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "compile error: {}", self.message)
    }
}

fn err(message: impl Into<String>) -> CompileError {
    CompileError {
        message: message.into(),
    }
}

/// Caps expression-nesting depth so a pathologically deep (but structurally
/// valid) expression tree fails cleanly instead of risking a native stack
/// overflow while compiling.
///
/// `pub(crate)`, not private: `vm::value_to_sexpr` (B14 macro expansion)
/// reuses this exact bound rather than an independently-chosen one, since
/// a macro's expansion result exceeding it would be rejected by THIS
/// module's own `compile_expr` guard anyway once compiled -- failing at
/// the same threshold while still just converting the data, before ever
/// reaching that point, turns what would otherwise be a native stack
/// overflow (qa test-design WARNING, msg #259: confirmed crashing on a
/// sufficiently deep, non-cyclic macro-returned value, since
/// `value_to_sexpr`'s own recursion had no bound of its own to catch it
/// first) into the same clean, already-expected kind of error.
pub(crate) const MAX_NESTING_DEPTH: usize = 512;

fn too_deep() -> CompileError {
    err(format!(
        "expression nesting exceeds the maximum supported depth ({MAX_NESTING_DEPTH})"
    ))
}

/// Caps how many elements a single quasiquote template list/vector may
/// have -- a separate, much larger bound than `MAX_NESTING_DEPTH`, and not
/// interchangeable with it: this check runs BEFORE `expand_qq_sequence`
/// builds its expanded `append`-chain tree at all, while `MAX_NESTING_DEPTH`
/// is `compile_expr`'s own guard checked AFTER that tree already exists,
/// while compiling it. For an ordinary flat list/vector template, that
/// depth guard already rejects anything past ~511 elements on its own --
/// this bound is not a second, independent acceptance threshold that ever
/// decides accept-vs-reject in that shape's case (reusing
/// `MAX_NESTING_DEPTH`'s own value here doesn't work either, though: the
/// expansion carries a little depth overhead by the time `compile_expr`
/// ever sees it, so testing exactly at that value directly would
/// sometimes reject what the depth guard alone would still have
/// accepted). Its real job is a much rarer, much larger failure the depth
/// guard cannot reach in time to prevent: at a large enough element
/// count, DROPPING the expanded tree overflows the native stack on its
/// own (ordinary recursive `Drop` glue, since `Sexpr` has no custom
/// iterative one) before or regardless of whatever `compile_expr` would
/// have done with it (warden security review: confirmed crash at
/// ~110,000+ elements). Comfortably below that threshold is all that
/// actually matters here.
const MAX_QUASIQUOTE_SEQUENCE_LEN: usize = 2_000;

/// Caps how many times a single macro call site's expansion result may
/// itself turn out to be another macro call before giving up (B14, E3): a
/// macro engineered to always expand into another macro call (possibly
/// itself) never reaches a non-macro-call result on its own, and without
/// this bound `compile_macro_call`'s repeat-until-done loop would spin
/// forever instead of failing. Not interchangeable with
/// `MAX_NESTING_DEPTH`: that guard bounds how deep an expression TREE
/// nests; this one bounds how many times ONE call site gets re-expanded in
/// place before compiling whatever it settles on.
const MAX_MACRO_EXPANSION_ROUNDS: usize = 1000;

fn too_many_macro_expansion_rounds() -> CompileError {
    err(format!(
        "macro expansion exceeded the maximum supported rounds ({MAX_MACRO_EXPANSION_ROUNDS}) -- possible infinite macro recursion"
    ))
}

/// Shared by `sexpr_to_const_with_budget` here and
/// `value_to_sexpr_at_depth` (vm.rs) -- both decrement the SAME cumulative
/// `Compilation::macro_conversion_budget_remaining` counter and must report
/// its exhaustion identically regardless of which conversion direction
/// was spending it when the budget ran out.
pub(crate) fn too_much_macro_conversion_work() -> CompileError {
    err(format!(
        "macro expansion exceeded the maximum supported conversion work ({}, across all macro expansion in this compilation) -- too much cumulative conversion cost across many expansions or call sites",
        crate::vm::MAX_MACRO_CONVERSION_BUDGET
    ))
}

/// Threads the module being built, a counter for generating unique internal
/// names (see [`Ctx::aliases`]), and every `define-macro` seen so far
/// (B14) — mapping a macro's name directly to the index of its already-
/// compiled body in `module.functions` (compiled once, at `define-macro`
/// time, exactly like an ordinary function; reused unchanged at every call
/// site rather than recompiled each time).
///
/// Deliberately NOT part of `Ctx`: `Ctx` is cloned and extended per lexical
/// scope (see its own doc comment), but a macro, once defined, stays
/// visible everywhere afterward in the program regardless of lexical
/// nesting — the same "flat, whole-compilation" visibility `Compilation`
/// itself already has, unlike a lexically-scoped binding.
struct Compilation {
    module: Module,
    gensym_counter: u32,
    macros: HashMap<String, u32>,
    /// The runtime `gensym` native's own counter (B14), persisted here and
    /// threaded through every `eval_top_level_function` call in
    /// `compile_macro_call` -- distinct from `gensym_counter` above (this
    /// struct's own compiler-internal hygiene-alias counter, an unrelated
    /// mechanism with a different naming scheme). Warden security review
    /// msg #260: without this, each macro invocation got its OWN fresh
    /// `Vm` with its `gensym` counter reset to 0, so `(gensym)` silently
    /// returned the identical symbol from separate macro calls within the
    /// same compilation -- a real, silent variable-capture risk for any
    /// macro (like this feature's own `swap` demo) that relies on
    /// `gensym` for hygiene.
    macro_gensym_counter: u64,
    /// The trampoline-hop budget remaining for ALL `define-macro` body
    /// execution combined, across the whole `compile_program` call --
    /// threaded through `eval_top_level_function` on every invocation
    /// exactly like `macro_gensym_counter` above, decremented cumulatively
    /// regardless of which macro, which re-expansion round, or which call
    /// site is spending it. Warden security review msg #265: a per-
    /// invocation-only budget let a macro that legitimately re-expands
    /// into itself (each round individually well under budget) cost up to
    /// (budget x round count), multiplying further with however many
    /// independent call sites a file contains -- a 173-byte source file
    /// reached 38 seconds of compile time this way despite no single
    /// round ever exceeding its own bound.
    macro_step_budget_remaining: usize,
    /// The conversion-work budget remaining for ALL `value_to_sexpr`/
    /// `sexpr_to_const` calls combined, across the whole `compile_program`
    /// call -- threaded through `compile_macro_call` on every round
    /// exactly like `macro_step_budget_remaining` above, for the same
    /// reason. Warden security review msgs #292/#293: a native call like
    /// `make-vector` costs the trampoline-step budget above essentially
    /// nothing regardless of how many elements it allocates (it returns
    /// directly from `call_native`, never re-entering the trampoline loop
    /// that budget decrements), while still costing real, uncapped
    /// conversion work every round -- a macro that re-expands into itself,
    /// each round returning a fresh near-`MAX_MACRO_RESULT_ELEMENTS`-sized
    /// value, paid that full per-round conversion cost every one of up to
    /// `MAX_MACRO_EXPANSION_ROUNDS` rounds with nothing summing it, and
    /// this compounds further across independent call sites in one file.
    macro_conversion_budget_remaining: usize,
}

impl Compilation {
    fn new() -> Self {
        Compilation {
            module: Module::default(),
            gensym_counter: 0,
            macros: HashMap::new(),
            macro_gensym_counter: 0,
            macro_step_budget_remaining: crate::vm::MACRO_TRAMPOLINE_STEP_BUDGET,
            macro_conversion_budget_remaining: crate::vm::MAX_MACRO_CONVERSION_BUDGET,
        }
    }

    /// Like `new`, but resumes from a REPL session's carried-forward state
    /// (its accumulated function table and its compiler-internal alias
    /// counter) instead of starting both fresh -- see [`ReplState`]'s own
    /// doc comment for why both must persist across entries.
    fn from_repl_state(state: ReplState) -> Self {
        Compilation {
            module: state.module,
            gensym_counter: state.gensym_counter,
            macros: HashMap::new(),
            macro_gensym_counter: 0,
            macro_step_budget_remaining: crate::vm::MACRO_TRAMPOLINE_STEP_BUDGET,
            macro_conversion_budget_remaining: crate::vm::MAX_MACRO_CONVERSION_BUDGET,
        }
    }

    fn gensym(&mut self, hint: &str) -> String {
        self.gensym_counter += 1;
        format!("%%{hint}%{}", self.gensym_counter)
    }
}

/// The REPL (B17) state that must be carried forward across an interactive
/// session's entries, threaded in and back out of [`compile_repl_entry`] on
/// every call exactly like `globals` is threaded through
/// `vm::eval_repl_entry` -- both halves of the SAME underlying problem:
/// warden security review (msg #327, Critical) found that giving each entry
/// an entirely fresh `Module` let a later entry's function table reuse the
/// SAME numeric index an earlier entry's now-persisted closure
/// (`Value::Closure(idx, ..)`) still refers to, silently resolving the
/// wrong function's body (or, if the aliased index happened to have an
/// incompatible arity, a wrong-but-plausible arity error) the moment a
/// closure created in one entry was called from a later one -- since a
/// closure only stores a bare function-table index with no reference to
/// which module it was compiled into.
///
/// The fix carries the module forward instead, so it only ever grows: an
/// index a closure captured in an earlier entry still refers to that exact
/// same function in every later entry's module too, because that module is
/// always a superset, never a fresh replacement.
///
/// `gensym_counter` must persist for the identical reason, one level down:
/// it names the synthetic global `compile_named_let`/internal `define`s
/// register via `DEF_GLOBAL` (see `Ctx::with_alias`'s call sites), and
/// those synthetic globals land in the SAME persisted `globals` map a
/// closure's own values live in. Resetting this counter to 0 every entry
/// would let two unrelated entries' internal bindings (e.g. two separate
/// named lets both hinted "loop") generate the identical alias name and
/// silently collide in `globals` -- the same class of bug as the module
/// one above, just one layer further down, so it gets the same fix: thread
/// it forward instead of resetting it.
///
/// Deliberately NOT included: `macros`/`macro_gensym_counter`/
/// `macro_step_budget_remaining`/`macro_conversion_budget_remaining`. A
/// `define-macro` seen in one entry is out of scope for this behaviour
/// (see `compile_repl_entry`'s own doc comment) -- only ordinary `define`d
/// VALUES need to persist -- and the macro-expansion budgets are sized per
/// `compile_program`-sized unit of work; an interactive entry is always
/// small enough that resetting them fresh each entry costs nothing real.
pub(crate) struct ReplState {
    module: Module,
    gensym_counter: u32,
}

impl ReplState {
    pub(crate) fn new() -> Self {
        ReplState {
            module: Module::default(),
            gensym_counter: 0,
        }
    }

    pub(crate) fn module(&self) -> &Module {
        &self.module
    }
}

/// Lexical scope for the function body currently being compiled.
///
/// `scopes` holds true local variable slots (function parameters and
/// let/let*-bound names), searched innermost-first; a name found here
/// compiles to GET_LOCAL/SET_LOCAL. Slot indices are never reused within one
/// function, even after a scope is dropped — simpler than slot-packing, at
/// the cost of a few unused runtime slots.
///
/// `aliases` maps a name to a compiler-generated *global* name it should
/// resolve through instead. This is how letrec, named let, and internal
/// (mutually recursive) definitions self- or mutually-reference each other:
/// bound under a unique global name — exactly like top-level `define`, which
/// already supports self-recursion via late-bound global lookup — rather
/// than via real upvalue capture, since none of those bindings need to
/// outlive the function that introduced them the way a captured closure
/// variable does. Unlike `locals`, `aliases` IS inherited into nested
/// function compiles, since it only redirects which global name a
/// GET_GLOBAL resolves through; it needs no real stack-frame access.
///
/// `parent` links to the `Ctx` that was active at the point a nested
/// function (lambda, `define`-sugar, or named-`let`) was compiled, so a free
/// variable inside it can resolve as an upvalue into an enclosing function's
/// real locals (GET_UPVALUE/SET_UPVALUE) instead of only ever falling back
/// to a global lookup — this is what makes real closures (B5) work.
#[derive(Clone)]
struct Ctx {
    // Flat, not nested: resolution always searches innermost-declared-first
    // regardless of which lexical block a name came from, and a `let`'s
    // extended `Ctx` is a clone that is simply discarded once that form
    // finishes compiling — so a scope boundary marker would never actually
    // be consulted. Simpler to just track (name, slot) pairs directly.
    locals: Vec<(String, u8)>,
    // Shared (not per-clone) on purpose: at runtime, PUSH_LOCAL only ever
    // grows one flat per-frame Vec<Value> and nothing ever pops from it, so
    // two `let`s in the same function — nested OR sequential siblings —
    // must never be allocated the same slot number, even though each one's
    // own extended Ctx clone is discarded once that form finishes
    // compiling. A plain `u8` here would let sibling `let`s each start
    // counting from the same inherited value and collide on one runtime
    // slot; sharing the counter via Rc<Cell<_>> keeps allocation
    // monotonic across every clone descended from the same function-level
    // Ctx, while `locals` itself stays independently cloned so name
    // visibility/shadowing is still scoped correctly.
    next_slot: Rc<Cell<u8>>,
    aliases: Vec<(String, String)>,
    parent: Option<Rc<Ctx>>,
}

impl Ctx {
    fn top_level() -> Self {
        Ctx {
            locals: Vec::new(),
            next_slot: Rc::new(Cell::new(0)),
            aliases: Vec::new(),
            parent: None,
        }
    }

    fn for_function(
        params: Vec<String>,
        aliases: Vec<(String, String)>,
        parent: Option<Rc<Ctx>>,
    ) -> Result<Self, CompileError> {
        let mut ctx = Ctx {
            locals: Vec::new(),
            next_slot: Rc::new(Cell::new(0)),
            aliases,
            parent,
        };
        for p in params {
            ctx.declare(p)?;
        }
        Ok(ctx)
    }

    /// Declares a new local slot, failing cleanly (rather than overflowing
    /// `next_slot: u8`) once a function has accumulated the maximum
    /// representable number of locals across its parameters and any nested
    /// `let`/`let*` bindings.
    fn declare(&mut self, name: String) -> Result<u8, CompileError> {
        let slot = self.next_slot.get();
        if slot == u8::MAX {
            return Err(err(format!(
                "too many local bindings in one function (maximum {})",
                u8::MAX
            )));
        }
        self.next_slot.set(slot + 1);
        self.locals.push((name, slot));
        Ok(slot)
    }

    fn resolve_local(&self, name: &str) -> Option<u8> {
        self.locals
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, slot)| *slot)
    }

    fn resolve_alias(&self, name: &str) -> Option<&str> {
        self.aliases
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a.as_str())
    }

    /// Resolves `name` as an upvalue by walking the `parent` chain outward:
    /// depth 1 is the immediately enclosing function's own locals, depth 2
    /// its parent's, and so on — matching how `Vm::resolve_env` counts
    /// levels at runtime. Only ever consults each ancestor's `locals`
    /// (never its `aliases` or globals), since an alias resolves through a
    /// global regardless of nesting and needs no upvalue at all.
    /// `Ok(None)`: `name` isn't a local anywhere in the enclosing chain (not
    /// an upvalue at all — the caller should fall back to a global lookup).
    /// `Ok(Some(_))`: found, encodable in the bytecode's `u8` depth operand.
    /// `Err(_)`: found, but more than 255 enclosing-function levels away —
    /// a hard compile error instead of silently falling through to a global
    /// lookup, which would resolve to the wrong value if a same-named
    /// global happens to exist (a security-review finding: `depth: u8`
    /// overflowing past 255 used to be treated identically to "not an
    /// upvalue," a silent wrong-answer risk, not just a rejected program).
    fn resolve_upvalue(&self, name: &str) -> Result<Option<(u8, u8)>, CompileError> {
        let mut depth: u8 = 1;
        let mut current = self.parent.as_deref();
        while let Some(ctx) = current {
            if let Some(slot) = ctx.resolve_local(name) {
                return Ok(Some((depth, slot)));
            }
            current = ctx.parent.as_deref();
            depth = depth.checked_add(1).ok_or_else(|| {
                err(format!(
                    "'{name}' is captured through too many levels of nested functions \
                     (more than {} — this bytecode format can't encode a deeper upvalue)",
                    u8::MAX
                ))
            })?;
        }
        Ok(None)
    }

    fn with_alias(&self, name: String, alias: String) -> Ctx {
        let mut ctx = self.clone();
        ctx.aliases.push((name, alias));
        ctx
    }
}

enum Formals {
    Fixed(Vec<String>),
    FixedPlusRest(Vec<String>, String),
    AllRest(String),
}

fn expect_symbol_name(sexpr: &Sexpr) -> Result<String, CompileError> {
    match sexpr {
        Sexpr::Symbol(s) => Ok(s.clone()),
        other => Err(err(format!("expected a parameter name, found {other:?}"))),
    }
}

fn parse_formals(sexpr: &Sexpr) -> Result<Formals, CompileError> {
    match sexpr {
        Sexpr::Symbol(s) => Ok(Formals::AllRest(s.clone())),
        Sexpr::List(items) => {
            let names = items
                .iter()
                .map(expect_symbol_name)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Formals::Fixed(names))
        }
        Sexpr::DottedList(items, tail) => {
            let names = items
                .iter()
                .map(expect_symbol_name)
                .collect::<Result<Vec<_>, _>>()?;
            let tail_name = expect_symbol_name(tail)?;
            Ok(Formals::FixedPlusRest(names, tail_name))
        }
        other => Err(err(format!("invalid parameter list: {other:?}"))),
    }
}

pub(crate) fn sexpr_to_const(sexpr: &Sexpr) -> Result<Const, CompileError> {
    // Deliberately unbounded (`usize::MAX`, not `MAX_MACRO_CONVERSION_BUDGET`):
    // this plain entry point serves ordinary, non-cumulative callers
    // (`quote`, `case` clause literals) whose cost is a single isolated
    // conversion, not the repeated macro-expansion-round/call-site
    // compounding that budget exists to cap -- warden security review msg
    // #146 already established that even a legitimately huge literal
    // (e.g. a million-element `quote`d list) must still succeed here.
    let mut budget = usize::MAX;
    sexpr_to_const_with_budget(sexpr, &mut budget)
}

/// Like [`sexpr_to_const`], but threads a conversion-work budget the
/// caller controls the lifetime of -- used by `compile_macro_call` to pass
/// `Compilation::macro_conversion_budget_remaining` so the same counter
/// persists across every round and every macro call site in one
/// `compile_program` call (mirroring `value_to_sexpr_with_budget`, vm.rs,
/// for the identical reason), instead of each call getting its own fresh
/// allowance the way the plain [`sexpr_to_const`] above does for ordinary,
/// non-cumulative callers (`quote`, `case` clause literals).
pub(crate) fn sexpr_to_const_with_budget(
    sexpr: &Sexpr,
    budget: &mut usize,
) -> Result<Const, CompileError> {
    *budget = budget
        .checked_sub(1)
        .ok_or_else(too_much_macro_conversion_work)?;
    Ok(match sexpr {
        Sexpr::Int(n) => Const::Int(*n),
        Sexpr::Float(n) => Const::Float(*n),
        Sexpr::Bool(b) => Const::Bool(*b),
        Sexpr::Char(c) => Const::Char(*c),
        Sexpr::Str(s) => Const::Str(s.clone()),
        Sexpr::Symbol(s) => Const::Symbol(s.clone()),
        Sexpr::List(items) => Const::List(
            items
                .iter()
                .map(|item| sexpr_to_const_with_budget(item, budget))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Sexpr::Vector(items) => Const::Vector(
            items
                .iter()
                .map(|item| sexpr_to_const_with_budget(item, budget))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Sexpr::DottedList(items, tail) => {
            let mut acc = sexpr_to_const_with_budget(tail, budget)?;
            for item in items.iter().rev() {
                acc = Const::Pair(
                    Box::new(sexpr_to_const_with_budget(item, budget)?),
                    Box::new(acc),
                );
            }
            acc
        }
    })
}

fn expect_bindings_list(sexpr: &Sexpr) -> Result<Vec<(String, Sexpr)>, CompileError> {
    match sexpr {
        Sexpr::List(items) => items
            .iter()
            .map(|item| match item {
                Sexpr::List(pair) if pair.len() == 2 => {
                    let name = expect_symbol_name(&pair[0])?;
                    Ok((name, pair[1].clone()))
                }
                other => Err(err(format!(
                    "expected a (name init) binding, found {other:?}"
                ))),
            })
            .collect(),
        other => Err(err(format!("expected a list of bindings, found {other:?}"))),
    }
}

/// `do`'s bindings are `(name init step)` or, with the step omitted,
/// `(name init)` — an omitted step just re-binds `name` to itself each
/// iteration, so it defaults to a reference to the variable.
fn expect_do_bindings(sexpr: &Sexpr) -> Result<Vec<(String, Sexpr, Sexpr)>, CompileError> {
    match sexpr {
        Sexpr::List(items) => items
            .iter()
            .map(|item| match item {
                Sexpr::List(triple) if triple.len() == 3 => {
                    let name = expect_symbol_name(&triple[0])?;
                    Ok((name, triple[1].clone(), triple[2].clone()))
                }
                Sexpr::List(pair) if pair.len() == 2 => {
                    let name = expect_symbol_name(&pair[0])?;
                    Ok((name.clone(), pair[1].clone(), Sexpr::Symbol(name)))
                }
                other => Err(err(format!(
                    "expected a (name init step) or (name init) do-binding, found {other:?}"
                ))),
            })
            .collect(),
        other => Err(err(format!(
            "expected a list of do-bindings, found {other:?}"
        ))),
    }
}

/// Matches `src/vm.rs`'s own `VM_STACK_SIZE`: compiling, like running, can
/// recurse deeply enough (through `compile_expr`'s ordinary nesting, or
/// through `expand_quasiquote`/`expand_qq_sequence`'s mutual recursion) that
/// how much native stack is actually available shouldn't be left to
/// whatever the CALLING thread happens to have -- a platform default, a
/// constrained embedding context, or simply a smaller stack than this
/// process's own main thread gets. Running on a thread with an explicit,
/// generous size of our own choosing makes every depth guard's safety
/// margin a fixed, known quantity instead of one that silently varies with
/// the caller (warden security review msg #242: confirmed by sweeping
/// stack sizes that an identical logical nesting depth crashes below one
/// threshold for one template shape but a different, lower one for
/// another, purely from how many real stack frames each shape's recursion
/// costs per depth unit -- decoupling from the caller's stack removes that
/// variable entirely rather than trying to keep re-tuning depth limits
/// against it).
///
/// Matches `VM_STACK_SIZE`'s own value exactly, deliberately -- not
/// independently tuned. Mutation testing can't observe the exact
/// multiplier here: even shrinking this expression to ~3 MiB (the last
/// `*` folded into a `+`) still comfortably clears the ~1.5-2 MiB crash
/// threshold msg #242 measured for this codebase's own test depths, so no
/// committed test distinguishes "generously oversized" from "some smaller
/// but still-sufficient value" without hard-coding a fragile assumption
/// about exactly how deep those tests happen to recurse.
const COMPILE_STACK_SIZE: usize = 3 * 1024 * 1024 * 1024;

pub fn compile_program(forms: &[Sexpr]) -> Result<Module, CompileError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(COMPILE_STACK_SIZE)
            .spawn_scoped(scope, || compile_program_on_this_thread(forms))
            .map_err(|e| err(format!("failed to spawn compiler thread: {e}")))?
            .join()
            .unwrap_or_else(|_| Err(err("internal error: compiler thread panicked")))
    })
}

fn compile_program_on_this_thread(forms: &[Sexpr]) -> Result<Module, CompileError> {
    let mut comp = Compilation::new();
    let mut entry = Chunk::new();
    let ctx = Ctx::top_level();
    for form in forms {
        // Never tail: each top-level form is followed by its own POP, so
        // none of them is "the last thing" the entry chunk does.
        compile_expr(form, &ctx, &mut entry, &mut comp, 0, false)?;
        entry.emit_pop();
    }
    entry.emit_halt();
    comp.module.entry_index = comp.module.functions.len() as u32;
    comp.module.functions.push(entry);
    Ok(comp.module)
}

/// Compiles one REPL entry (B17) as a callable, zero-arg function whose
/// body is the entry's own top-level forms -- all but the last discarded
/// via `POP` exactly like [`compile_program_on_this_thread`]'s own entry
/// chunk, but the LAST form's value is left on the stack and returned via
/// `RETURN` instead of being popped and the whole thing ending in `HALT`.
/// This is what lets a REPL session auto-print each entry's result: an
/// ordinary top-level program's entry chunk has nowhere for that value to
/// go once `HALT` runs, since every one of its own top-level forms is
/// unconditionally followed by a `POP`.
///
/// Deliberately NOT `compile_body`: that function gives LEADING `define`
/// forms special internal-alias-only handling (correct for a function/
/// `let` body, where an internal `define` never creates a real global),
/// but a REPL entry's `(define x ...)` must create a REAL global -- the
/// same one `compile_define`'s ordinary top-level path already produces
/// -- since it has to remain visible to every LATER entry in the session
/// (B17 spec 9.1's persistence requirement), not just later expressions
/// within this same entry.
///
/// Takes `state` by value and always returns it back (see [`ReplState`]'s
/// own doc comment for why it must keep growing across entries rather than
/// reset), paired with the `Result` for just this entry's own function
/// index -- not `Result<(ReplState, u32), _>`, since a compile error must
/// not lose whatever `state` already grew to on a previous, successfully
/// compiled entry.
///
/// Each entry still gets a fresh macro registry and macro-related budgets
/// (only `ReplState`'s two fields persist) -- out of scope for this
/// behaviour, which only requires ordinary VALUE bindings (`define`) to
/// persist across entries, not `define-macro`.
pub(crate) fn compile_repl_entry(
    state: ReplState,
    forms: &[Sexpr],
) -> (ReplState, Result<u32, CompileError>) {
    let mut comp = Compilation::from_repl_state(state);
    let result = compile_repl_entry_body(&mut comp, forms);
    let state = ReplState {
        module: comp.module,
        gensym_counter: comp.gensym_counter,
    };
    (state, result)
}

fn compile_repl_entry_body(comp: &mut Compilation, forms: &[Sexpr]) -> Result<u32, CompileError> {
    let mut fn_chunk = Chunk::new();
    let ctx = Ctx::top_level();
    let Some((last, rest)) = forms.split_last() else {
        let idx = fn_chunk.add_const(Const::Unspecified);
        fn_chunk.emit_const(idx);
        fn_chunk.emit_return();
        let index = comp.module.functions.len() as u32;
        comp.module.functions.push(fn_chunk);
        return Ok(index);
    };
    for form in rest {
        compile_expr(form, &ctx, &mut fn_chunk, comp, 0, false)?;
        fn_chunk.emit_pop();
    }
    compile_expr(last, &ctx, &mut fn_chunk, comp, 0, true)?;
    fn_chunk.emit_return();
    let index = comp.module.functions.len() as u32;
    comp.module.functions.push(fn_chunk);
    Ok(index)
}

/// Compiles a body of expressions where all but the last are evaluated for
/// effect and discarded, and the last one's value is left on the stack.
/// Used for `begin` and, via [`compile_body`], for function/let bodies.
/// `tail` describes whether *this sequence's own result* is itself in tail
/// position (per spec 3.8) — only ever relevant to the last expression,
/// since every earlier one is followed by a POP, not a return.
fn compile_sequence(
    exprs: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let Some((last, rest)) = exprs.split_last() else {
        let idx = chunk.add_const(Const::Unspecified);
        chunk.emit_const(idx);
        return Ok(());
    };
    for e in rest {
        compile_expr(e, ctx, chunk, comp, depth, false)?;
        chunk.emit_pop();
    }
    compile_expr(last, ctx, chunk, comp, depth, tail)
}

fn is_define_form(expr: &Sexpr) -> bool {
    matches!(expr, Sexpr::List(items) if matches!(items.first(), Some(Sexpr::Symbol(s)) if s == "define"))
}

/// Extracts `(name, value-expr)` from a `(define name expr)` or
/// `(define (name . formals) body...)` form, desugaring the function-sugar
/// shape into an equivalent `(lambda formals body...)` value expression,
/// without emitting any bytecode yet.
fn extract_define_binding(items: &[Sexpr]) -> Result<(String, Sexpr), CompileError> {
    let head = items
        .get(1)
        .ok_or_else(|| err("define requires a name or a (name . formals) head"))?;
    match head {
        Sexpr::Symbol(name) => {
            if items.len() != 3 {
                return Err(err(
                    "define with a plain name takes exactly one value expression",
                ));
            }
            Ok((name.clone(), items[2].clone()))
        }
        Sexpr::List(head_items) => {
            let (name_sexpr, formal_items) = head_items
                .split_first()
                .ok_or_else(|| err("define's function head cannot be empty"))?;
            let name = expect_symbol_name(name_sexpr)?;
            let formals = Sexpr::List(formal_items.to_vec());
            let mut lambda = vec![Sexpr::Symbol("lambda".to_string()), formals];
            lambda.extend(items[2..].iter().cloned());
            Ok((name, Sexpr::List(lambda)))
        }
        Sexpr::DottedList(head_items, tail) => {
            let (name_sexpr, formal_items) = head_items
                .split_first()
                .ok_or_else(|| err("define's function head cannot be empty"))?;
            let name = expect_symbol_name(name_sexpr)?;
            let formals = if formal_items.is_empty() {
                (**tail).clone()
            } else {
                Sexpr::DottedList(formal_items.to_vec(), tail.clone())
            };
            let mut lambda = vec![Sexpr::Symbol("lambda".to_string()), formals];
            lambda.extend(items[2..].iter().cloned());
            Ok((name, Sexpr::List(lambda)))
        }
        other => Err(err(format!("invalid define head: {other:?}"))),
    }
}

/// Compiles a function/let-family body: a leading run of `(define ...)`
/// forms is treated as one mutually-visible group (every name sees every
/// other, including itself, regardless of declaration order — like
/// `letrec`), followed by the remaining expressions as an ordinary
/// sequence.
fn compile_body(
    exprs: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let split = exprs.iter().take_while(|e| is_define_form(e)).count();
    let (defines, rest) = exprs.split_at(split);

    if defines.is_empty() {
        return compile_sequence(rest, ctx, chunk, comp, depth, tail);
    }

    let mut extended = ctx.clone();
    let mut bindings = Vec::with_capacity(defines.len());
    for d in defines {
        let Sexpr::List(items) = d else {
            unreachable!("is_define_form guarantees a list")
        };
        let (name, init) = extract_define_binding(items)?;
        let alias = comp.gensym(&name);
        extended = extended.with_alias(name, alias.clone());
        bindings.push((alias, init));
    }
    for (alias, init) in &bindings {
        compile_expr(init, &extended, chunk, comp, depth + 1, false)?;
        let idx = chunk.add_const(Const::Symbol(alias.clone()));
        chunk.emit_def_global(idx);
        chunk.emit_pop();
    }
    compile_sequence(rest, &extended, chunk, comp, depth, tail)
}

/// Compiles `formals body...` into a new function chunk appended to the
/// module's function table, returning its index. Shared by `lambda`,
/// `define`'s function-definition sugar, and named `let`.
fn compile_function(
    formals_sexpr: &Sexpr,
    body: &[Sexpr],
    comp: &mut Compilation,
    depth: usize,
    enclosing_ctx: &Ctx,
    name: Option<String>,
) -> Result<u32, CompileError> {
    let formals = parse_formals(formals_sexpr)?;
    let (params, arity, has_rest) = match formals {
        Formals::Fixed(names) => {
            let arity = names.len() as u32;
            (names, arity, false)
        }
        Formals::FixedPlusRest(mut names, rest) => {
            let arity = names.len() as u32;
            names.push(rest);
            (names, arity, true)
        }
        Formals::AllRest(rest) => (vec![rest], 0, true),
    };

    let ctx = Ctx::for_function(
        params,
        enclosing_ctx.aliases.clone(),
        Some(Rc::new(enclosing_ctx.clone())),
    )?;
    let mut fn_chunk = Chunk::new();
    fn_chunk.arity = arity;
    fn_chunk.has_rest = has_rest;
    fn_chunk.name = name;
    // A function's own body starts a fresh tail-position context: its last
    // expression's value IS this function's return value, regardless of
    // whatever tail status the *lambda-creating* expression itself had.
    compile_body(body, &ctx, &mut fn_chunk, comp, depth + 1, true)?;
    fn_chunk.emit_return();

    let index = comp.module.functions.len() as u32;
    comp.module.functions.push(fn_chunk);
    Ok(index)
}

fn compile_quote(items: &[Sexpr], chunk: &mut Chunk) -> Result<(), CompileError> {
    if items.len() != 2 {
        return Err(err("quote requires exactly one datum"));
    }
    let const_value = sexpr_to_const(&items[1])?;
    let idx = chunk.add_const(const_value);
    chunk.emit_const(idx);
    Ok(())
}

fn compile_quasiquote(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let [_, template] = items else {
        return Err(err(format!(
            "quasiquote requires exactly one template, got {}",
            items.len().saturating_sub(1)
        )));
    };
    let expanded = expand_quasiquote(template, 1, 1)?;
    // The `+ 1` here is a one-level safety margin, not the load-bearing
    // part of the depth limit: the expansion (built from nested `list`/
    // `append` calls) is itself compiled through the ordinary call path
    // right below, which adds its own many levels of depth well before
    // this single increment's presence or absence could matter -- hand-
    // verified via mutation testing (forcing this to `depth * 1` instead
    // still produces a clean "nesting exceeds maximum" error for a
    // template deep enough to matter, never a crash).
    compile_expr(&expanded, ctx, chunk, comp, depth + 1, tail)
}

/// True if `items` is the 2-element shape `(tag datum)` the reader produces
/// for `` `x ``/`,x`/`,@x` shorthand (spec 3.4) -- e.g.
/// `is_tagged(items, "unquote")` recognizes `(unquote x)`.
fn is_tagged<'a>(items: &'a [Sexpr], tag: &str) -> Option<&'a Sexpr> {
    match items {
        [Sexpr::Symbol(s), datum] if s == tag => Some(datum),
        _ => None,
    }
}

/// `(list (quote sym) expr)` -- code that, when evaluated, reconstructs the
/// 2-element `(sym <value-of-expr>)` shape the reader produces for
/// `` `x ``/`,x`/`,@x`, used when a marker doesn't reach nesting level 0 and
/// must survive as literal (but still recursively-expanded) data instead of
/// being evaluated.
fn qq_reconstruct_tagged(tag: &str, expr: Sexpr) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::Symbol("list".to_string()),
        Sexpr::List(vec![
            Sexpr::Symbol("quote".to_string()),
            Sexpr::Symbol(tag.to_string()),
        ]),
        expr,
    ])
}

/// Expands a quasiquote template (spec 3.4) into an ordinary expression --
/// built entirely from the existing `quote`/`list`/`append`/`list->vector`
/// forms -- that reconstructs the template's data when evaluated, with each
/// `,expr` replaced by `expr`'s runtime value and each `,@expr` splicing
/// `expr`'s (list-valued) runtime value's elements in directly. Reuses the
/// ordinary compilation path for the expansion result rather than adding
/// any new bytecode: this is exactly the same "desugar into existing
/// `Sexpr` forms, then recurse through `compile_expr`" strategy `do`'s
/// expansion into `let`/`if` already uses.
///
/// `level` is the current quasiquote nesting depth (starting at 1 for the
/// template right after a backquote): a nested backquote raises it by one,
/// and an unquote/unquote-splicing marker lowers it by one -- only a marker
/// that brings `level` all the way to 0 is actually evaluated; one that
/// doesn't reach 0 is reconstructed as literal (still-tagged) data instead,
/// via `qq_reconstruct_tagged`, continuing to expand recursively inside it
/// in case something deeper still reaches 0 (spec 3.4's doubly-nested case).
///
/// `depth` is a native-recursion-depth safety counter, unrelated to
/// `level`'s quasiquote-nesting bookkeeping above -- restored (qa
/// test-design review msg #225) after being removed once as apparently
/// redundant with `compile_expr`'s own `MAX_NESTING_DEPTH` check on the
/// fully-expanded result tree. That removal relied on an unstated
/// invariant: only the reader ever produces trees fed to `compile_program`,
/// and the reader's own guard keeps real backquote-shorthand nesting far
/// below any depth that could matter. A hand-built `Sexpr` tree (any
/// direct caller of `compile_program`, bypassing the reader) has no such
/// bound -- and unlike a flat list template's element count, THIS
/// recursion happens entirely inside this function, before `compile_expr`
/// ever sees anything to check, so its own downstream guard cannot help
/// here at all: confirmed as a genuine native stack overflow via
/// `compile_program`'s public API in a DEBUG build at a hand-built
/// nesting depth as low as 2,000, independent of `compile_expr`'s guard
/// entirely. Build-profile-dependent, not a fixed number: qa test-design
/// review msg #238 independently confirmed a release build's smaller
/// stack frames push the actual crash threshold out to roughly 30,000+ on
/// typical hardware (expect this to vary by platform/stack size -- the
/// mechanism, a hand-built AST defeating the downstream guard via pure
/// native recursion, is the fact that matters, not either specific
/// number).
///
/// Mutation testing cannot observe this check's own correctness (the `>`
/// here, or any individual recursive call's `depth + 1`) at any depth
/// that's safe to use in a committed test: `compile_expr`'s separate,
/// pre-existing downstream guard on the fully-expanded result ALSO
/// independently fires at essentially the same depth for every template
/// shape this file's tests construct, so "some nesting error occurred"
/// stays true whether this specific counter is correct, inverted, or
/// disabled -- only an unsafe-to-commit depth distinguishes them, which
/// is why the tests below stop at a depth low enough to never risk
/// reproducing the crash. Hand-verified instead, the same way the actual
/// crash fix above was: temporarily breaking the comparison (an
/// unreachably-high threshold) and each `depth + 1` this function
/// contains or passes to `expand_qq_sequence` (negating it) independently
/// reproduces the exact stack overflow the `#[ignore]`d probe test below
/// exists to catch, confirming each is genuinely load-bearing despite
/// being untestable through output alone at any depth this test suite
/// can safely reach.
fn expand_quasiquote(template: &Sexpr, level: u32, depth: usize) -> Result<Sexpr, CompileError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(err(format!(
            "quasiquote template nesting exceeds the maximum supported depth ({MAX_NESTING_DEPTH})"
        )));
    }
    match template {
        Sexpr::List(items) => {
            if let Some(inner) = is_tagged(items, "quasiquote") {
                let expanded_inner = expand_quasiquote(inner, level + 1, depth + 1)?;
                return Ok(qq_reconstruct_tagged("quasiquote", expanded_inner));
            }
            if let Some(inner) = is_tagged(items, "unquote") {
                return if level == 1 {
                    Ok(inner.clone())
                } else {
                    let expanded_inner = expand_quasiquote(inner, level - 1, depth + 1)?;
                    Ok(qq_reconstruct_tagged("unquote", expanded_inner))
                };
            }
            if is_tagged(items, "unquote-splicing").is_some() {
                return Err(err(
                    "unquote-splicing is only valid as an element of a list or vector template",
                ));
            }
            expand_qq_sequence(items, level, depth)
        }
        Sexpr::Vector(items) => {
            let list_expr = expand_qq_sequence(items, level, depth)?;
            Ok(Sexpr::List(vec![
                Sexpr::Symbol("list->vector".to_string()),
                list_expr,
            ]))
        }
        Sexpr::DottedList(items, tail) => {
            // NOT `append`: `append`'s second argument must itself be a
            // proper list (spec 5.1), but a dotted template's tail is
            // exactly the value that ISN'T one. `(fold-right cons tail
            // head-list)` builds the correct cons chain ending in
            // whatever `tail` evaluates to, proper list or not, reusing
            // two more already-existing natives (warden security review
            // msg #221: this previously crashed at runtime with an
            // "append expects a proper list" error for e.g. `` `(a . b) ``).
            //
            // `head`'s call doesn't increment `depth`, matching the plain
            // `List`/`Vector` arms above (delegating to iterate THIS
            // list's own elements isn't itself a deeper nesting level --
            // `expand_qq_sequence`'s own per-element recursion charges
            // the one `depth + 1` that a genuinely deeper element earns).
            // `tail`'s call DOES, since a chain of dotted pairs is exactly
            // as deep, form for form, as the reader's own recursive
            // descent into each one (warden security review msg #240:
            // double-charging both of these silently halved the usable
            // nesting depth for ordinary templates).
            let head = expand_qq_sequence(items, level, depth)?;
            let expanded_tail = expand_quasiquote(tail, level, depth + 1)?;
            Ok(Sexpr::List(vec![
                Sexpr::Symbol("fold-right".to_string()),
                Sexpr::Symbol("cons".to_string()),
                expanded_tail,
                head,
            ]))
        }
        // An atomic/self-evaluating datum (or a plain symbol -- data here,
        // not a variable reference): literal, unchanged regardless of level.
        other => Ok(Sexpr::List(vec![
            Sexpr::Symbol("quote".to_string()),
            other.clone(),
        ])),
    }
}

/// Expands every element of a list/vector template into one `append` chain
/// of "pieces": an ordinary element contributes a one-element `(list
/// <value>)` piece, while `,@expr` at the level that actually splices here
/// contributes `expr` itself directly (its own list value's elements, not
/// wrapped in another list) -- the one place list- and vector-template
/// expansion actually differ (vector wraps the result in `list->vector`;
/// see the caller).
///
/// Both `depth + 1`s below carry the exact same load-bearing
/// recursion-depth safety this function's caller documents on itself --
/// each recurses back into `expand_quasiquote`, so an element (or an
/// unquote-splicing operand) that's itself deeply quasiquote-nested hits
/// the identical native-stack-overflow risk. Same hand-verification
/// result too: temporarily breaking either one reproduces the crash the
/// `#[ignore]`d probe test exists to catch, but only at a depth too
/// unsafe to commit -- at any depth safe to test, `compile_expr`'s own
/// separate downstream guard already independently catches the same
/// input regardless, making these specific counters unobservable through
/// committed-test output alone.
fn expand_qq_sequence(items: &[Sexpr], level: u32, depth: usize) -> Result<Sexpr, CompileError> {
    if items.len() > MAX_QUASIQUOTE_SEQUENCE_LEN {
        return Err(err(format!(
            "quasiquote template list/vector has {} elements, exceeding the maximum of {}",
            items.len(),
            MAX_QUASIQUOTE_SEQUENCE_LEN
        )));
    }
    let mut pieces = Vec::with_capacity(items.len());
    for item in items {
        let splice_target = match item {
            Sexpr::List(inner) => is_tagged(inner, "unquote-splicing"),
            _ => None,
        };
        match splice_target {
            Some(inner) if level == 1 => pieces.push(inner.clone()),
            Some(inner) => {
                let expanded_inner = expand_quasiquote(inner, level - 1, depth + 1)?;
                pieces.push(wrap_singleton_list(qq_reconstruct_tagged(
                    "unquote-splicing",
                    expanded_inner,
                )));
            }
            None => {
                let value_expr = expand_quasiquote(item, level, depth + 1)?;
                pieces.push(wrap_singleton_list(value_expr));
            }
        }
    }
    Ok(fold_append(pieces))
}

fn wrap_singleton_list(expr: Sexpr) -> Sexpr {
    Sexpr::List(vec![Sexpr::Symbol("list".to_string()), expr])
}

fn qq_append(a: Sexpr, b: Sexpr) -> Sexpr {
    Sexpr::List(vec![Sexpr::Symbol("append".to_string()), a, b])
}

/// Combines "pieces" (each already an expression whose value is a list of
/// the elements it contributes) into one right-associated `append` chain --
/// `append` itself only takes exactly 2 arguments (spec 5.1), so N pieces
/// need N-1 nested calls, not one variadic one.
fn fold_append(mut pieces: Vec<Sexpr>) -> Sexpr {
    match pieces.pop() {
        // A single remaining piece, after popping, folds over zero further
        // pieces below and comes back unchanged -- so there's no separate
        // "exactly one piece" case to handle; only "none at all" (an empty
        // template list, e.g. `` `() ``, which has no piece to pop and
        // needs its own literal-empty-list result) is actually distinct.
        None => Sexpr::List(vec![
            Sexpr::Symbol("quote".to_string()),
            Sexpr::List(vec![]),
        ]),
        Some(last) => pieces
            .into_iter()
            .rev()
            .fold(last, |acc, piece| qq_append(piece, acc)),
    }
}

fn compile_if(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    if items.len() < 3 || items.len() > 4 {
        return Err(err(
            "if requires a condition and a then-branch, and takes an optional else-branch",
        ));
    }
    let condition = &items[1];
    let then_branch = &items[2];
    let else_branch = items.get(3);

    compile_expr(condition, ctx, chunk, comp, depth + 1, false)?;
    let else_jump = chunk.emit_jump(Op::JumpIfFalse);
    compile_expr(then_branch, ctx, chunk, comp, depth + 1, tail)?;
    let end_jump = chunk.emit_jump(Op::Jump);

    chunk.patch_jump(else_jump);
    match else_branch {
        Some(else_expr) => compile_expr(else_expr, ctx, chunk, comp, depth + 1, tail)?,
        None => {
            let idx = chunk.add_const(Const::Unspecified);
            chunk.emit_const(idx);
        }
    }
    chunk.patch_jump(end_jump);
    Ok(())
}

fn compile_define(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
) -> Result<(), CompileError> {
    // A define reached here (top-level, or one that isn't part of a leading
    // run at the start of a body — compile_body handles that case) binds a
    // real global under its own literal name.
    let (name, value_expr) = extract_define_binding(items)?;
    // `(define (f ...) ...)` desugars (above, in extract_define_binding) to
    // `(define f (lambda ...))` -- recognized here and compiled directly
    // via `compile_function` (bypassing `compile_lambda`'s own, otherwise
    // identical, dispatch) SPECIFICALLY so `f`'s name can be threaded
    // through to the compiled chunk (B16): `compile_expr`'s ordinary
    // `"lambda"` dispatch has no name to give it, since by the time it
    // runs, the binding's own name has already been erased by this exact
    // desugaring. An ordinary `(define x <non-lambda-expr>)` (including
    // one whose expression merely EVALUATES to a procedure value, e.g.
    // `(define x (if #t + -))`) takes the unnamed path below unchanged.
    if let Sexpr::List(lambda_items) = &value_expr
        && matches!(lambda_items.first(), Some(Sexpr::Symbol(s)) if s == "lambda")
    {
        let formals_sexpr = lambda_items
            .get(1)
            .ok_or_else(|| err("lambda requires a parameter list"))?;
        let fn_index = compile_function(
            formals_sexpr,
            &lambda_items[2..],
            comp,
            depth,
            ctx,
            Some(name.clone()),
        )?;
        chunk.emit_make_function(fn_index);
        let idx = chunk.add_const(Const::Symbol(name));
        chunk.emit_def_global(idx);
        return Ok(());
    }
    compile_expr(&value_expr, ctx, chunk, comp, depth + 1, false)?;
    let idx = chunk.add_const(Const::Symbol(name));
    chunk.emit_def_global(idx);
    Ok(())
}

/// Splits `define-macro`'s `(name . formals)` head into the macro's name
/// and a standalone formals `Sexpr` -- the same shape [`extract_define_
/// binding`] extracts for `define`'s own function shorthand, kept as its
/// own small function rather than reusing that one directly since it
/// returns a formals `Sexpr` alone (fed straight to [`compile_function`]),
/// not a synthetic `lambda` form wrapping a name/value pair.
fn split_define_macro_head(head: &Sexpr) -> Result<(String, Sexpr), CompileError> {
    match head {
        Sexpr::List(head_items) => {
            let (name_sexpr, formal_items) = head_items
                .split_first()
                .ok_or_else(|| err("define-macro's head cannot be empty"))?;
            let name = expect_symbol_name(name_sexpr)?;
            Ok((name, Sexpr::List(formal_items.to_vec())))
        }
        Sexpr::DottedList(head_items, tail) => {
            let (name_sexpr, formal_items) = head_items
                .split_first()
                .ok_or_else(|| err("define-macro's head cannot be empty"))?;
            let name = expect_symbol_name(name_sexpr)?;
            let formals = if formal_items.is_empty() {
                (**tail).clone()
            } else {
                Sexpr::DottedList(formal_items.to_vec(), tail.clone())
            };
            Ok((name, formals))
        }
        other => Err(err(format!(
            "define-macro requires a (name . formals) head, got: {other:?}"
        ))),
    }
}

/// `(define-macro (name . formals) body...)` (B14): compiles the macro's
/// body exactly like an ordinary function -- reusing [`compile_function`]
/// directly, not wrapped in a synthetic `lambda` `Sexpr` -- and registers
/// `name` in `comp.macros` pointing at that compiled body's index. Nothing
/// observable happens at runtime: unlike `define`, no global is created
/// (a macro is a compile-time-only construct; expanding a call to it never
/// goes through `GET_GLOBAL`), so this just leaves the same `Unspecified`
/// placeholder on the stack every top-level form needs for the `POP`
/// `compile_program` emits after it.
fn compile_define_macro(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
) -> Result<(), CompileError> {
    let head = items
        .get(1)
        .ok_or_else(|| err("define-macro requires a (name . formals) head"))?;
    let (name, formals_sexpr) = split_define_macro_head(head)?;
    let fn_index = compile_function(
        &formals_sexpr,
        &items[2..],
        comp,
        depth,
        ctx,
        Some(name.clone()),
    )?;
    comp.macros.insert(name, fn_index);
    let idx = chunk.add_const(Const::Unspecified);
    chunk.emit_const(idx);
    Ok(())
}

/// True if `name` is shadowed by an ordinary binding visible at `ctx` --
/// exactly the same three lookups, in the same order, [`compile_expr`]'s
/// own `Sexpr::Symbol` arm already checks before falling back to a global
/// reference. Used to decide whether `(name ...)` should be treated as a
/// macro call at all (B14, E5): a macro whose name is shadowed by a local
/// variable, parameter, or `let`/`letrec` binding in the enclosing scope
/// must compile as an ordinary call against that binding, not a macro
/// expansion -- the same rule that already applies to a bare reference to
/// the name.
fn is_shadowed_by_a_binding(name: &str, ctx: &Ctx) -> Result<bool, CompileError> {
    Ok(ctx.resolve_local(name).is_some()
        || ctx.resolve_alias(name).is_some()
        || ctx.resolve_upvalue(name)?.is_some())
}

/// Expands a macro call in place and compiles whatever it settles on.
///
/// Each round: the operands (still raw, unevaluated `Sexpr`s at this
/// point) become literal-data `Value`s via the same `sexpr_to_const` this
/// file already uses for `quote` (B14, E1 -- an operand referencing an
/// undefined name must never be evaluated, only handed over as the data
/// describing it), the macro's already-compiled body is actually run
/// against them via `eval_top_level_function`, and its return `Value`
/// becomes an `Sexpr` again via `value_to_sexpr` -- the replacement code.
/// If THAT replacement is itself a call to a (still-unshadowed) macro, the
/// same process repeats on it instead of compiling it directly (E3), up to
/// `MAX_MACRO_EXPANSION_ROUNDS` rounds before giving up with a clean error
/// rather than looping forever on a macro that always expands into
/// another macro call.
fn compile_macro_call(
    initial_op: &str,
    initial_operands: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let mut op = initial_op.to_string();
    let mut operands = initial_operands.to_vec();
    for _ in 0..MAX_MACRO_EXPANSION_ROUNDS {
        let fn_index = *comp
            .macros
            .get(&op)
            .expect("caller already confirmed this name is a registered macro");
        let args = operands
            .iter()
            .map(|operand| {
                sexpr_to_const_with_budget(operand, &mut comp.macro_conversion_budget_remaining)
                    .map(|c| crate::vm::const_to_value(&c))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (result, updated_gensym_counter, updated_step_budget) = match eval_top_level_function(
            &comp.module,
            fn_index,
            args,
            comp.macro_gensym_counter,
            comp.macro_step_budget_remaining,
        ) {
            Ok(ok) => ok,
            // qa test-design review msg #296: `exit`'s deliberate-
            // termination signal reuses this same `RuntimeError` channel
            // with an empty message (see `exit_signal`, vm.rs) -- passing
            // it through the ordinary error-wrapping below would silently
            // discard the requested exit code and produce a dangling
            // "...: runtime error: " message with nothing after the
            // colon. Calling `exit` while a macro body is being expanded
            // at COMPILE time is a distinct scenario from a running
            // program calling it (B15's own scope), rejected here with
            // its own clear diagnostic instead.
            Err(e) if e.exit_code.is_some() => {
                return Err(err(format!(
                    "macro '{op}' called exit during macro expansion, which is not supported -- exit terminates the running program, not the compiler"
                )));
            }
            Err(e) => return Err(err(format!("error while expanding macro '{op}': {e}"))),
        };
        comp.macro_gensym_counter = updated_gensym_counter;
        comp.macro_step_budget_remaining = updated_step_budget;
        let expanded = crate::vm::value_to_sexpr_with_budget(
            &result,
            &mut comp.macro_conversion_budget_remaining,
        )?;
        if let Sexpr::List(next_items) = &expanded
            && let Some(Sexpr::Symbol(next_op)) = next_items.first()
            && !is_shadowed_by_a_binding(next_op, ctx)?
            && comp.macros.contains_key(next_op)
        {
            op = next_op.clone();
            operands = next_items[1..].to_vec();
            continue;
        }
        // A one-level safety margin, not the load-bearing part of this
        // guard, mirroring `compile_quasiquote`'s own identical `depth + 1`
        // (see its doc comment): whatever `expanded` actually contains
        // gets compiled by the ordinary recursive `compile_expr` call this
        // returns, which adds its own many further levels of depth as it
        // descends into that structure -- this single increment only
        // matters for a macro whose expansion is one bare, deeply-nested
        // atom-adjacent form with no nesting of its own for `compile_expr`
        // to descend into at all, an edge case `MAX_NESTING_DEPTH`'s own
        // margin (512) comfortably absorbs regardless.
        return compile_expr(&expanded, ctx, chunk, comp, depth + 1, tail);
    }
    Err(too_many_macro_expansion_rounds())
}

fn compile_lambda(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
) -> Result<(), CompileError> {
    let formals_sexpr = items
        .get(1)
        .ok_or_else(|| err("lambda requires a parameter list"))?;
    let fn_index = compile_function(formals_sexpr, &items[2..], comp, depth, ctx, None)?;
    chunk.emit_make_function(fn_index);
    Ok(())
}

/// `let`: all binding expressions see only the enclosing scope (not each
/// other) — evaluated first against the outer `ctx`, then declared as new
/// locals only afterward. Also dispatches to [`compile_named_let`] when the
/// `(let name ((...)) ...)` shape is used.
fn compile_let(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    if let Some(Sexpr::Symbol(_)) = items.get(1) {
        return compile_named_let(items, ctx, chunk, comp, depth, tail);
    }
    let bindings_sexpr = items
        .get(1)
        .ok_or_else(|| err("let requires a bindings list"))?;
    let bindings = expect_bindings_list(bindings_sexpr)?;
    let body = &items[2..];

    let mut extended = ctx.clone();
    for (name, init) in &bindings {
        compile_expr(init, ctx, chunk, comp, depth + 1, false)?;
        chunk.emit_push_local();
        extended.declare(name.clone())?;
    }
    compile_body(body, &extended, chunk, comp, depth + 1, tail)
}

/// `let*`: each binding expression sees every binding introduced before it
/// in the same group (sequential visibility).
fn compile_let_star(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let bindings_sexpr = items
        .get(1)
        .ok_or_else(|| err("let* requires a bindings list"))?;
    let bindings = expect_bindings_list(bindings_sexpr)?;
    let body = &items[2..];

    let mut extended = ctx.clone();
    for (name, init) in &bindings {
        compile_expr(init, &extended, chunk, comp, depth + 1, false)?;
        chunk.emit_push_local();
        extended.declare(name.clone())?;
    }
    compile_body(body, &extended, chunk, comp, depth + 1, tail)
}

/// `letrec`: every binding sees every other binding, including itself —
/// enabling (mutual) self-reference. Implemented via [`Ctx::aliases`] (see
/// its doc comment) rather than real local slots, since the bound values are
/// typically lambdas that need to reference their siblings from within a
/// separately-compiled function body.
fn compile_letrec(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let bindings_sexpr = items
        .get(1)
        .ok_or_else(|| err("letrec requires a bindings list"))?;
    let bindings = expect_bindings_list(bindings_sexpr)?;
    let body = &items[2..];

    let mut extended = ctx.clone();
    let mut aliased = Vec::with_capacity(bindings.len());
    for (name, init) in &bindings {
        let alias = comp.gensym(name);
        extended = extended.with_alias(name.clone(), alias.clone());
        aliased.push((alias, init.clone()));
    }
    for (alias, init) in &aliased {
        compile_expr(init, &extended, chunk, comp, depth + 1, false)?;
        let idx = chunk.add_const(Const::Symbol(alias.clone()));
        chunk.emit_def_global(idx);
        chunk.emit_pop();
    }
    compile_body(body, &extended, chunk, comp, depth + 1, tail)
}

/// Named `let`: `(let loop ((v init) ...) body...)` desugars to a
/// `letrec`-bound self-recursive function immediately called with the
/// initial values, giving iteration without a separate top-level `define`.
fn compile_named_let(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let name = expect_symbol_name(&items[1])?;
    let bindings_sexpr = items
        .get(2)
        .ok_or_else(|| err("named let requires a bindings list"))?;
    let bindings = expect_bindings_list(bindings_sexpr)?;
    let body = &items[3..];

    let alias = comp.gensym(&name);
    let ctx_with_alias = ctx.with_alias(name.clone(), alias.clone());

    let formals_sexpr = Sexpr::List(
        bindings
            .iter()
            .map(|(n, _)| Sexpr::Symbol(n.clone()))
            .collect(),
    );
    let fn_index = compile_function(
        &formals_sexpr,
        body,
        comp,
        depth,
        &ctx_with_alias,
        Some(name.clone()),
    )?;
    chunk.emit_make_function(fn_index);
    let def_idx = chunk.add_const(Const::Symbol(alias.clone()));
    chunk.emit_def_global(def_idx);
    chunk.emit_pop();

    let callee_idx = chunk.add_const(Const::Symbol(alias));
    chunk.emit_get_global(callee_idx);
    for (_, init) in &bindings {
        compile_expr(init, ctx, chunk, comp, depth + 1, false)?;
    }
    if bindings.len() > u8::MAX as usize {
        return Err(err(format!(
            "too many bindings in named let: {}",
            bindings.len()
        )));
    }
    // This is the loop's *initial* invocation, from whatever context the
    // named-let expression itself appears in -- if that context is tail
    // position, this call inherits it (its own recursive self-calls are
    // handled independently, inside the freshly-compiled loop body above).
    if tail {
        chunk.emit_tail_call(bindings.len() as u8);
    } else {
        chunk.emit_call(bindings.len() as u8);
    }
    Ok(())
}

/// `do`: standard iteration, desugared directly into the equivalent named
/// `let` —
///   (do ((v init step)...) (test result...) command...)
///   =>
///   (let LOOP ((v init)...)
///     (if test (begin result...) (begin command... (LOOP step...))))
/// — rather than given its own compilation strategy, so it reuses
/// named-let's already-proven self-recursion mechanism (and, transitively,
/// the VM's call-depth guard and dedicated large stack) instead of
/// introducing a second, separate iteration construct with its own risk
/// surface. `LOOP` is a plain placeholder symbol here: compile_named_let
/// immediately rebinds it to a fresh gensym alias and never exposes it to
/// user code, so it can't collide with anything.
fn compile_do(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let bindings_sexpr = items
        .get(1)
        .ok_or_else(|| err("do requires a bindings list"))?;
    let bindings = expect_do_bindings(bindings_sexpr)?;

    let test_clause = match items.get(2) {
        Some(Sexpr::List(clause)) => clause,
        _ => return Err(err("do requires a (test result...) clause")),
    };
    let (test, results) = test_clause
        .split_first()
        .ok_or_else(|| err("do's test clause requires a test expression"))?;
    let commands = &items[3..];

    let loop_name = "do-loop".to_string();
    let let_bindings = Sexpr::List(
        bindings
            .iter()
            .map(|(name, init, _)| Sexpr::List(vec![Sexpr::Symbol(name.clone()), init.clone()]))
            .collect(),
    );

    let recur = Sexpr::List(
        std::iter::once(Sexpr::Symbol(loop_name.clone()))
            .chain(bindings.iter().map(|(_, _, step)| step.clone()))
            .collect(),
    );
    let else_branch = Sexpr::List(
        std::iter::once(Sexpr::Symbol("begin".to_string()))
            .chain(commands.iter().cloned())
            .chain(std::iter::once(recur))
            .collect(),
    );
    let then_branch = Sexpr::List(
        std::iter::once(Sexpr::Symbol("begin".to_string()))
            .chain(results.iter().cloned())
            .collect(),
    );
    let if_form = Sexpr::List(vec![
        Sexpr::Symbol("if".to_string()),
        test.clone(),
        then_branch,
        else_branch,
    ]);

    let named_let = vec![
        Sexpr::Symbol("let".to_string()),
        Sexpr::Symbol(loop_name),
        let_bindings,
        if_form,
    ];
    compile_named_let(&named_let, ctx, chunk, comp, depth, tail)
}

/// `set!`: mutates an existing binding in place. Resolves the target the
/// same way a read would (local slot, then alias, then plain global);
/// mutating a name that turns out not to exist as a global is a runtime
/// error (checked by the VM, since a not-yet-defined global is not
/// distinguishable from "will be defined later" at compile time).
fn compile_set(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
) -> Result<(), CompileError> {
    if items.len() != 3 {
        return Err(err("set! requires exactly a name and a value expression"));
    }
    let name = expect_symbol_name(&items[1])?;
    compile_expr(&items[2], ctx, chunk, comp, depth + 1, false)?;
    if let Some(slot) = ctx.resolve_local(&name) {
        chunk.emit_set_local(slot);
    } else if let Some(alias) = ctx.resolve_alias(&name) {
        let idx = chunk.add_const(Const::Symbol(alias.to_string()));
        chunk.emit_set_global(idx);
    } else if let Some((depth, slot)) = ctx.resolve_upvalue(&name)? {
        chunk.emit_set_upvalue(depth, slot);
    } else {
        let idx = chunk.add_const(Const::Symbol(name));
        chunk.emit_set_global(idx);
    }
    Ok(())
}

/// Short-circuiting `and`: evaluates left to right, stopping at (and
/// returning) the first falsy value; returns the last value if all are
/// truthy. `(and)` with no operands is `#t`.
fn compile_and(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let exprs = &items[1..];
    let Some((last, rest)) = exprs.split_last() else {
        let idx = chunk.add_const(Const::Bool(true));
        chunk.emit_const(idx);
        return Ok(());
    };
    let mut end_jumps = Vec::new();
    for e in rest {
        compile_expr(e, ctx, chunk, comp, depth + 1, false)?;
        chunk.emit_dup();
        end_jumps.push(chunk.emit_jump(Op::JumpIfFalse));
        chunk.emit_pop();
    }
    compile_expr(last, ctx, chunk, comp, depth + 1, tail)?;
    for j in end_jumps {
        chunk.patch_jump(j);
    }
    Ok(())
}

/// Short-circuiting `or`: evaluates left to right, stopping at (and
/// returning) the first truthy value; returns the last value if all are
/// falsy. `(or)` with no operands is `#f`.
fn compile_or(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let exprs = &items[1..];
    let Some((last, rest)) = exprs.split_last() else {
        let idx = chunk.add_const(Const::Bool(false));
        chunk.emit_const(idx);
        return Ok(());
    };
    let mut end_jumps = Vec::new();
    for e in rest {
        compile_expr(e, ctx, chunk, comp, depth + 1, false)?;
        chunk.emit_dup();
        let falsy = chunk.emit_jump(Op::JumpIfFalse);
        end_jumps.push(chunk.emit_jump(Op::Jump));
        chunk.patch_jump(falsy);
        chunk.emit_pop();
    }
    compile_expr(last, ctx, chunk, comp, depth + 1, tail)?;
    for j in end_jumps {
        chunk.patch_jump(j);
    }
    Ok(())
}

/// `when`: runs its body (as an implicit `begin`) only if the condition is
/// truthy; produces the unspecified value with no visible effect otherwise.
fn compile_when(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let condition = items
        .get(1)
        .ok_or_else(|| err("when requires a condition"))?;
    let body = &items[2..];
    compile_expr(condition, ctx, chunk, comp, depth + 1, false)?;
    let else_jump = chunk.emit_jump(Op::JumpIfFalse);
    compile_sequence(body, ctx, chunk, comp, depth + 1, tail)?;
    let end_jump = chunk.emit_jump(Op::Jump);
    chunk.patch_jump(else_jump);
    let idx = chunk.add_const(Const::Unspecified);
    chunk.emit_const(idx);
    chunk.patch_jump(end_jump);
    Ok(())
}

/// `unless`: runs its body only if the condition is falsy; unspecified with
/// no visible effect otherwise.
fn compile_unless(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let condition = items
        .get(1)
        .ok_or_else(|| err("unless requires a condition"))?;
    let body = &items[2..];
    compile_expr(condition, ctx, chunk, comp, depth + 1, false)?;
    let run_jump = chunk.emit_jump(Op::JumpIfFalse);
    let idx = chunk.add_const(Const::Unspecified);
    chunk.emit_const(idx);
    let end_jump = chunk.emit_jump(Op::Jump);
    chunk.patch_jump(run_jump);
    compile_sequence(body, ctx, chunk, comp, depth + 1, tail)?;
    chunk.patch_jump(end_jump);
    Ok(())
}

/// `cond`: tests are checked in order; the first truthy one's body runs
/// (falling back to `else` if present, otherwise unspecified). A clause of
/// the form `(test => func)` applies `func` to the test's own value instead
/// of running a separate body.
fn compile_cond(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let clauses = &items[1..];
    let mut end_jumps = Vec::new();

    for (i, clause) in clauses.iter().enumerate() {
        let Sexpr::List(clause_items) = clause else {
            return Err(err(format!("cond clause must be a list, found {clause:?}")));
        };
        let (test, rest) = clause_items
            .split_first()
            .ok_or_else(|| err("cond clause cannot be empty"))?;

        let is_else = i == clauses.len() - 1 && matches!(test, Sexpr::Symbol(s) if s == "else");
        if is_else {
            compile_sequence(rest, ctx, chunk, comp, depth + 1, tail)?;
            end_jumps.push(chunk.emit_jump(Op::Jump));
            continue;
        }

        let is_arrow = rest.len() == 2 && matches!(&rest[0], Sexpr::Symbol(s) if s == "=>");
        if is_arrow {
            let func = &rest[1];
            compile_expr(test, ctx, chunk, comp, depth + 1, false)?; // [t]
            chunk.emit_dup(); // [t, t]
            let skip = chunk.emit_jump(Op::JumpIfFalse); // pops one; falsy -> skip, leaves [t]
            compile_expr(func, ctx, chunk, comp, depth + 1, false)?; // [t, f]
            chunk.emit_swap(); // [f, t]
            if tail {
                chunk.emit_tail_call(1);
            } else {
                chunk.emit_call(1); // [result]
            }
            end_jumps.push(chunk.emit_jump(Op::Jump));
            chunk.patch_jump(skip);
            chunk.emit_pop(); // discard leftover t on the falsy path
            continue;
        }

        compile_expr(test, ctx, chunk, comp, depth + 1, false)?;
        let skip = chunk.emit_jump(Op::JumpIfFalse);
        compile_sequence(rest, ctx, chunk, comp, depth + 1, tail)?;
        end_jumps.push(chunk.emit_jump(Op::Jump));
        chunk.patch_jump(skip);
    }

    let idx = chunk.add_const(Const::Unspecified);
    chunk.emit_const(idx);
    for j in end_jumps {
        chunk.patch_jump(j);
    }
    Ok(())
}

/// `case`: evaluates the key expression once, then compares it (by value
/// equivalence, not by re-evaluating candidates as code) against each
/// clause's literal candidate list, running the first matching clause's
/// body, falling back to `else` if present, otherwise unspecified.
fn compile_case(
    items: &[Sexpr],
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    let key_expr = items
        .get(1)
        .ok_or_else(|| err("case requires a key expression"))?;
    let clauses = &items[2..];

    compile_expr(key_expr, ctx, chunk, comp, depth + 1, false)?; // [k], kept live across every clause check

    let mut end_jumps = Vec::new();
    let mut pending_next_clause: Vec<usize> = Vec::new();

    for (i, clause) in clauses.iter().enumerate() {
        for j in pending_next_clause.drain(..) {
            chunk.patch_jump(j);
        }
        let Sexpr::List(clause_items) = clause else {
            return Err(err(format!("case clause must be a list, found {clause:?}")));
        };
        let (selector, body) = clause_items
            .split_first()
            .ok_or_else(|| err("case clause cannot be empty"))?;

        let is_else = i == clauses.len() - 1 && matches!(selector, Sexpr::Symbol(s) if s == "else");
        if is_else {
            chunk.emit_pop(); // discard k, unused in the else body
            compile_sequence(body, ctx, chunk, comp, depth + 1, tail)?;
            end_jumps.push(chunk.emit_jump(Op::Jump));
            continue;
        }

        let Sexpr::List(candidates) = selector else {
            return Err(err(format!(
                "case clause selector must be a list of candidate values, found {selector:?}"
            )));
        };

        let mut found_jumps = Vec::new();
        for candidate in candidates {
            let candidate_const = sexpr_to_const(candidate)?;
            chunk.emit_dup(); // [k, k]
            let cidx = chunk.add_const(candidate_const);
            chunk.emit_const(cidx); // [k, k, c]
            chunk.emit_eqv(); // [k, bool]
            let try_next = chunk.emit_jump(Op::JumpIfFalse); // falsy -> next candidate, leaves [k]
            found_jumps.push(chunk.emit_jump(Op::Jump)); // matched -> run this clause's body, leaves [k]
            chunk.patch_jump(try_next);
        }
        // No candidate in this clause matched: move on to the next clause,
        // carrying k forward for its checks.
        pending_next_clause.push(chunk.emit_jump(Op::Jump));

        for j in found_jumps {
            chunk.patch_jump(j);
        }
        chunk.emit_pop(); // discard k, unused in the body
        compile_sequence(body, ctx, chunk, comp, depth + 1, tail)?;
        end_jumps.push(chunk.emit_jump(Op::Jump));
    }

    for j in pending_next_clause.drain(..) {
        chunk.patch_jump(j);
    }
    chunk.emit_pop(); // no clause matched and there was no else: discard k
    let idx = chunk.add_const(Const::Unspecified);
    chunk.emit_const(idx);

    for j in end_jumps {
        chunk.patch_jump(j);
    }
    Ok(())
}

fn compile_expr(
    expr: &Sexpr,
    ctx: &Ctx,
    chunk: &mut Chunk,
    comp: &mut Compilation,
    depth: usize,
    tail: bool,
) -> Result<(), CompileError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(too_deep());
    }
    match expr {
        Sexpr::Int(n) => {
            let idx = chunk.add_const(Const::Int(*n));
            chunk.emit_const(idx);
        }
        Sexpr::Float(n) => {
            let idx = chunk.add_const(Const::Float(*n));
            chunk.emit_const(idx);
        }
        Sexpr::Bool(b) => {
            let idx = chunk.add_const(Const::Bool(*b));
            chunk.emit_const(idx);
        }
        Sexpr::Char(c) => {
            let idx = chunk.add_const(Const::Char(*c));
            chunk.emit_const(idx);
        }
        Sexpr::Str(s) => {
            let idx = chunk.add_const(Const::Str(s.clone()));
            chunk.emit_const(idx);
        }
        Sexpr::Vector(items) => {
            let items = items
                .iter()
                .map(sexpr_to_const)
                .collect::<Result<Vec<_>, _>>()?;
            let idx = chunk.add_const(Const::Vector(items));
            chunk.emit_const(idx);
        }
        Sexpr::Symbol(s) => {
            if let Some(slot) = ctx.resolve_local(s) {
                chunk.emit_get_local(slot);
            } else if let Some(alias) = ctx.resolve_alias(s) {
                let idx = chunk.add_const(Const::Symbol(alias.to_string()));
                chunk.emit_get_global(idx);
            } else if let Some((depth, slot)) = ctx.resolve_upvalue(s)? {
                chunk.emit_get_upvalue(depth, slot);
            } else {
                let idx = chunk.add_const(Const::Symbol(s.clone()));
                chunk.emit_get_global(idx);
            }
        }
        Sexpr::DottedList(..) => {
            return Err(err("dotted-pair syntax is only valid in a parameter list"));
        }
        Sexpr::List(items) => {
            if let Some(Sexpr::Symbol(op)) = items.first() {
                match op.as_str() {
                    "quote" => return compile_quote(items, chunk),
                    "quasiquote" => {
                        return compile_quasiquote(items, ctx, chunk, comp, depth, tail);
                    }
                    "if" => return compile_if(items, ctx, chunk, comp, depth, tail),
                    "define" => return compile_define(items, ctx, chunk, comp, depth),
                    "define-macro" => {
                        return compile_define_macro(items, ctx, chunk, comp, depth);
                    }
                    "lambda" => return compile_lambda(items, ctx, chunk, comp, depth),
                    "begin" => {
                        return compile_sequence(&items[1..], ctx, chunk, comp, depth + 1, tail);
                    }
                    "let" => return compile_let(items, ctx, chunk, comp, depth, tail),
                    "let*" => return compile_let_star(items, ctx, chunk, comp, depth, tail),
                    "letrec" => return compile_letrec(items, ctx, chunk, comp, depth, tail),
                    "set!" => return compile_set(items, ctx, chunk, comp, depth),
                    "and" => return compile_and(items, ctx, chunk, comp, depth, tail),
                    "or" => return compile_or(items, ctx, chunk, comp, depth, tail),
                    "when" => return compile_when(items, ctx, chunk, comp, depth, tail),
                    "unless" => return compile_unless(items, ctx, chunk, comp, depth, tail),
                    "cond" => return compile_cond(items, ctx, chunk, comp, depth, tail),
                    "case" => return compile_case(items, ctx, chunk, comp, depth, tail),
                    "do" => return compile_do(items, ctx, chunk, comp, depth, tail),
                    // Reachable ONLY as a bare, top-level `,x`/`,@x` outside
                    // any quasiquote template: inside an actual template,
                    // `expand_quasiquote`'s own tag matching consumes these
                    // forms before compile_expr ever sees them (spec 3.4).
                    // Previously fell through to an ordinary call against
                    // an undefined global, failing only at runtime with a
                    // generic unbound-name error instead of a clear
                    // diagnostic (qa test-design review msg #225).
                    "unquote" => {
                        return Err(err("unquote is only valid inside a quasiquote template"));
                    }
                    "unquote-splicing" => {
                        return Err(err(
                            "unquote-splicing is only valid inside a quasiquote template",
                        ));
                    }
                    _ => {}
                }
                if comp.macros.contains_key(op.as_str()) && !is_shadowed_by_a_binding(op, ctx)? {
                    return compile_macro_call(op, &items[1..], ctx, chunk, comp, depth, tail);
                }
            }
            let (callee, args) = items
                .split_first()
                .ok_or_else(|| err("cannot call the empty list ()"))?;
            compile_expr(callee, ctx, chunk, comp, depth + 1, false)?;
            for arg in args {
                compile_expr(arg, ctx, chunk, comp, depth + 1, false)?;
            }
            if args.len() > u8::MAX as usize {
                return Err(err(format!("too many arguments in call: {}", args.len())));
            }
            if tail {
                chunk.emit_tail_call(args.len() as u8);
            } else {
                chunk.emit_call(args.len() as u8);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes just the sequence of opcodes from a chunk's code, discarding
    /// operand bytes — so tests can assert on the compiler's control-flow
    /// shape (which opcodes, in what order) without coupling to the exact
    /// operand encoding, which is bytecode.rs's concern (see its own
    /// round-trip tests) and not the compiler's observable behavior.
    fn opcode_sequence(code: &[u8]) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut i = 0;
        while i < code.len() {
            let op = code[i];
            i += 1;
            let decoded = if op == Op::Const as u8 {
                i += 4;
                Op::Const
            } else if op == Op::GetGlobal as u8 {
                i += 4;
                Op::GetGlobal
            } else if op == Op::DefGlobal as u8 {
                i += 4;
                Op::DefGlobal
            } else if op == Op::GetLocal as u8 {
                i += 1;
                Op::GetLocal
            } else if op == Op::SetLocal as u8 {
                i += 1;
                Op::SetLocal
            } else if op == Op::SetGlobal as u8 {
                i += 4;
                Op::SetGlobal
            } else if op == Op::PushLocal as u8 {
                Op::PushLocal
            } else if op == Op::Dup as u8 {
                Op::Dup
            } else if op == Op::Swap as u8 {
                Op::Swap
            } else if op == Op::Eqv as u8 {
                Op::Eqv
            } else if op == Op::MakeFunction as u8 {
                i += 4;
                Op::MakeFunction
            } else if op == Op::Jump as u8 {
                i += 4;
                Op::Jump
            } else if op == Op::JumpIfFalse as u8 {
                i += 4;
                Op::JumpIfFalse
            } else if op == Op::Call as u8 {
                i += 1;
                Op::Call
            } else if op == Op::TailCall as u8 {
                i += 1;
                Op::TailCall
            } else if op == Op::Pop as u8 {
                Op::Pop
            } else if op == Op::Return as u8 {
                Op::Return
            } else if op == Op::Halt as u8 {
                Op::Halt
            } else {
                panic!("opcode_sequence: unrecognised opcode byte {op}");
            };
            ops.push(decoded);
        }
        ops
    }

    fn sym(s: &str) -> Sexpr {
        Sexpr::Symbol(s.to_string())
    }

    fn list(items: Vec<Sexpr>) -> Sexpr {
        Sexpr::List(items)
    }

    fn entry_of(module: &Module) -> &Chunk {
        &module.functions[module.entry_index as usize]
    }

    #[test]
    fn compiles_an_int_literal_to_const_then_pop_then_halt() {
        let module = compile_program(&[Sexpr::Int(5)]).unwrap();
        let entry = entry_of(&module);
        assert_eq!(entry.constants, vec![Const::Int(5)]);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn compiles_a_call_expression_as_callee_then_args_then_call() {
        let program = [list(vec![sym("+"), Sexpr::Int(1), Sexpr::Int(2)])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            entry.constants,
            vec![Const::Symbol("+".to_string()), Const::Int(1), Const::Int(2)]
        );
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::GetGlobal, // callee
                Op::Const,     // arg 1
                Op::Const,     // arg 2
                Op::Call,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_nested_calls_depth_first() {
        let program = [list(vec![
            sym("display"),
            list(vec![sym("+"), Sexpr::Int(1), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(entry.constants.len(), 4);
        assert!(entry.code.contains(&(Op::Call as u8)));
    }

    #[test]
    fn compiles_each_top_level_form_followed_by_its_own_pop() {
        let program = [Sexpr::Int(1), Sexpr::Int(2)];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        let pop_count = entry.code.iter().filter(|&&b| b == Op::Pop as u8).count();
        assert_eq!(pop_count, 2);
        assert_eq!(*entry.code.last().unwrap(), Op::Halt as u8);
    }

    #[test]
    fn compiles_a_bare_symbol_as_a_global_lookup() {
        let module = compile_program(&[sym("display")]).unwrap();
        let entry = entry_of(&module);
        assert_eq!(entry.constants, vec![Const::Symbol("display".to_string())]);
        assert_eq!(entry.code[0], Op::GetGlobal as u8);
    }

    #[test]
    fn compiles_string_and_bool_literals_as_constants() {
        let program = [Sexpr::Str("hi".to_string()), Sexpr::Bool(true)];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            entry.constants,
            vec![Const::Str("hi".to_string()), Const::Bool(true)]
        );
    }

    #[test]
    fn rejects_calling_the_empty_list() {
        let program = [list(vec![])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compile_error_display_includes_the_underlying_message() {
        let e = CompileError {
            message: "something specific went wrong".to_string(),
        };
        assert_eq!(
            e.to_string(),
            "compile error: something specific went wrong"
        );
    }

    fn call_with_n_args(n: usize) -> Sexpr {
        let mut items = vec![sym("+")];
        items.extend((0..n).map(|_| Sexpr::Int(1)));
        list(items)
    }

    #[test]
    fn accepts_a_call_with_exactly_the_maximum_representable_argument_count() {
        let program = [call_with_n_args(u8::MAX as usize)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn rejects_a_call_with_one_more_than_the_maximum_representable_argument_count() {
        let program = [call_with_n_args(u8::MAX as usize + 1)];
        assert!(compile_program(&program).is_err());
    }

    fn nested_call(depth: usize) -> Sexpr {
        let mut expr = Sexpr::Int(1);
        for _ in 0..depth {
            expr = list(vec![sym("+"), expr]);
        }
        expr
    }

    /// Builds a program by substituting a maximally-deep nested expression
    /// into the position `build` chooses, and asserts that compiling it
    /// fails — proving the nesting-depth limit is enforced at that specific
    /// position (i.e. that position's `depth + 1` really propagates instead
    /// of silently resetting to 0).
    fn assert_propagates_depth(build: impl FnOnce(Sexpr) -> Sexpr) {
        let program = [build(nested_call(MAX_NESTING_DEPTH))];
        assert!(
            compile_program(&program).is_err(),
            "expected the nesting-depth limit to propagate through this position"
        );
    }

    #[test]
    fn accepts_expression_nesting_comfortably_under_the_configured_maximum() {
        let program = [nested_call(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn accepts_expression_nesting_of_exactly_the_configured_maximum_depth() {
        let program = [nested_call(MAX_NESTING_DEPTH)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn rejects_expression_nesting_of_one_more_than_the_configured_maximum_depth() {
        let program = [nested_call(MAX_NESTING_DEPTH + 1)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("nesting") && error.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            error.message
        );
    }

    fn nested_in_callee_position(depth: usize) -> Sexpr {
        let mut expr = sym("+");
        for _ in 0..depth {
            expr = list(vec![expr]);
        }
        expr
    }

    #[test]
    fn accepts_callee_position_nesting_comfortably_under_the_configured_maximum() {
        let program = [nested_in_callee_position(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn accepts_callee_position_nesting_of_exactly_the_configured_maximum_depth() {
        let program = [nested_in_callee_position(MAX_NESTING_DEPTH)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn rejects_callee_position_nesting_of_one_more_than_the_configured_maximum_depth() {
        let program = [nested_in_callee_position(MAX_NESTING_DEPTH + 1)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("nesting") && error.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            error.message
        );
    }

    #[test]
    fn nesting_depth_inside_a_function_body_starts_one_deeper_than_top_level() {
        let body = nested_call(MAX_NESTING_DEPTH);
        let program = [list(vec![sym("lambda"), list(vec![]), body])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn if_condition_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("if"),
            nested_call(MAX_NESTING_DEPTH),
            Sexpr::Int(1),
            Sexpr::Int(2),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn if_then_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(true),
            nested_call(MAX_NESTING_DEPTH),
            Sexpr::Int(2),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn if_else_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(false),
            Sexpr::Int(1),
            nested_call(MAX_NESTING_DEPTH),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn define_value_position_propagates_nesting_depth() {
        let program = [list(vec![
            sym("define"),
            sym("x"),
            nested_call(MAX_NESTING_DEPTH),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn begin_propagates_nesting_depth_to_its_expressions() {
        let program = [list(vec![sym("begin"), nested_call(MAX_NESTING_DEPTH)])];
        assert!(compile_program(&program).is_err());
    }

    // --- B2 special forms ---

    #[test]
    fn compiles_quote_as_a_constant_push_with_no_evaluation() {
        let program = [list(vec![
            sym("quote"),
            list(vec![sym("+"), Sexpr::Int(1), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::Pop, Op::Halt]
        );
        assert_eq!(
            entry.constants,
            vec![Const::List(vec![
                Const::Symbol("+".to_string()),
                Const::Int(1),
                Const::Int(2),
            ])]
        );
    }

    #[test]
    fn compiles_if_with_else_as_condition_then_conditional_jump_then_both_branches() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(true),
            Sexpr::Int(1),
            Sexpr::Int(2),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::Const,       // condition
                Op::JumpIfFalse, // to else
                Op::Const,       // then
                Op::Jump,        // to end
                Op::Const,       // else
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_if_without_else_to_push_unspecified_on_the_false_path() {
        let program = [list(vec![sym("if"), Sexpr::Bool(false), Sexpr::Int(1)])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert!(entry.constants.contains(&Const::Unspecified));
    }

    #[test]
    fn rejects_if_with_too_few_arguments() {
        let program = [list(vec![sym("if"), Sexpr::Bool(true)])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_if_with_too_many_arguments() {
        let program = [list(vec![
            sym("if"),
            Sexpr::Bool(true),
            Sexpr::Int(1),
            Sexpr::Int(2),
            Sexpr::Int(3),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compiles_a_plain_define_as_value_then_def_global() {
        let program = [list(vec![sym("define"), sym("x"), Sexpr::Int(42)])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::DefGlobal, Op::Pop, Op::Halt]
        );
        assert!(entry.constants.contains(&Const::Symbol("x".to_string())));
    }

    #[test]
    fn compiles_a_fixed_arity_function_define() {
        // (define (add a b) (+ a b))
        let program = [list(vec![
            sym("define"),
            list(vec![sym("add"), sym("a"), sym("b")]),
            list(vec![sym("+"), sym("a"), sym("b")]),
        ])];
        let module = compile_program(&program).unwrap();
        assert_eq!(module.functions.len(), 2);
        let fn_chunk = &module.functions[0];
        assert_eq!(fn_chunk.arity, 2);
        assert!(!fn_chunk.has_rest);
        // (+ a b) is add's own tail expression, so it's a TailCall (B6).
        assert_eq!(
            opcode_sequence(&fn_chunk.code),
            vec![
                Op::GetGlobal,
                Op::GetLocal,
                Op::GetLocal,
                Op::TailCall,
                Op::Return
            ]
        );
    }

    #[test]
    fn compiles_a_fixed_plus_rest_function_define() {
        // (define (f a b . rest) rest)
        let program = [list(vec![
            sym("define"),
            Sexpr::DottedList(vec![sym("f"), sym("a"), sym("b")], Box::new(sym("rest"))),
            sym("rest"),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        assert_eq!(fn_chunk.arity, 2);
        assert!(fn_chunk.has_rest);
        assert_eq!(
            opcode_sequence(&fn_chunk.code),
            vec![Op::GetLocal, Op::Return]
        );
    }

    #[test]
    fn compiles_an_all_rest_function_define() {
        // (define (f . args) args)
        let program = [list(vec![
            sym("define"),
            Sexpr::DottedList(vec![sym("f")], Box::new(sym("args"))),
            sym("args"),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        assert_eq!(fn_chunk.arity, 0);
        assert!(fn_chunk.has_rest);
    }

    #[test]
    fn compiles_lambda_to_make_function_referencing_a_new_chunk() {
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("x")]),
            list(vec![sym("+"), sym("x"), Sexpr::Int(1)]),
        ])];
        let module = compile_program(&program).unwrap();
        assert_eq!(module.functions.len(), 2);
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::MakeFunction, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn compiles_begin_running_each_expression_and_keeping_only_the_last_value() {
        let program = [list(vec![
            sym("begin"),
            Sexpr::Int(1),
            Sexpr::Int(2),
            Sexpr::Int(3),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        let pop_count = entry.code.iter().filter(|&&b| b == Op::Pop as u8).count();
        assert_eq!(pop_count, 3);
    }

    #[test]
    fn rejects_a_lambda_with_a_malformed_parameter_list() {
        let program = [list(vec![sym("lambda"), Sexpr::Int(5), Sexpr::Int(1)])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_dotted_pair_syntax_outside_a_parameter_list() {
        let program = [Sexpr::DottedList(vec![sym("a")], Box::new(sym("b")))];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compiles_a_quoted_dotted_pair_into_a_pair_constant() {
        let program = [list(vec![
            sym("quote"),
            Sexpr::DottedList(vec![sym("a")], Box::new(sym("b"))),
        ])];
        let module = compile_program(&program).expect("quoting a dotted pair should compile");
        assert_eq!(
            module.functions[module.entry_index as usize].constants,
            vec![Const::Pair(
                Box::new(Const::Symbol("a".to_string())),
                Box::new(Const::Symbol("b".to_string()))
            )]
        );
    }

    // --- B3: local bindings, mutation, conditional/sequencing forms ---

    fn binding(name: &str, init: Sexpr) -> Sexpr {
        list(vec![sym(name), init])
    }

    #[test]
    fn compiles_let_bindings_evaluated_against_the_outer_scope_then_pushed_as_locals() {
        // (let ((x 1) (y 2)) (+ x y))
        let program = [list(vec![
            sym("let"),
            list(vec![
                binding("x", Sexpr::Int(1)),
                binding("y", Sexpr::Int(2)),
            ]),
            list(vec![sym("+"), sym("x"), sym("y")]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::Const,     // 1
                Op::PushLocal, // x
                Op::Const,     // 2
                Op::PushLocal, // y
                Op::GetGlobal, // +
                Op::GetLocal,  // x
                Op::GetLocal,  // y
                Op::Call,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_let_star_seeing_earlier_bindings_via_get_local() {
        // (let* ((x 1) (y (+ x 1))) y)
        let program = [list(vec![
            sym("let*"),
            list(vec![
                binding("x", Sexpr::Int(1)),
                binding("y", list(vec![sym("+"), sym("x"), Sexpr::Int(1)])),
            ]),
            sym("y"),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        // y's init `(+ x 1)` must reference x via GET_LOCAL, not GET_GLOBAL.
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::Const,     // 1
                Op::PushLocal, // x
                Op::GetGlobal, // +
                Op::GetLocal,  // x
                Op::Const,     // 1
                Op::Call,
                Op::PushLocal, // y
                Op::GetLocal,  // y (the body)
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_letrec_binding_via_a_global_alias_not_a_local_slot() {
        // (letrec ((f (lambda (n) (f n)))) f) — f's own body must reference
        // itself via GET_GLOBAL(alias), since it's a separate chunk with no
        // access to the letrec's locals.
        let program = [list(vec![
            sym("letrec"),
            list(vec![binding(
                "f",
                list(vec![
                    sym("lambda"),
                    list(vec![sym("n")]),
                    list(vec![sym("f"), sym("n")]),
                ]),
            )]),
            sym("f"),
        ])];
        let module = compile_program(&program).unwrap();
        assert_eq!(module.functions.len(), 2); // f's lambda body + entry
        let f_chunk = &module.functions[0];
        // f's body calls itself in tail position: GET_GLOBAL(alias),
        // GET_LOCAL(n), TAIL_CALL, RETURN (B6).
        assert_eq!(
            opcode_sequence(&f_chunk.code),
            vec![Op::GetGlobal, Op::GetLocal, Op::TailCall, Op::Return]
        );
        let entry = entry_of(&module);
        // the letrec body references f via the SAME alias (GET_GLOBAL, not GET_LOCAL)
        assert!(opcode_sequence(&entry.code).contains(&Op::GetGlobal));
        assert!(!opcode_sequence(&entry.code).contains(&Op::GetLocal));
    }

    #[test]
    fn compiles_named_let_as_a_letrec_bound_function_called_immediately() {
        // (let loop ((i 0)) i)
        let program = [list(vec![
            sym("let"),
            sym("loop"),
            list(vec![binding("i", Sexpr::Int(0))]),
            sym("i"),
        ])];
        let module = compile_program(&program).unwrap();
        assert_eq!(module.functions.len(), 2); // loop's body + entry
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::MakeFunction,
                Op::DefGlobal,
                Op::Pop,
                Op::GetGlobal, // the immediate call to loop
                Op::Const,     // initial value 0
                Op::Call,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_do_as_a_self_recursive_named_let_desugaring() {
        // (do ((i 0 (+ i 1))) ((= i 3) i))
        let program = [list(vec![
            sym("do"),
            list(vec![list(vec![
                sym("i"),
                Sexpr::Int(0),
                list(vec![sym("+"), sym("i"), Sexpr::Int(1)]),
            ])]),
            list(vec![
                list(vec![sym("="), sym("i"), Sexpr::Int(3)]),
                sym("i"),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        // Same shape as named-let's own test: a MakeFunction-bound loop
        // called immediately, proving `do` reused that mechanism rather
        // than introducing a separate one.
        assert_eq!(module.functions.len(), 2);
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::MakeFunction,
                Op::DefGlobal,
                Op::Pop,
                Op::GetGlobal, // the immediate call to the loop function
                Op::Const,     // initial value 0
                Op::Call,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn do_with_an_omitted_step_defaults_to_re_binding_the_variable_to_itself() {
        // (do ((i 0)) (#t i)) -- no step given for i.
        let program = [list(vec![
            sym("do"),
            list(vec![list(vec![sym("i"), Sexpr::Int(0)])]),
            list(vec![Sexpr::Bool(true), sym("i")]),
        ])];
        // Compiles without error; the desugared recursive call `(loop i)`
        // resolves `i` as a local reference, not an unbound global.
        let module = compile_program(&program).unwrap();
        let loop_fn = &module.functions[0];
        assert!(opcode_sequence(&loop_fn.code).contains(&Op::GetLocal));
    }

    #[test]
    fn a_do_binding_with_neither_two_nor_three_elements_is_a_compile_error() {
        // Guards expect_do_bindings' two arity-checked match arms: a
        // single-element binding matches neither the (name init step) nor
        // the (name init) shape and must be rejected, not silently
        // accepted by an over-loose guard.
        let program = [list(vec![
            sym("do"),
            list(vec![list(vec![sym("i")])]),
            list(vec![Sexpr::Bool(true), sym("i")]),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compiles_internal_defines_at_the_start_of_a_body_as_a_letrec_group() {
        // (lambda () (define (a) 1) (define (b) (a)) (b))
        let program = [list(vec![
            sym("lambda"),
            list(vec![]),
            list(vec![sym("define"), list(vec![sym("a")]), Sexpr::Int(1)]),
            list(vec![
                sym("define"),
                list(vec![sym("b")]),
                list(vec![sym("a")]),
            ]),
            list(vec![sym("b")]),
        ])];
        let module = compile_program(&program).unwrap();
        // a's chunk, b's chunk, the lambda's own chunk, entry = 4 functions
        assert_eq!(module.functions.len(), 4);
    }

    #[test]
    fn compiles_set_on_a_local_to_set_local() {
        // (lambda (x) (set! x 2))
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("x")]),
            list(vec![sym("set!"), sym("x"), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        assert_eq!(
            opcode_sequence(&fn_chunk.code),
            vec![Op::Const, Op::SetLocal, Op::Return]
        );
    }

    #[test]
    fn compiles_set_on_a_global_to_set_global() {
        let program = [list(vec![sym("set!"), sym("x"), Sexpr::Int(2)])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::Const, Op::SetGlobal, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn rejects_set_with_the_wrong_number_of_arguments() {
        let program = [list(vec![sym("set!"), sym("x")])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compiles_and_with_a_dup_and_conditional_jump_per_non_last_operand() {
        let program = [list(vec![
            sym("and"),
            Sexpr::Int(1),
            Sexpr::Int(2),
            Sexpr::Int(3),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::Const,
                Op::Dup,
                Op::JumpIfFalse,
                Op::Pop,
                Op::Const,
                Op::Dup,
                Op::JumpIfFalse,
                Op::Pop,
                Op::Const,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn empty_and_is_true() {
        let program = [list(vec![sym("and")])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(entry.constants, vec![Const::Bool(true)]);
    }

    #[test]
    fn empty_or_is_false() {
        let program = [list(vec![sym("or")])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(entry.constants, vec![Const::Bool(false)]);
    }

    #[test]
    fn compiles_when_as_a_conditional_with_unspecified_on_the_false_path() {
        let program = [list(vec![sym("when"), Sexpr::Bool(true), Sexpr::Int(1)])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert!(entry.constants.contains(&Const::Unspecified));
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![
                Op::Const,
                Op::JumpIfFalse,
                Op::Const,
                Op::Jump,
                Op::Const,
                Op::Pop,
                Op::Halt,
            ]
        );
    }

    #[test]
    fn compiles_unless_as_a_conditional_with_unspecified_on_the_true_path() {
        let program = [list(vec![sym("unless"), Sexpr::Bool(false), Sexpr::Int(1)])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert!(entry.constants.contains(&Const::Unspecified));
    }

    #[test]
    fn compiles_cond_with_an_else_fallback() {
        let program = [list(vec![
            sym("cond"),
            list(vec![Sexpr::Bool(false), Sexpr::Int(1)]),
            list(vec![sym("else"), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        assert!(compile_program(&[Sexpr::Int(0)]).is_ok()); // sanity: unrelated program still fine
        let entry = entry_of(&module);
        assert!(opcode_sequence(&entry.code).contains(&Op::JumpIfFalse));
    }

    #[test]
    fn compiles_cond_arrow_variant_with_dup_swap_and_call() {
        let program = [list(vec![
            sym("cond"),
            list(vec![Sexpr::Int(5), sym("=>"), sym("display")]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        let ops = opcode_sequence(&entry.code);
        assert!(ops.contains(&Op::Dup));
        assert!(ops.contains(&Op::Swap));
        assert!(ops.contains(&Op::Call));
    }

    #[test]
    fn rejects_a_cond_clause_that_is_not_a_list() {
        let program = [list(vec![sym("cond"), Sexpr::Int(1)])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn compiles_case_comparing_the_key_against_candidate_groups() {
        let program = [list(vec![
            sym("case"),
            Sexpr::Int(2),
            list(vec![
                list(vec![Sexpr::Int(1), Sexpr::Int(2)]),
                sym("true-branch"),
            ]),
            list(vec![sym("else"), sym("else-branch")]),
        ])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        let ops = opcode_sequence(&entry.code);
        assert!(ops.contains(&Op::Eqv));
        assert!(ops.contains(&Op::Dup));
    }

    #[test]
    fn rejects_a_case_clause_selector_that_is_not_a_list() {
        let program = [list(vec![
            sym("case"),
            Sexpr::Int(1),
            list(vec![Sexpr::Int(1), sym("body")]),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_a_malformed_let_binding() {
        let program = [list(vec![
            sym("let"),
            list(vec![Sexpr::Int(1)]),
            Sexpr::Int(1),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn rejects_a_let_binding_list_with_the_wrong_number_of_elements() {
        // (x) has only a name, no init expression.
        let program = [list(vec![
            sym("let"),
            list(vec![list(vec![sym("x")])]),
            Sexpr::Int(1),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn gensym_produces_distinct_names_across_calls_even_with_the_same_hint() {
        let mut comp = Compilation::new();
        let a = comp.gensym("x");
        let b = comp.gensym("x");
        assert_ne!(a, b);
    }

    #[test]
    fn two_sibling_internal_definitions_with_distinct_names_do_not_collide() {
        // If gensym ever produced the same alias for both, six's DEF_GLOBAL
        // would clobber double's, and (+ (double 5) (six 5)) would use the
        // same underlying function for both calls (10 + 10 = 20, not 40).
        let src = "(define (f) \
                     (define (double x) (* x 2)) \
                     (define (six x) (* x 6)) \
                     (+ (double 5) (six 5))) \
                   (display (f))";
        let forms = crate::reader::read_program(src).unwrap();
        let module = compile_program(&forms).unwrap();
        let mut out = Vec::new();
        crate::vm::run(&module, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "40");
    }

    #[test]
    fn an_else_symbol_is_only_special_as_the_last_cond_clause() {
        // "else" appearing in a NON-last clause must be compiled as an
        // ordinary (here: unbound) test expression, not recognised as the
        // else-fallback — proven by it failing at runtime instead of always
        // matching.
        let program = [list(vec![
            sym("cond"),
            list(vec![sym("else"), Sexpr::Int(1)]),
            list(vec![Sexpr::Bool(true), Sexpr::Int(2)]),
        ])];
        let module = compile_program(&program).unwrap();
        let mut out = Vec::new();
        assert!(crate::vm::run(&module, &mut out).is_err());
    }

    #[test]
    fn an_else_symbol_is_only_special_as_the_last_case_clause() {
        let program = [list(vec![
            sym("case"),
            Sexpr::Int(1),
            list(vec![sym("else"), Sexpr::Int(1)]),
            list(vec![list(vec![Sexpr::Int(1)]), Sexpr::Int(2)]),
        ])];
        // "else" as a selector in a non-last position must be parsed as a
        // (malformed) candidate list, not the else-fallback.
        assert!(compile_program(&program).is_err());
    }

    fn named_let_with_n_bindings(n: usize) -> Sexpr {
        let bindings = list(
            (0..n)
                .map(|i| binding(&format!("v{i}"), Sexpr::Int(0)))
                .collect(),
        );
        list(vec![sym("let"), sym("loop"), bindings, Sexpr::Int(0)])
    }

    #[test]
    fn accepts_a_named_let_with_exactly_the_maximum_representable_binding_count() {
        assert!(compile_program(&[named_let_with_n_bindings(u8::MAX as usize)]).is_ok());
    }

    #[test]
    fn rejects_a_named_let_with_one_more_than_the_maximum_representable_binding_count() {
        assert!(compile_program(&[named_let_with_n_bindings(u8::MAX as usize + 1)]).is_err());
    }

    #[test]
    fn internal_define_init_position_propagates_nesting_depth() {
        // Calibrated, not just MAX_NESTING_DEPTH: this position is already
        // 2 levels deep from top level by the time it's reached (lambda's
        // own +1, then compile_body's own +1 for the define's init), so a
        // generically-deep nested_call would exceed the limit either way and
        // fail to discriminate a missing +1 here from a correct one. Using
        // MAX_NESTING_DEPTH - 1 means: correct code (starts at depth 2)
        // reaches MAX_NESTING_DEPTH + 1 and errors; code missing this one
        // +1 (starts at depth 1) reaches exactly MAX_NESTING_DEPTH and
        // succeeds — so only the correct code errors here.
        let program = [list(vec![
            sym("lambda"),
            list(vec![]),
            list(vec![
                sym("define"),
                sym("x"),
                nested_call(MAX_NESTING_DEPTH - 1),
            ]),
            sym("x"),
        ])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn internal_defines_do_not_leak_into_the_outer_global_namespace() {
        // If internal defines were bound under their literal names instead
        // of a gensym'd alias, f's internal "helper" would permanently
        // overwrite the OUTER global "helper" (999) with its own function
        // value once f is called.
        let src = "(define helper 999) \
                   (define (f) (define (helper) 1) (helper)) \
                   (display (f)) (newline) (display helper)";
        let forms = crate::reader::read_program(src).unwrap();
        let module = compile_program(&forms).unwrap();
        let mut out = Vec::new();
        crate::vm::run(&module, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "1\n999");
    }

    #[test]
    fn a_two_expression_cond_body_is_not_mistaken_for_the_arrow_variant() {
        // A regular clause `(test body1 body2)` also happens to have
        // rest.len() == 2, same as the `=>` shape `(test => func)` — the
        // arrow detection must also check that rest[0] is literally `=>`,
        // not just that there are two trailing elements.
        let src = "(display (cond (#t 1 2)))";
        let forms = crate::reader::read_program(src).unwrap();
        let module = compile_program(&forms).unwrap();
        let mut out = Vec::new();
        crate::vm::run(&module, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "2");
    }

    #[test]
    fn let_binding_init_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("let"),
                list(vec![binding("x", deep)]),
                Sexpr::Int(1),
            ])
        });
    }

    #[test]
    fn let_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("let"),
                list(vec![binding("x", Sexpr::Int(1))]),
                deep,
            ])
        });
    }

    #[test]
    fn let_star_binding_init_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("let*"),
                list(vec![binding("x", deep)]),
                Sexpr::Int(1),
            ])
        });
    }

    #[test]
    fn let_star_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("let*"),
                list(vec![binding("x", Sexpr::Int(1))]),
                deep,
            ])
        });
    }

    #[test]
    fn letrec_binding_init_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("letrec"),
                list(vec![binding("x", deep)]),
                Sexpr::Int(1),
            ])
        });
    }

    #[test]
    fn letrec_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("letrec"),
                list(vec![binding("x", Sexpr::Int(1))]),
                deep,
            ])
        });
    }

    #[test]
    fn named_let_binding_init_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("let"),
                sym("loop"),
                list(vec![binding("x", deep)]),
                sym("x"),
            ])
        });
    }

    #[test]
    fn set_value_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("set!"), sym("x"), deep]));
    }

    #[test]
    fn and_non_last_operand_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("and"), deep, Sexpr::Int(1)]));
    }

    #[test]
    fn and_last_operand_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("and"), Sexpr::Int(1), deep]));
    }

    #[test]
    fn or_non_last_operand_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("or"), deep, Sexpr::Int(1)]));
    }

    #[test]
    fn or_last_operand_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("or"), Sexpr::Int(1), deep]));
    }

    #[test]
    fn when_condition_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("when"), deep, Sexpr::Int(1)]));
    }

    #[test]
    fn when_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("when"), Sexpr::Bool(true), deep]));
    }

    #[test]
    fn unless_condition_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("unless"), deep, Sexpr::Int(1)]));
    }

    #[test]
    fn unless_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("unless"), Sexpr::Bool(false), deep]));
    }

    #[test]
    fn cond_else_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("cond"), list(vec![sym("else"), deep])]));
    }

    #[test]
    fn cond_arrow_test_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("cond"),
                list(vec![deep, sym("=>"), sym("display")]),
            ])
        });
    }

    #[test]
    fn cond_arrow_func_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("cond"),
                list(vec![Sexpr::Bool(true), sym("=>"), deep]),
            ])
        });
    }

    #[test]
    fn cond_regular_test_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| list(vec![sym("cond"), list(vec![deep, Sexpr::Int(1)])]));
    }

    #[test]
    fn cond_regular_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![sym("cond"), list(vec![Sexpr::Bool(true), deep])])
        });
    }

    #[test]
    fn case_key_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("case"),
                deep,
                list(vec![list(vec![Sexpr::Int(1)]), Sexpr::Int(1)]),
            ])
        });
    }

    #[test]
    fn case_clause_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("case"),
                Sexpr::Int(1),
                list(vec![list(vec![Sexpr::Int(1)]), deep]),
            ])
        });
    }

    #[test]
    fn case_else_body_position_propagates_nesting_depth() {
        assert_propagates_depth(|deep| {
            list(vec![
                sym("case"),
                Sexpr::Int(1),
                list(vec![sym("else"), deep]),
            ])
        });
    }

    #[test]
    fn capturing_a_variable_through_more_than_255_nested_function_levels_is_a_compile_error() {
        // qa test-design review (msg #101): the resolve_upvalue depth->255
        // overflow fix had tests at resolve_env/upvalue_cell (the adjacent
        // runtime layer) but none here, at compile_program itself -- the
        // layer where the original bug actually lived (a >255-deep capture
        // silently resolving to the wrong value via a same-named global).
        // Mirrors MAX_NESTING_DEPTH's own established boundary-test pattern
        // in this same file.
        let program = [nested_capture(300)];
        let err = compile_program(&program).unwrap_err();
        assert!(
            err.message.contains('x') && err.message.contains("too many levels"),
            "expected the depth-overflow error naming the captured variable, got: {}",
            err.message
        );
    }

    #[test]
    fn capturing_a_variable_through_exactly_255_nested_function_levels_still_succeeds() {
        // Companion to the test above (qa test-design review, msg #112):
        // the depth-255 overflow guard must reject only what's genuinely
        // too deep to encode, not the boundary value itself. 255 is the
        // deepest capture this bytecode format's u8 depth operand can
        // represent, so it must still compile successfully.
        let program = [nested_capture(255)];
        assert!(compile_program(&program).is_ok());
    }

    /// (lambda (x) (lambda () (lambda () ... x ...)))  -- `depth` levels of
    /// parameterless lambdas sit between x's binding function and the
    /// innermost body that references it, so x is an upvalue exactly
    /// `depth` enclosing-function levels away. Shared by the two
    /// capture-depth boundary tests above.
    fn nested_capture(depth: usize) -> Sexpr {
        let mut body = sym("x");
        for _ in 0..depth {
            body = list(vec![sym("lambda"), list(vec![]), body]);
        }
        list(vec![sym("lambda"), list(vec![sym("x")]), body])
    }

    // B6: tail-call analysis. These tests pin down exactly which call sites
    // become Op::TailCall (reusing the current native frame) versus
    // Op::Call (a genuine, stack-consuming call) per spec 3.8's enumerated
    // tail positions.

    #[test]
    fn top_level_call_is_never_a_tail_call() {
        // Every top-level form is followed by its own POP (see
        // compile_program), so none of them are ever in tail position, even
        // though nothing textually follows the last one.
        let program = [list(vec![sym("f")])];
        let module = compile_program(&program).unwrap();
        let entry = entry_of(&module);
        assert_eq!(
            opcode_sequence(&entry.code),
            vec![Op::GetGlobal, Op::Call, Op::Pop, Op::Halt]
        );
    }

    #[test]
    fn if_condition_call_is_never_a_tail_call() {
        // (lambda (n) (if (p n) 1 2)) -- the condition is never tail, even
        // though the whole `if` is this function's own tail expression.
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![
                sym("if"),
                list(vec![sym("p"), sym("n")]),
                Sexpr::Int(1),
                Sexpr::Int(2),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert!(ops.contains(&Op::Call));
        assert!(!ops.contains(&Op::TailCall));
    }

    #[test]
    fn if_branches_in_tail_position_emit_tail_call() {
        // (lambda (n) (if n (f n) (g n))) -- the whole `if` is this
        // function's own tail expression, so both branches' calls are
        // tail calls.
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![
                sym("if"),
                sym("n"),
                list(vec![sym("f"), sym("n")]),
                list(vec![sym("g"), sym("n")]),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert_eq!(
            ops.iter().filter(|op| **op == Op::TailCall).count(),
            2,
            "both if-branches' calls should be tail calls: {ops:?}"
        );
        assert!(!ops.contains(&Op::Call));
    }

    #[test]
    fn argument_position_call_is_never_a_tail_call() {
        // (lambda (n) (h (f n))) -- the outer call to h IS this function's
        // tail expression, but the inner (f n), being an argument, never is.
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![sym("h"), list(vec![sym("f"), sym("n")])]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert_eq!(
            ops.iter().filter(|op| **op == Op::Call).count(),
            1,
            "inner (f n), an argument, must be a plain Call: {ops:?}"
        );
        assert_eq!(
            ops.iter().filter(|op| **op == Op::TailCall).count(),
            1,
            "outer (h ...), the function's own tail expression, must be a TailCall: {ops:?}"
        );
    }

    #[test]
    fn let_binding_init_call_is_never_tail_but_the_bodys_last_expr_is() {
        // (lambda (n) (let ((x (g n))) (f x))) -- the binding init is never
        // tail; the body's last expression inherits the let's own tail
        // status, which here is the whole function's tail position.
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![
                sym("let"),
                list(vec![binding("x", list(vec![sym("g"), sym("n")]))]),
                list(vec![sym("f"), sym("x")]),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert_eq!(
            ops.iter().filter(|op| **op == Op::Call).count(),
            1,
            "the binding init (g n) must be a plain Call: {ops:?}"
        );
        assert_eq!(
            ops.iter().filter(|op| **op == Op::TailCall).count(),
            1,
            "the body's last expression (f x) must be a TailCall: {ops:?}"
        );
    }

    #[test]
    fn named_let_loop_bodys_self_call_is_a_tail_call_regardless_of_the_named_lets_own_context() {
        // (let loop ((i 0)) (loop i)) at the top level: the loop body's own
        // self-call is ALWAYS a tail call (compile_function always starts a
        // fresh tail context for a function's own body), even though the
        // named-let expression itself sits in non-tail (top-level) context.
        let program = [list(vec![
            sym("let"),
            sym("loop"),
            list(vec![binding("i", Sexpr::Int(0))]),
            list(vec![sym("loop"), sym("i")]),
        ])];
        let module = compile_program(&program).unwrap();
        assert_eq!(module.functions.len(), 2); // loop's body + entry
        let loop_chunk = &module.functions[0];
        assert_eq!(
            opcode_sequence(&loop_chunk.code),
            vec![Op::GetGlobal, Op::GetLocal, Op::TailCall, Op::Return],
            "the loop body's own recursive self-call must be a TailCall"
        );
        // But the *initial* invocation, from this non-tail (top-level)
        // context, must remain a plain Call.
        let entry = entry_of(&module);
        assert!(opcode_sequence(&entry.code).contains(&Op::Call));
        assert!(!opcode_sequence(&entry.code).contains(&Op::TailCall));
    }

    #[test]
    fn named_lets_initial_invocation_is_a_tail_call_when_the_named_let_itself_is_in_tail_position()
    {
        // (lambda () (let loop ((i 0)) i)) -- the named-let expression is
        // this function's own tail expression, so its initial invocation
        // (as opposed to the loop body's internal self-calls) is a
        // TailCall too.
        let program = [list(vec![
            sym("lambda"),
            list(vec![]),
            list(vec![
                sym("let"),
                sym("loop"),
                list(vec![binding("i", Sexpr::Int(0))]),
                sym("i"),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        let outer_fn_chunk = &module.functions[1]; // loop's chunk is [0], the lambda's own chunk is [1]
        let ops = opcode_sequence(&outer_fn_chunk.code);
        assert!(
            ops.contains(&Op::TailCall),
            "the named-let's initial invocation must be a TailCall here: {ops:?}"
        );
        assert!(!ops.contains(&Op::Call));
    }

    #[test]
    fn set_value_call_is_never_a_tail_call() {
        // (lambda (x) (set! x (f x))) -- set!'s value expression is never
        // in tail position, even though the whole set! is this function's
        // own tail expression (its own result is Unspecified, not f's
        // result).
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("x")]),
            list(vec![sym("set!"), sym("x"), list(vec![sym("f"), sym("x")])]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert!(ops.contains(&Op::Call));
        assert!(!ops.contains(&Op::TailCall));
    }

    #[test]
    fn and_non_last_operand_call_is_never_tail_but_the_last_operand_is() {
        // (lambda (n) (and (p n) (f n))) -- p's call, a non-last operand,
        // is never tail; f's call, the last operand, inherits and's own
        // tail status (here, the whole function's tail position).
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![
                sym("and"),
                list(vec![sym("p"), sym("n")]),
                list(vec![sym("f"), sym("n")]),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert_eq!(
            ops.iter().filter(|op| **op == Op::Call).count(),
            1,
            "the non-last operand (p n) must be a plain Call: {ops:?}"
        );
        assert_eq!(
            ops.iter().filter(|op| **op == Op::TailCall).count(),
            1,
            "the last operand (f n) must be a TailCall: {ops:?}"
        );
    }

    #[test]
    fn cond_arrow_clause_application_is_a_tail_call_when_the_whole_cond_is() {
        // (lambda (n) (cond (n => f))) -- the arrow clause's application of
        // f to the test value inherits cond's own tail status.
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![sym("cond"), list(vec![sym("n"), sym("=>"), sym("f")])]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert!(
            ops.contains(&Op::TailCall),
            "the arrow clause's application must be a TailCall here: {ops:?}"
        );
        assert!(!ops.contains(&Op::Call));
    }

    #[test]
    fn case_clause_body_call_is_a_tail_call_when_the_whole_case_is() {
        // (lambda (n) (case n ((1) (f n)) (else (g n)))) -- both clause
        // bodies inherit case's own tail status; the key expression itself
        // never does (it's just an integer literal here, so it emits no
        // call either way).
        let program = [list(vec![
            sym("lambda"),
            list(vec![sym("n")]),
            list(vec![
                sym("case"),
                sym("n"),
                list(vec![
                    list(vec![Sexpr::Int(1)]),
                    list(vec![sym("f"), sym("n")]),
                ]),
                list(vec![sym("else"), list(vec![sym("g"), sym("n")])]),
            ]),
        ])];
        let module = compile_program(&program).unwrap();
        let fn_chunk = &module.functions[0];
        let ops = opcode_sequence(&fn_chunk.code);
        assert_eq!(
            ops.iter().filter(|op| **op == Op::TailCall).count(),
            2,
            "both case-clause bodies must be tail calls: {ops:?}"
        );
        assert!(!ops.contains(&Op::Call));
    }

    // --- B13: quasiquotation error paths (spec 3.4) ---

    #[test]
    fn unquote_splicing_outside_of_a_list_or_vector_is_a_clean_compile_error() {
        let forms = crate::reader::read_program("`,@(list 1 2)").unwrap();
        let err = compile_program(&forms).unwrap_err();
        assert!(err.message.contains("unquote-splicing"));
    }

    #[test]
    fn unquote_splicing_as_the_sole_element_of_a_quasiquoted_list_is_still_an_error_when_not_at_level_one()
     {
        // Nested one level deeper (inside a second backquote), the same
        // `,@x` marker doesn't reach level 0 and is reconstructed as
        // literal data instead -- this must NOT hit the "outside of a list"
        // error, since it's still inside one.
        let forms = crate::reader::read_program("`(a `(,@b))").unwrap();
        assert!(compile_program(&forms).is_ok());
    }

    #[test]
    fn quasiquote_with_zero_arguments_is_a_clean_compile_error() {
        let program = [list(vec![sym("quasiquote")])];
        assert!(compile_program(&program).is_err());
    }

    #[test]
    fn quasiquote_with_more_than_one_argument_is_a_clean_compile_error() {
        let program = [list(vec![sym("quasiquote"), sym("a"), sym("b")])];
        assert!(compile_program(&program).is_err());
    }

    fn deeply_nested_quasiquote_forms(depth: usize) -> Sexpr {
        // N literal nested `quasiquote` FORMS (unlike `nested_quasiquoted_list`
        // below, which nests plain lists once inside a single quasiquote):
        // `expand_quasiquote`'s own `is_tagged(items, "quasiquote")` branch
        // recurses into itself directly, entirely before `compile_expr` ever
        // sees anything -- the one recursion path its removed depth counter
        // used to bound that `compile_expr`'s own downstream guard cannot,
        // since it only ever inspects the fully-expanded RESULT, never the
        // recursion that builds it. Only reachable via a hand-built AST:
        // the reader's own guard bounds real source text's backquote-
        // shorthand nesting to far below any depth that could matter here.
        let mut expr = sym("x");
        for _ in 0..depth {
            expr = Sexpr::List(vec![sym("quasiquote"), expr]);
        }
        expr
    }

    #[test]
    fn deeply_nested_quasiquote_forms_comfortably_under_the_maximum_still_compile() {
        let program = [deeply_nested_quasiquote_forms(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    #[ignore = "manual verification only: confirms the fix at a depth that previously crashed \
                the process outright in a DEBUG build (qa test-design review msg #238: this \
                specific depth doesn't reproduce the pre-fix crash under --release, where the \
                stack overflow this guard prevents needs a much deeper hand-built input, ~30,000+ \
                on typical hardware -- the guard restored above fires at the same, much shallower \
                threshold regardless of build profile, so this probe is still valid evidence the \
                fix works, just not evidence the crash it prevents is reachable at exactly this \
                depth in every profile); not run by default since a regression here would abort \
                the whole test binary rather than fail cleanly"]
    fn probe_manual_verify_deeply_nested_quasiquote_forms_at_the_previously_crashing_depth_now_errors_cleanly()
     {
        let program = [deeply_nested_quasiquote_forms(2000)];
        let error = compile_program(&program).unwrap_err();
        assert!(error.message.contains("quasiquote"));
    }

    #[test]
    fn deeply_nested_quasiquote_forms_of_one_more_than_the_maximum_is_rejected_by_expand_quasiquotes_own_guard()
     {
        // Regression test for qa test-design review msg #225: removing
        // `expand_quasiquote`'s own depth counter (as redundant with
        // `compile_expr`'s downstream check) relied on an unstated
        // invariant -- that only the reader ever produces trees fed to
        // `compile_program`, and the reader's own guard keeps real
        // backquote-shorthand nesting far below any depth that could
        // matter. A hand-built AST bypasses that guard entirely: nested
        // `quasiquote` FORMS recurse directly inside `expand_quasiquote`
        // itself, entirely before `compile_expr`'s check ever runs on
        // anything -- confirmed to be a genuine, currently-reachable
        // native stack overflow via `compile_program`'s public API (at a
        // depth as low as 2,000 -- far too deep to safely reproduce in a
        // committed test, so not attempted here; verified by hand instead).
        //
        // At THIS depth specifically, `compile_expr`'s own downstream
        // guard would also happen to catch the fully-expanded result
        // anyway (513 native recursion frames doesn't itself risk a
        // crash) -- so asserting only "some nesting error occurred" would
        // pass even with this restored guard entirely removed again,
        // silently losing this regression's protection. Asserting the
        // SPECIFIC message this guard alone produces (distinct from
        // `too_deep()`'s generic wording) is what actually distinguishes
        // "this guard fired" from "the other, insufficient one did".
        let program = [deeply_nested_quasiquote_forms(MAX_NESTING_DEPTH + 1)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("quasiquote") && error.message.contains("nesting"),
            "expected expand_quasiquote's own nesting-depth error, got: {}",
            error.message
        );
    }

    #[test]
    fn compile_program_does_not_depend_on_the_calling_threads_own_stack_size() {
        // Regression test for warden security review msg #242: the depth
        // counter above correctly costs one logical unit per real nesting
        // level, but the plain nested-list template shape still costs
        // TWO real Rust stack frames per unit (one hop into
        // `expand_qq_sequence`, one back into `expand_quasiquote`), versus
        // one for the repeated-quasiquote-FORMS shape the guard was
        // originally sized for -- so the same logical depth limit leaves a
        // smaller real-stack safety margin for this shape than the other.
        // `compile_program` now runs on its own dedicated, generously-sized
        // thread specifically so this never depends on the caller at all.
        //
        // qa test-design WARNING msg #245: at THIS depth (5,000) and stack
        // size (1 MiB), this test does not actually discriminate fixed
        // from reverted under `cargo test --release` -- the profile this
        // whole review process tests in -- release's smaller stack frames
        // mean neither variant crashes here. It's kept anyway as a fast,
        // always-run smoke check that this exact call shape returns a
        // clean `Err` rather than hanging or panicking; the actual
        // fixed-vs-reverted discriminating regression coverage for this
        // fix lives in `tests/cli_integration/internals.rs`
        // (`compiling_a_hand_built_deeply_nested_quasiquote_list_does_not_crash_on_a_severely_constrained_calling_thread`),
        // which observes a real OS process from outside itself and so can
        // register a genuine stack overflow (which aborts, not unwinds --
        // something `.join()` here structurally cannot catch either way)
        // as an ordinary, targeted test failure instead of taking the
        // whole suite down.
        //
        // Built on this (ordinary-stack) thread, not inside the
        // constrained one below: dropping a 5,000-deep `Sexpr` tree uses
        // Rust's default recursive `Drop` glue (a separate, already-
        // documented limitation, not what this test targets), which
        // would itself crash a 1 MiB stack regardless of anything this
        // test is actually checking. Only borrowed into the constrained
        // thread, never owned/dropped there.
        let program = [nested_quasiquoted_list(5000)];
        let result = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(1024 * 1024)
                .spawn_scoped(scope, || compile_program(&program))
                .expect("should spawn the constrained calling thread")
                .join()
                .expect("compile_program itself must not crash the calling thread")
        });
        assert!(result.is_err());
    }

    fn nested_quasiquoted_list(depth: usize) -> Sexpr {
        // A hand-built AST (bypassing the reader entirely, unlike the
        // backtick-nesting reader tests): `expand_quasiquote` no longer has
        // its own depth counter (removed as redundant with the check
        // below, since every reader-sourced template's nesting is bounded
        // by the reader's own guard) -- this instead exercises
        // `compile_expr`'s `MAX_NESTING_DEPTH` guard on the fully-expanded
        // result tree, reachable this deep only via a hand-built AST, not
        // through any real source text -- mirrors nested_call's same
        // rationale above.
        let mut expr = Sexpr::Int(1);
        for _ in 0..depth {
            expr = Sexpr::List(vec![expr]);
        }
        Sexpr::List(vec![sym("quasiquote"), expr])
    }

    #[test]
    fn quasiquote_template_nesting_comfortably_under_the_configured_maximum_still_compiles() {
        let program = [nested_quasiquoted_list(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn quasiquote_template_nesting_close_to_the_readers_own_maximum_still_compiles() {
        // Regression test for warden security review msg #240: the
        // restored depth counter double-charged ordinary nested-list-
        // under-one-backtick templates (once when `expand_quasiquote`
        // delegated to `expand_qq_sequence`, again when `expand_qq_sequence`
        // recursed back into `expand_quasiquote` for the one element) --
        // silently halving the usable nesting depth to ~255 for this
        // common shape, well under the reader's own advertised limit of
        // 511 for real source text of the same shape. 500 is comfortably
        // under 511 with correct (single) counting, but was already
        // rejected under the double-counting bug (500 * 2 > 512).
        let program = [nested_quasiquoted_list(500)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn quasiquote_template_nesting_of_one_more_than_the_configured_maximum_is_a_clean_error_not_a_crash()
     {
        let program = [nested_quasiquoted_list(MAX_NESTING_DEPTH + 1)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("nesting") && error.message.contains("depth"),
            "expected a nesting-depth error, got: {}",
            error.message
        );
    }

    fn flat_quasiquoted_list(count: usize) -> Sexpr {
        // Unlike `nested_quasiquoted_list` above (N levels of nesting
        // around a single element), this is N elements side by side at
        // the SAME level -- the shape that made `expand_qq_sequence` build
        // an N-deep `append` chain (warden security review's flat-list
        // stack-overflow finding) rather than the shape that exercises
        // `compile_expr`'s ordinary nesting guard.
        let items: Vec<Sexpr> = (0..count).map(|i| Sexpr::Int(i as i64)).collect();
        Sexpr::List(vec![sym("quasiquote"), Sexpr::List(items)])
    }

    #[test]
    fn quasiquote_template_element_count_comfortably_under_the_configured_maximum_still_compiles() {
        let program = [flat_quasiquoted_list(100)];
        assert!(compile_program(&program).is_ok());
    }

    #[test]
    fn quasiquote_template_element_count_of_exactly_the_configured_maximum_is_rejected_by_the_ordinary_nesting_guard_not_this_new_check()
     {
        // Pins the boundary itself (exactly `MAX_QUASIQUOTE_SEQUENCE_LEN`,
        // not one more) so the check is a strict `>`, not `==` or `>=` --
        // either of which would make THIS specific length trip the new
        // element-count check. `MAX_QUASIQUOTE_SEQUENCE_LEN` is well above
        // `MAX_NESTING_DEPTH`, so a template this long is already rejected
        // by `compile_expr`'s ordinary nesting guard regardless -- the
        // distinguishing signal is WHICH error comes back, not whether one
        // does.
        let program = [flat_quasiquoted_list(MAX_QUASIQUOTE_SEQUENCE_LEN)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("nesting") && error.message.contains("depth"),
            "expected the ordinary nesting-depth error (not the new element-count one), got: {}",
            error.message
        );
    }

    #[test]
    fn quasiquote_template_element_count_of_one_more_than_the_configured_maximum_is_a_clean_error_not_a_crash()
     {
        // Regression test for warden security review's flat-list finding:
        // a flat template with enough elements used to build an
        // `append`-chain tree deep enough to overflow the stack on drop
        // (confirmed at ~110,000+ elements) instead of erroring cleanly.
        // This pins the bound well below the crash threshold, so this
        // test stays fast and never risks reproducing the actual crash.
        let program = [flat_quasiquoted_list(MAX_QUASIQUOTE_SEQUENCE_LEN + 1)];
        let error = compile_program(&program).unwrap_err();
        assert!(
            error.message.contains("elements") && error.message.contains("maximum"),
            "expected a template-too-long error, got: {}",
            error.message
        );
    }

    #[test]
    fn bare_unquote_outside_quasiquote_is_a_clean_compile_error_not_a_runtime_one() {
        // qa test-design review (msg #225): a stray `,x` at the top level
        // (outside any quasiquote template) used to fall through to
        // ordinary function-call compilation against an undefined global
        // `unquote`, failing only at runtime with a generic unbound-global
        // error -- a real user-facing rough edge, since `,x` outside a
        // template is unambiguously meaningless, not merely an unbound
        // name. Now caught directly in compile_expr's dispatch (reachable
        // ONLY this way: inside an actual quasiquote template,
        // expand_quasiquote's own tag matching consumes `unquote`/
        // `unquote-splicing` forms before compile_expr ever sees them).
        let forms = crate::reader::read_program(",x").unwrap();
        let error = compile_program(&forms).unwrap_err();
        assert!(
            error.message.contains("unquote") && error.message.contains("quasiquote"),
            "expected a clear unquote-outside-quasiquote error, got: {}",
            error.message
        );
    }

    #[test]
    fn bare_unquote_splicing_outside_quasiquote_is_a_clean_compile_error_not_a_runtime_one() {
        let forms = crate::reader::read_program(",@x").unwrap();
        let error = compile_program(&forms).unwrap_err();
        assert!(
            error.message.contains("unquote-splicing") && error.message.contains("quasiquote"),
            "expected a clear unquote-splicing-outside-quasiquote error, got: {}",
            error.message
        );
    }

    #[test]
    fn quasiquote_compiles_to_only_already_existing_opcodes_no_new_bytecode() {
        // qa test-design review (msg #220): the "no new bytecode" claim in
        // quasiquote's own design doc comment was previously unverified --
        // nothing would have caught a future change that special-cased
        // quasiquote with a dedicated opcode (same output, different
        // bytecode, every functional test still passing). `opcode_sequence`
        // panics on any byte it doesn't recognize, so successfully decoding
        // a compiled quasiquote expression's bytecode here IS the guard:
        // a genuinely new opcode would make this test panic, not just the
        // ones that happen to exercise it directly.
        let forms = crate::reader::read_program("`(1 ,(+ 1 1) ,@(list 2 3) #(4 ,5) . ,6)").unwrap();
        let module = compile_program(&forms).unwrap();
        let entry = entry_of(&module);
        // Doesn't panic => every opcode byte is one `opcode_sequence`
        // already knows how to decode.
        let ops = opcode_sequence(&entry.code);
        assert!(!ops.is_empty());
    }
}
