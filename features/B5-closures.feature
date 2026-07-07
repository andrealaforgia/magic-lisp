Feature: B5 — Closures that remember and share their surroundings
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want functions created inside other functions to keep working with captured locals, sharing live storage
  So that stateful abstractions like counters and getter/setter pairs work correctly on top of B1-B4

  # Builds on B1-B4. Needs only enough pair support (construct a pair, retrieve each
  # half) to run the demos below — the full pair/list operation library is a later
  # behaviour, out of scope here. How capture and sharing are represented internally
  # is not observable and not part of this behaviour.

  Scenario: E1 — a closure outlives its creator and still sees the captured local
    Given a factory function that returns a closure capturing one of its parameters
    When the factory call fully returns, and the returned closure is called afterward
    Then it correctly uses the value captured at creation time, not a default or garbage value
    # Evidence: (define (make-adder n) (lambda (x) (+ x n))) (display ((make-adder 3) 4)) -> 7
    #   (the factory call fully returns before the closure is invoked, and it still
    #   correctly uses the captured n=3)

  Scenario: E2 — captured variables are shared live storage, not a value snapshot
    Given a factory returning a getter closure and a setter closure over one shared local
    When the setter is called with a value and then the getter is called
    Then the getter observes the value the setter wrote
    # Evidence (DEMO 2, pair factory):
    #   (define (pairf) (let ((x 0)) (cons (lambda () x) (lambda (v) (set! x v)))))
    #   (define p (pairf)) ((cdr p) 10) (display ((car p))) -> 10
    #   (the setter and getter closures share one storage cell)

  Scenario: E3 — each call to the creator function produces a fresh, independent variable
    Given two independent closures created from two separate calls to the same counter-factory function
    When calls to the two closures are interleaved (first counter twice, then second counter once)
    Then each counter's state is independent of the other's
    # Evidence (DEMO 1, counter factory):
    #   (define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n)))
    #   (define a (counter)) (define b (counter))
    #   (display (a)) (newline) (display (a)) (newline) (display (b)) -> 1, 2, 1

  Scenario: E4 — pairs can be constructed and each half retrieved back out correctly
    Given a pair constructed from two distinguishable values
    When each half is retrieved
    Then each half matches its original position, not swapped or merged
    # Evidence: (display (cons "a" "b")) -> (a . b)
    #   (display (car (cons 1 2))) (display (cdr (cons 1 2))) -> 12
    #   (car/cdr retrieve the correct, unswapped half)

  Scenario: E5 — integration: both verbatim demo programs produce exactly the prescribed output
    Given the counter-factory DEMO and the pair-factory DEMO from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output followed by a trailing newline, and exits 0
    # Evidence:
    #   $ magiclisp eval demo1.ml -> 1 / 2 / 1, exit 0
    #   $ magiclisp eval demo2.ml -> 10, exit 0
    #
    # Also verified beyond the required demos (regression tests, not just ad hoc):
    # a doubly-nested lambda correctly captures a grandparent's local (two levels of
    # upvalue indirection), and mutating a captured variable before the closure is ever
    # called is still observed afterward (proving a live cell, not a value snapshot).
    #
    # Disclosed alongside this evidence: implementing general closures fixed a real gap
    # from a prior B3 test-design review (qa-confirmed as load-bearing) — a lambda
    # referencing an enclosing let's local used to fail with "unbound global"; it now
    # resolves correctly via this behaviour's closure mechanism. Verified this doesn't
    # retroactively invalidate any B3-accepted evidence: B3's own scenarios only use
    # lambda via letrec self-reference and cond's `=>` variant, neither of which
    # captures an enclosing let's local — that exact shape is now covered by E2/E3 above.
