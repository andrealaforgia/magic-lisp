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
    # Evidence: (define x 1) (let ((x 2) (y x)) (display y)) -> 1
    #   (sibling-blind: y sees outer x=1, not sibling x=2)

  Scenario: E2 — let* bindings see those introduced before them
    Given a let* group where a later binding's expression references an earlier one
    When "(let* ((x 1) (y (+ x 1))) (display y))" is evaluated
    Then it displays "2"
    # Evidence: (let* ((x 1) (y (+ x 1))) (display y)) -> 2
    #   (y sees the already-established x)

  Scenario: E3 — letrec bindings see every other binding, including themselves
    Given a letrec-bound self-referencing recursive function
    When it is called with an argument requiring multiple recursive steps
    Then it terminates and returns the mathematically correct result
    # Evidence: (display (letrec ((fact (lambda (n) (if (< n 2) 1 (* n (fact (- n 1)))))))
    #   (fact 5))) -> 120

  Scenario: E4 — named local-binding loop produces iteration without a separate function definition
    Given a named-let loop summing from 1 to 100
    When it is evaluated
    Then it displays "5050"
    # Evidence: (display (let loop ((i 1) (sum 0)) (if (> i 100) sum (loop (+ i 1) (+ sum i)))))
    #   -> 5050

  Scenario: E5 — a run of internal definitions at the start of a body sees each other regardless of order
    Given a function body starting with definitions that reference each other out of declaration order
    When the function is called
    Then it resolves correctly as if all definitions were mutually visible from the start
    # Evidence: (define (f) (define (double x) (* x 2)) (define (six-times x) (double (triple x)))
    #   (define (triple x) (* x 3)) (six-times 5)) (display (f)) -> 30
    #   (six-times references triple, defined AFTER it -- proves order-independent mutual visibility)

  Scenario: E6 — set! mutates an existing binding, and fails distinctly on an undefined one
    Given a bound variable, and separately a name that was never defined
    When the bound variable is mutated with set! and then displayed
    Then it shows the new value
    When set! is applied to the undefined name
    Then the process fails with a distinct, non-zero exit code separate from success/usage/read-compile/file-format errors
    # Evidence: (define v 0) (set! v 1) (display v) -> 1
    #   (set! never-defined 1) -> stderr "runtime error: cannot set! undefined variable: never-defined",
    #   exit 70 (RUNTIME_ERROR class -- distinct from success=0, usage=64, source=65, bad-artifact=66)

  Scenario: E7 — cond checks tests in order with an else fallback, and supports the apply-to-test-value variant
    Given a cond with several falsy tests and a trailing else
    When it is evaluated
    Then the else branch's value is returned
    Given a cond clause using the "=>" variant with a truthy test value
    When it is evaluated
    Then the function is applied to the test's own value
    # Evidence: (display (cond (#f 1) (#f 2) (else 3))) -> 3
    #   (display (cond (5 => (lambda (x) (* x 2))))) -> 10

  Scenario: E8 — case matches a key against groups of candidates by equivalence, with an else fallback
    Given a case expression with a key matching one candidate group, and separately a key matching none
    When each is evaluated
    Then the matching group's body runs, and the non-matching key falls through to else
    # Evidence: (display (case 2 ((1 2 3) "hi") (else "bye"))) -> hi
    #   (display (case 99 ((1 2 3) "hi") (else "bye"))) -> bye

  Scenario: E9 — and short-circuits on the first falsy value, else returns the last value
    Given an and expression where every argument is truthy
    When it is evaluated
    Then it returns the last value
    Given an and expression where an early argument is falsy and a later argument has a side effect
    When it is evaluated
    Then the falsy value is returned and the later side effect never runs
    # Evidence: (display (and 1 2 3)) -> 3
    #   (define fired #f) (and #f (begin (set! fired #t) 1)) (display fired) -> #f
    #   (the begin never ran -- and short-circuited at the first falsy value)

  Scenario: E10 — or short-circuits on the first truthy value, else returns the last value
    Given an or expression where an early argument is truthy and a later argument has a side effect
    When it is evaluated
    Then the truthy value is returned and the later side effect never runs
    Given an or expression where every argument is falsy
    When it is evaluated
    Then it returns the last (falsy) value, proving the "no truthy value found" branch actually runs
    # Evidence: (display (or #f 'x 'y)) -> x
    #   (define fired2 #f) (or 1 (begin (set! fired2 #t) 2)) (display fired2) -> #f
    #   (the begin never ran -- or short-circuited at the first truthy value)
    #   (display (or #f #f #f)) -> #f, exit 0
    #   (genuinely all-falsy: distinguishes "returns last value because none were truthy"
    #   from "returns the first/only truthy value it happened to hit")

  Scenario: E11 — when and unless are one-sided conditionals
    Given all four combinations of when/unless with a truthy or falsy condition
    When each is evaluated
    Then when-true runs its body, when-false doesn't, unless-false runs its body, unless-true doesn't
    # Evidence: (display (when #t 1)) -> 1                                          [when-true]
    #   (define ran #f) (when #f (set! ran #t)) (display ran) -> #f                [when-false]
    #   (display (unless #f 1)) -> 1                                                [unless-false]
    #   (define ran2 #f) (unless #t (set! ran2 #t)) (display ran2) -> #f            [unless-true]

  Scenario: E12 — integration: all eight verbatim demo programs produce exactly the prescribed output
    Given each of the eight DEMO programs from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output followed by a trailing newline, and exits 0
    # Evidence (each "-> ...\n", exit 0):
    #   1. (display (let loop ((i 1) (sum 0)) (if (> i 100) sum (loop (+ i 1) (+ sum i))))) (newline) -> 5050
    #   2. (display (let* ((x 2) (y (* x 3))) y)) (newline) -> 6
    #   3. (display (cond (5 => (lambda (x) (* x 2))))) (newline) -> 10
    #   4. (display (case 2 ((1 2 3) "hi") (else "bye"))) (newline) -> hi
    #   5. (display (and 1 2 3)) (newline) -> 3
    #   6. (display (or #f 'x 'y)) (newline) -> x
    #   7. (define v 0) (set! v 1) (display v) (newline) -> 1
    #   8. (define (f) (define (double x) (* x 2)) (define (six-times x) (double (triple x)))
    #        (define (triple x) (* x 3)) (six-times 5)) (display (f)) (newline) -> 30
