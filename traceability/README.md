# Traceability store

One record per expectation, across every delivered behaviour (B1-B23, EX1, DOC1).
Each record carries that expectation's full evidence and a reference to the exact
`.feature` file and scenario that crystallises it. See `../features/*.feature` for the
executable Given/When/Then scenarios themselves -- the inline evidence comments that
used to live there have been relocated here (TRACE1).

## B1

- [E1](B1/E1.md) -- eval reads and runs a source file directly
- [E2](B1/E2.md) -- compile then run reproduces the source program's behaviour
- [E3](B1/E3.md) -- disasm prints a human-readable instruction listing
- [E4](B1/E4.md) -- each of the five verbs is routed to genuinely distinct handling
- [E5](B1/E5.md) -- the reader accepts numbers, symbols, escaped strings, booleans, nested lists, comments, and whitespace together
- [E6](B1/E6.md) -- an unescaped literal newline inside a string literal is a read error
- [E7](B1/E7.md) -- invalid or corrupt compiled artifacts are rejected outright by run and disasm
- [E8](B1/E8.md) -- the five failure classes produce pairwise-distinct exit codes
- [E9](B1/E9.md) -- display, newline, and variadic + work out of the box, output stays ordered and fully flushed
- [E10](B1/E10.md) -- the full pipeline works end-to-end across process boundaries (integration)
- [E11](B1/E11.md) -- compiling the same source text twice is deterministic

## B2

- [E1](B2/E1.md) -- quote and its shorthand evaluate to the literal datum, unevaluated
- [E2](B2/E2.md) -- if with and without an else branch
- [E3](B2/E3.md) -- define binds values and functions with flexible parameter lists
- [E4](B2/E4.md) -- lambda produces callable values with the same parameter-list flexibility
- [E5](B2/E5.md) -- begin runs expressions in order and yields the value of the last one
- [E6](B2/E6.md) -- redefining a top-level name replaces it, resolved at call time not define time
- [E7](B2/E7.md) -- a function can call itself and terminates correctly at its base case
- [E8](B2/E8.md) -- call arguments are evaluated left-to-right before the call is applied
- [E9](B2/E9.md) -- only #f is falsy; every other value, including 0 and the empty list, is truthy
- [E10](B2/E10.md) -- subtraction, multiplication, and comparisons accept 2+ args, and comparisons check the whole chain
- [E11](B2/E11.md) -- integer overflow wraps around instead of erroring or promoting to bignum
- [E12](B2/E12.md) -- displayed values format as ordinary decimal numbers and #t/#f booleans
- [E13](B2/E13.md) -- integration: the behaviour's two verbatim demo programs produce exactly the prescribed output

## B3

- [E1](B3/E1.md) -- let bindings see only the outer scope, not their siblings
- [E2](B3/E2.md) -- let* bindings see those introduced before them
- [E3](B3/E3.md) -- letrec bindings see every other binding, including themselves
- [E4](B3/E4.md) -- named local-binding loop produces iteration without a separate function definition
- [E5](B3/E5.md) -- a run of internal definitions at the start of a body sees each other regardless of order
- [E6](B3/E6.md) -- set! mutates an existing binding, and fails distinctly on an undefined one
- [E7](B3/E7.md) -- cond checks tests in order with an else fallback, and supports the apply-to-test-value variant
- [E8](B3/E8.md) -- case matches a key against groups of candidates by equivalence, with an else fallback
- [E9](B3/E9.md) -- and short-circuits on the first falsy value, else returns the last value
- [E10](B3/E10.md) -- or short-circuits on the first truthy value, else returns the last value
- [E11](B3/E11.md) -- when and unless are one-sided conditionals
- [E12](B3/E12.md) -- integration: all eight verbatim demo programs produce exactly the prescribed output
- [E13](B3/E13.md) -- two sequential sibling let/let* blocks in one body don't alias each other's slot
- [E14](B3/E14.md) -- a nested let shadows the outer binding, then the outer resumes once the inner scope closes
- [E15](B3/E15.md) -- set! from a nested scope mutates the outer let's own binding, not a shadowed copy
- [E16](B3/E16.md) -- a letrec binding referencing another binding before it is initialized fails cleanly
- [E17](B3/E17.md) -- a lambda body correctly captures a variable from its enclosing let

## B4

- [E1](B4/E1.md) -- a general iteration form with loop variables, a step, a test, and a result
- [E2](B4/E2.md) -- number literals are recognised as float or integer, including radix prefixes
- [E3](B4/E3.md) -- an integer literal too large for the integer range is a read error
- [E4](B4/E4.md) -- float display formatting rules
- [E5](B4/E5.md) -- integer arithmetic overflow wraps around
- [E6](B4/E6.md) -- +, -, *, / variadic arg-count edge cases
- [E7](B4/E7.md) -- division's integer-vs-float result rule, every branch
- [E8](B4/E8.md) -- integration: all six verbatim demo programs produce exactly the prescribed output

