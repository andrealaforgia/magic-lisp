Feature: B3 — Local bindings, mutation, and the full family of conditional/sequencing forms
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want local scratch variables, mutation, named-loop iteration, and a fuller set of conditional/sequencing forms
  So that I can express logic concisely on top of B1/B2's functions, recursion, and if

  # Builds on B1/B2 (functions, recursion, if). Closures that capture and share a variable
  # across function-call boundaries, and loops/recursion running in guaranteed constant
  # space, are explicitly OUT of scope — the named-loop form here only needs to produce
  # correct results, not run in constant space. How local scopes, mutation, or these forms
  # are implemented internally is not observable and not part of this behaviour.

  Scenario: E1 — let bindings see only the outer scope, not their siblings
    Given an outer x bound to 1, and a let group binding a sibling x to 2 and y to the outer x
    When "(define x 1) (let ((x 2) (y x)) (display y))" is evaluated
    Then it displays "1" — y resolves to the outer x, not the sibling binding

  Scenario: E2 — let* bindings see those introduced before them
    Given a let* group where a later binding's expression references an earlier one
    When "(let* ((x 1) (y (+ x 1))) (display y))" is evaluated
    Then it displays "2"

  Scenario: E3 — letrec bindings see every other binding, including themselves
    Given a letrec-bound self-referencing recursive function
    When it is called with an argument requiring multiple recursive steps
    Then it terminates and returns the mathematically correct result

  Scenario: E4 — named local-binding loop produces iteration without a separate function definition
    Given a named-let loop summing from 1 to 100
    When it is evaluated
    Then it displays "5050"

  Scenario: E5 — a run of internal definitions at the start of a body sees each other regardless of order
    Given a function body starting with definitions that reference each other out of declaration order
    When the function is called
    Then it resolves correctly as if all definitions were mutually visible from the start

  Scenario: E6 — set! mutates an existing binding, and fails distinctly on an undefined one
    Given a bound variable, and separately a name that was never defined
    When the bound variable is mutated with set! and then displayed
    Then it shows the new value
    When set! is applied to the undefined name
    Then the process fails with a distinct, non-zero exit code separate from success/usage/read-compile/file-format errors

  Scenario: E7 — cond checks tests in order with an else fallback, and supports the apply-to-test-value variant
    Given a cond with several falsy tests and a trailing else
    When it is evaluated
    Then the else branch's value is returned
    Given a cond clause using the "=>" variant with a truthy test value
    When it is evaluated
    Then the function is applied to the test's own value

  Scenario: E8 — case matches a key against groups of candidates by equivalence, with an else fallback
    Given a case expression with a key matching one candidate group, and separately a key matching none
    When each is evaluated
    Then the matching group's body runs, and the non-matching key falls through to else

  Scenario: E9 — and short-circuits on the first falsy value, else returns the last value
    Given an and expression where every argument is truthy
    When it is evaluated
    Then it returns the last value
    Given an and expression where an early argument is falsy and a later argument has a side effect
    When it is evaluated
    Then the falsy value is returned and the later side effect never runs

  Scenario: E10 — or short-circuits on the first truthy value, else returns the last value
    Given an or expression where an early argument is truthy and a later argument has a side effect
    When it is evaluated
    Then the truthy value is returned and the later side effect never runs
    Given an or expression where every argument is falsy
    When it is evaluated
    Then it returns the last (falsy) value, proving the "no truthy value found" branch actually runs

  Scenario: E11 — when and unless are one-sided conditionals
    Given all four combinations of when/unless with a truthy or falsy condition
    When each is evaluated
    Then when-true runs its body, when-false doesn't, unless-false runs its body, unless-true doesn't

  Scenario: E12 — integration: all eight verbatim demo programs produce exactly the prescribed output
    Given each of the eight DEMO programs from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output followed by a trailing newline, and exits 0

  Scenario: E13 — two sequential sibling let/let* blocks in one body don't alias each other's slot
    Given a function body containing two independent, non-nested let blocks one after another
    When the function is called
    Then each let's binding is evaluated correctly, with neither overwritten by or aliased to the other's runtime slot
    # Retroactive addition: disclosed by the Builder as a real, previously-unknown bug found
    # while adding unrelated test coverage (commit 05855e1) — the compiler's local-slot
    # counter was copied instead of shared when cloning scope context, so two sequential
    # sibling lets in the same body were assigned the same physical runtime slot.

  Scenario: E14 — a nested let shadows the outer binding, then the outer resumes once the inner scope closes
    Given a let binding x to 1, containing a nested let that rebinds x to 2
    When the function is called
    Then the inner scope sees 2 while it is active, and the outer scope sees its own 1 again afterward
    # Retroactive addition: scope-edge-case regression coverage (qa test-design review, msg #49)

  Scenario: E15 — set! from a nested scope mutates the outer let's own binding, not a shadowed copy
    Given a let binding x to 1, containing a nested let that mutates x with set!
    When the function is called
    Then the outer x reflects the mutation once the inner scope closes
    # Retroactive addition: scope-edge-case regression coverage (qa test-design review, msg #49)

  Scenario: E16 — a letrec binding referencing another binding before it is initialized fails cleanly
    Given a letrec group where one binding's own initializer reads another binding that has not run yet
    When the letrec expression is evaluated
    Then the process fails with a distinct, non-zero exit code separate from success/usage/read-compile/file-format errors
    # Retroactive addition: scope-edge-case regression coverage (qa test-design review, msg #49)

  Scenario: E17 — a lambda body correctly captures a variable from its enclosing let
    Given a let binding x, containing a lambda with no parameters of its own that references x
    When the function is called
    Then it correctly resolves x from the enclosing scope
    # Retroactive addition: scope-edge-case regression coverage (qa test-design review, msg #49) —
    # originally a documented B3-era limitation (lambdas compiled as separate chunks had no access
    # to enclosing locals and failed with "unbound global"); B5 gave lambdas real upvalue capture,
    # so this now resolves correctly.
