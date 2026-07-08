Feature: B11 — Vectors and hash tables
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want fixed-length mutable arrays and mutable key/value maps
  So that I have two more first-class data structures on top of B1-B10

  # Builds on B1-B10. No vector/hash-table operations beyond those named below are in
  # scope. How vectors or hash tables are represented, allocated, or hashed internally is
  # not observable and not part of this behaviour — only that each named operation exists,
  # behaves correctly, and key ordering is reproducible.

  Scenario: E1 — vector construction, indexing, and out-of-bounds errors in both directions
    Given a vector built from a sequence of values, a vector created with a given length and an explicit fill value, and positions inside and outside a vector's bounds
    When elements are read, replaced, and the length is measured
    Then each returns the correct value, the explicit-fill construction works, and both out-of-bounds reading and out-of-bounds writing are clean runtime errors
    # Evidence: $ magiclisp eval (display (vector-ref (vector 1 2 3) 1))          -> 2
    #   $ magiclisp eval (define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector-ref v 1)) -> 99
    #   $ magiclisp eval (display (vector-length (vector 1 2 3)))                 -> 3
    #   $ magiclisp eval (display (make-vector 3 7))                              -> #(7 7 7)
    #   $ magiclisp eval (display (vector-ref (vector 1 2 3) 3))
    #   -> "error: runtime error: vector-ref index 3 is out of range", exit 70
    #   $ magiclisp eval (vector-set! (vector 1 2 3) 3 99)
    #   -> "error: runtime error: vector-set! index 3 is out of range", exit 70

  Scenario: E2 — vector/list conversion and whole-vector fill
    Given a mutated vector converted to a list, a list converted to a vector, an existing vector filled entirely with one value, and a round trip through both conversions
    When each operation is applied
    Then each produces the correct result, the fill operation changes every position, and the round trip reproduces the original list exactly
    # Evidence: $ magiclisp eval (define v (vector 1 2 3)) (vector-set! v 1 99) (display (vector->list v)) -> (1 99 3)
    #   $ magiclisp eval (display (list->vector (list 1 2)))                      -> #(1 2)
    #   $ magiclisp eval (define v (vector 1 2 3)) (vector-fill! v 9) (display v) -> #(9 9 9)
    #   $ magiclisp eval (display (vector->list (list->vector (list 1 2 3))))     -> (1 2 3)

  Scenario: E3 — vector literals read to genuine vector values
    Given a vector literal written directly in source
    When it is displayed, checked with vector?, and indexed into
    Then it displays correctly as a whole, is recognized as a genuine vector (not merely text), and its elements are individually accessible
    # Evidence: $ magiclisp eval (display #(1 2 3))               -> #(1 2 3)
    #   $ magiclisp eval (display (vector? #(1 2 3)))              -> #t
    #   $ magiclisp eval (display (vector-ref #(1 2 3) 2))         -> 3

  Scenario: E4 — hash table CRUD, structural-equality keys, and missing-key handling
    Given an empty hash table with entries stored, retrieved, and removed by key, a compound key built separately but structurally identical to a stored one, and a missing key looked up with and without a fallback
    When each operation is applied
    Then count and presence are reported correctly, a structurally-identical but separately-built compound key still retrieves its value (equal?-based, not identity-based), a missing key with a fallback returns the fallback, and a missing key without one is a clean, distinct runtime error
    # Evidence: $ magiclisp eval (define h (make-hash)) (hash-set! h 'a 1) (hash-set! h 'b 2) (display (hash-count h)) -> 2
    #   $ magiclisp eval (display (hash-ref (make-hash) 'c "nope"))                -> nope
    #   $ magiclisp eval (define h (make-hash)) (hash-set! h 'a 1) (display (hash-has-key? h 'a)) -> #t
    #   $ magiclisp eval (hash-remove! h 'a) (display (hash-has-key? h 'a))         -> #f
    #   $ magiclisp eval (define h (make-hash)) (hash-set! h (list 1 2) 42) (display (hash-ref h (list 1 2))) -> 42
    #   (a separately-built structurally-identical list key finds the entry — equal?, not identity)
    #   $ magiclisp eval (hash-ref (make-hash) 'c)
    #   -> "error: runtime error: hash-ref: key c not found", exit 70 (distinct from the with-fallback case)
    #   Independently re-verified the structural-equality key case against the release binary.

  Scenario: E5 — hash table key listing is deterministic insertion order
    Given a two-entry table and a table with three insertions, a removal, and a re-insertion of the removed key
    When the key list is retrieved
    Then keys come back in first-insertion order, and a removed-then-re-inserted key lands at the end, not its original position
    # Evidence: $ magiclisp eval (define h (make-hash)) (hash-set! h 'a 1) (hash-set! h 'b 2) (display (hash-keys h)) -> (a b)
    #   $ magiclisp eval (define h (make-hash)) (hash-set! h 'a 1) (hash-set! h 'b 2) (hash-set! h 'c 3)
    #     (hash-remove! h 'a) (hash-set! h 'a 99) (display (hash-keys h)) -> (b c a)
    #   (the re-inserted key lands at the end, not restored to its old position)
    #   Independently re-verified against the release binary.

  Scenario: E6 — integration: all twelve verbatim demo expressions produce exactly the prescribed output
    Given all twelve DEMO expressions/sequences from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
    # Evidence: $ cat e6.ml (all 12 demo expressions in sequence)
    #   $ magiclisp eval e6.ml ->
    #   2 / 99 / 3 / (1 99 3) / #(0 0 0) / #(1 2) / #(1 2 3) / 2 / (a b) / nope / #t / #f
    #   exit 0