## B5

- [E1](B5/E1.md) -- a closure outlives its creator and still sees the captured local
- [E2](B5/E2.md) -- captured variables are shared live storage, not a value snapshot
- [E3](B5/E3.md) -- each call to the creator function produces a fresh, independent variable
- [E4](B5/E4.md) -- pairs can be constructed and each half retrieved back out correctly
- [E5](B5/E5.md) -- integration: both verbatim demo programs produce exactly the prescribed output

## B6

- [E1](B6/E1.md) -- a self-tail-call loop runs an enormous number of iterations in flat memory
- [E2](B6/E2.md) -- mutual tail calls run an enormous number of round trips in flat memory
- [E3](B6/E3.md) -- genuine non-tail recursion nests on the order of 100,000 levels and completes correctly
- [E4](B6/E4.md) -- non-tail recursion nested too deep fails cleanly with a distinct exit code
- [E5](B6/E5.md) -- integration: all three verbatim demo programs produce exactly the prescribed output

## B7

- [E1](B7/E1.md) -- quotient, remainder, and modulo, including the floor-vs-truncate distinction and division by zero
- [E2](B7/E2.md) -- abs, min, max, and the sign/parity predicates, each shown both ways
- [E3](B7/E3.md) -- floor, ceiling, round, truncate preserve numeric type and round half to even
- [E4](B7/E4.md) -- expt, sqrt, and the transcendental functions, with the integer-power exactness rule
- [E5](B7/E5.md) -- number/integer/float predicates, exact/inexact conversions, and the non-finite error case
- [E6](B7/E6.md) -- number/text conversion, with invalid text yielding a distinguishable #f
- [E7](B7/E7.md) -- every numeric operation is a first-class procedure value, not special syntax
- [E8](B7/E8.md) -- integration: all thirteen verbatim demo expressions produce exactly the prescribed output

## B8

- [E1](B8/E1.md) -- eq? is genuine object identity, not structural sameness
- [E2](B8/E2.md) -- eqv? compares floats by value, with NaN-equal and signed-zero-unequal wrinkles
- [E3](B8/E3.md) -- equal? recurses into pairs/vectors/strings and falls back to eqv? otherwise
- [E4](B8/E4.md) -- not returns true for exactly false, false for everything else
- [E5](B8/E5.md) -- the ten type predicates are correct in both directions
- [E6](B8/E6.md) -- integration: all twelve verbatim demo expressions produce exactly the prescribed output

## B9

- [E1](B9/E1.md) -- pair mutation and multi-level accessor composition
- [E2](B9/E2.md) -- list construction, length, append, reverse, indexing, tail, and last pair
- [E3](B9/E3.md) -- member, memv, and memq search a list at three strictness levels
- [E4](B9/E4.md) -- assoc, assv, and assq search an association list at three strictness levels
- [E5](B9/E5.md) -- map, for-each, and filter, with for-each's side-effect-only nature proven distinct from map
- [E6](B9/E6.md) -- fold-left, fold-right, and reduce have genuinely distinct evaluation orders
- [E7](B9/E7.md) -- apply flattens direct arguments plus a trailing list, at both edges
- [E8](B9/E8.md) -- quoted list literals read to exactly the structure written, including nested and dotted forms
- [E9](B9/E9.md) -- integration: all fourteen verbatim demo expressions produce exactly the prescribed output

## B10

- [E1](B10/E1.md) -- length, indexing, sub-range extraction, joining, and out-of-bounds errors
- [E2](B10/E2.md) -- string equality and ordering, both directions
- [E3](B10/E3.md) -- conversions between strings, symbols, and character lists, with a round trip
- [E4](B10/E4.md) -- string case conversion, both directions, including a Unicode-aware case
- [E5](B10/E5.md) -- character conversion, comparison, and predicates, each shown both ways
- [E6](B10/E6.md) -- character literals read correctly, verified via their code points
- [E7](B10/E7.md) -- length and indexing count by displayed character, not by byte
- [E8](B10/E8.md) -- integration: all seventeen verbatim demo expressions produce exactly the prescribed output

## B11

- [E1](B11/E1.md) -- vector construction, indexing, and out-of-bounds errors in both directions
- [E2](B11/E2.md) -- vector/list conversion and whole-vector fill
- [E3](B11/E3.md) -- vector literals read to genuine vector values
- [E4](B11/E4.md) -- hash table CRUD, structural-equality keys, and missing-key handling
- [E5](B11/E5.md) -- hash table key listing is deterministic insertion order
- [E6](B11/E6.md) -- integration: all twelve verbatim demo expressions produce exactly the prescribed output
- [E7](B11/E7.md) -- a vector made self-referential, or cyclic across pairs and vectors together, terminates cleanly instead of crashing or hanging

## B12

- [E1](B12/E1.md) -- read returns data unevaluated, advances across calls, and EOF is checkable both ways
- [E2](B12/E2.md) -- read-line reads a string with the line terminator genuinely stripped, same EOF semantics
- [E3](B12/E3.md) -- display prints strings and characters raw
- [E4](B12/E4.md) -- write prints escaped, re-readable text; ordinary values are identical under write and display
- [E5](B12/E5.md) -- all output is fully flushed before exit, including with interleaved reads and writes
- [E6](B12/E6.md) -- integration: all three verbatim demo scenarios produce exactly the prescribed output

## B13

- [E1](B13/E1.md) -- a template with no markers is literal data, not evaluated as code
- [E2](B13/E2.md) -- unquote inserts a single evaluated value in place
- [E3](B13/E3.md) -- unquote-splicing flattens a list value's elements directly into the surrounding list
- [E4](B13/E4.md) -- nested quasiquote: only a marker whose level reaches zero is evaluated
- [E5](B13/E5.md) -- both markers work inside a vector template
- [E6](B13/E6.md) -- integration: all five verbatim demo expressions produce exactly the prescribed output

## B14

- [E1](B14/E1.md) -- a macro's operands are handed to its body as literal, unevaluated data
- [E2](B14/E2.md) -- the macro's expansion is itself evaluated, and macros are visible in later-defined function bodies
- [E3](B14/E3.md) -- recursive macro expansion is bounded at a floor of at least 1000 rounds
- [E4](B14/E4.md) -- gensym produces symbols distinct from every other symbol, source-written or generated
- [E5](B14/E5.md) -- a local variable shadowing a macro name wins over the macro within that scope
- [E6](B14/E6.md) -- the swap macro uses gensym internally to avoid colliding with its own operands
- [E7](B14/E7.md) -- integration: all four verbatim demo expressions produce exactly the prescribed output

## B15

- [E1](B15/E1.md) -- deliberately raised errors show the message raw and irritants machine-readable
- [E2](B15/E2.md) -- every uncaught runtime error is uniform across failure categories, and nothing after the failure point runs
- [E3](B15/E3.md) -- a read/compile error exits with its own code, distinct from the runtime-error code
- [E4](B15/E4.md) -- the exit procedure ends the program early with a chosen code or with success by default
- [E5](B15/E5.md) -- dividing a float by exactly zero is not an error, unlike dividing an integer by zero
- [E6](B15/E6.md) -- integration: all four verbatim demo scenarios produce exactly the prescribed output and exit codes

## B16

- [E1](B16/E1.md) -- every function's header shows index, name-or-distinct-placeholder, arity, variadic flag, and upvalue count
- [E2](B16/E2.md) -- every function's constant pool shows index, type, and machine-readable value, across multiple distinct types
- [E3](B16/E3.md) -- every instruction line shows a numeric offset, mnemonic, and operands, with all required instruction kinds present
- [E4](B16/E4.md) -- a jump instruction's target is an absolute offset landing on a real instruction boundary
- [E5](B16/E5.md) -- integration: both verbatim demo programs' full dumps exhibit every described property together

## B17

- [E1](B17/E1.md) -- the exact prompt bytes appear once per entry plus a final one before end-of-input
- [E2](B17/E2.md) -- results print write-style, except the unspecified value which prints nothing
- [E3](B17/E3.md) -- a definition persists across entries, and the latest redefinition wins — for both plain values and functions
- [E4](B17/E4.md) -- a runtime error reports exactly one error line, recovers, and leaves bindings intact
- [E5](B17/E5.md) -- end-of-input exits cleanly even with no errors
- [E6](B17/E6.md) -- running with no arguments starts the identical session
- [E7](B17/E7.md) -- integration: the full five-entry verbatim demo produces exactly the prescribed transcript

## B18

- [E1](B18/E1.md) -- broken source text always ends with the source-error exit code, never a crash
- [E2](B18/E2.md) -- an invalid outer-container artifact always ends with the file-format-error exit code
- [E3](B18/E3.md) -- a valid-outer-container but internally-broken artifact is reported cleanly, never a crash
- [E4](B18/E4.md) -- command-line misuse always ends with the usage-error exit code
- [E5](B18/E5.md) -- a genuine breadth sweep across many malformed inputs never crashes or exits outside the established set
- [E6](B18/E6.md) -- integration: all six verbatim demo cases exit with exactly the prescribed code

## B19

- [E1](B19/E1.md) -- line comments still work
- [E2](B19/E2.md) -- block comments, including one fully nested inside another, are skipped correctly
- [E3](B19/E3.md) -- the skip-next-datum marker removes exactly one complete datum
- [E4](B19/E4.md) -- dotted-pair literals and alternate-radix integer literals still read correctly
- [E5](B19/E5.md) -- oversized integer literals still read-error, and arithmetic overflow still wraps
- [E6](B19/E6.md) -- all ten officially published sample programs reproduce their specified results exactly
- [E7](B19/E7.md) -- integration: all four verbatim lexical demos produce exactly the prescribed output

## B20

- [E1](B20/E1.md) -- a single documented command runs a test suite covering all five required categories
- [E2](B20/E2.md) -- the formatting check produces no differences
- [E3](B20/E3.md) -- the linter with its default rule set produces no warnings
- [E4](B20/E4.md) -- the project builds on stable Rust, uses only std-library runtime dependencies, and contains no unsafe code
- [E5](B20/E5.md) -- compiling the same source twice produces byte-identical output, for macro-free and macro-using files alike
- [E6](B20/E6.md) -- integration: the documented test command, both quality gates, and the determinism check all hold together

## B21

- [E1](B21/E1.md) -- a ten-million-iteration tail-recursive loop completes within the time ceiling
- [E2](B21/E2.md) -- a naive doubly-recursive Fibonacci computation completes within the time ceiling
- [E3](B21/E3.md) -- compiling a genuine ~2000-line source file completes within the time ceiling
- [E4](B21/E4.md) -- the tail-call loop uses constant, non-growing call-frame memory on the release build
- [E5](B21/E5.md) -- sustained closure creation with a shared captured variable does not leak memory over ~60 seconds
- [E6](B21/E6.md) -- integration: all four checks hold together on the release build

## B22

- [E1](B22/E1.md) -- sustained self-referential closure creation does not leak memory over ~60 seconds
- [E2](B22/E2.md) -- sustained mutual-reference closure creation does not leak memory over ~60 seconds
- [E3](B22/E3.md) -- the cycle-safe mechanism is documented in plain language in the README
- [E4](B22/E4.md) -- integration: the self-reference, mutual-reference, and B21 acyclic patterns interleaved in one run all hold together

## B23

- [E1](B23/E1.md) -- compiling a program with a dotted-list literal well past the nesting cap succeeds
- [E2](B23/E2.md) -- the freshly-written artifact decodes back byte-for-byte
- [E3](B23/E3.md) -- running the compiled program produces the correct long dotted-list value
- [E4](B23/E4.md) -- genuine car-side nesting past the cap is still rejected
- [E5](B23/E5.md) -- integration: the long dotted list round-trips and pathological nesting is still rejected, together

## DOC1

- [E1](DOC1/E1.md) -- the document states what MagicLisp is and does at a glance
- [E2](DOC1/E2.md) -- the document honestly and completely summarises delivered scope, with facts that match SPEC.md and the test suites
- [E3](DOC1/E3.md) -- the document points the reader to how to run the tool, including the Huffman example
- [E4](DOC1/E4.md) -- every substantive claim is truthful and traceable to a real, committed artifact
- [E5](DOC1/E5.md) -- integration: read start to finish, the document holds together as an accurate closing account

## EX1

- [E1](EX1/E1.md) -- compressing a real file from the command line produces a genuine, distinct output
- [E2](EX1/E2.md) -- decompressing reproduces the original input byte-for-byte, across genuinely different inputs
- [E3](EX1/E3.md) -- a skewed-frequency input compresses measurably smaller, proving genuine Huffman coding
- [E4](EX1/E4.md) -- the documented run instructions are self-sufficient for a new, unaided user
- [E5](EX1/E5.md) -- integration: the documented pipeline round-trips a real file end to end, now via a real BDD run


## TRACE1

- [E1](TRACE1/E1.md) -- a durable, committed folder holds one record per expectation, for every behaviour
- [E2](TRACE1/E2.md) -- each record carries that expectation's evidence in full
- [E3](TRACE1/E3.md) -- each record references the specific .feature file and scenario that crystallises it
- [E4](TRACE1/E4.md) -- the .feature files' inline evidence comments are cleaned out, scenarios unchanged
- [E5](TRACE1/E5.md) -- the migration is complete and lossless
- [E6](TRACE1/E6.md) -- integration: the traceability system is navigable and the BDD suites still execute
