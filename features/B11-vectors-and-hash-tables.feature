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

  Scenario: E2 — vector/list conversion and whole-vector fill
    Given a mutated vector converted to a list, a list converted to a vector, an existing vector filled entirely with one value, and a round trip through both conversions
    When each operation is applied
    Then each produces the correct result, the fill operation changes every position, and the round trip reproduces the original list exactly

  Scenario: E3 — vector literals read to genuine vector values
    Given a vector literal written directly in source
    When it is displayed, checked with vector?, and indexed into
    Then it displays correctly as a whole, is recognized as a genuine vector (not merely text), and its elements are individually accessible

  Scenario: E4 — hash table CRUD, structural-equality keys, and missing-key handling
    Given an empty hash table with entries stored, retrieved, and removed by key, a compound key built separately but structurally identical to a stored one, and a missing key looked up with and without a fallback
    When each operation is applied
    Then count and presence are reported correctly, a structurally-identical but separately-built compound key still retrieves its value (equal?-based, not identity-based), a missing key with a fallback returns the fallback, and a missing key without one is a clean, distinct runtime error

  Scenario: E5 — hash table key listing is deterministic insertion order
    Given a two-entry table and a table with three insertions, a removal, and a re-insertion of the removed key
    When the key list is retrieved
    Then keys come back in first-insertion order, and a removed-then-re-inserted key lands at the end, not its original position

  Scenario: E6 — integration: all twelve verbatim demo expressions produce exactly the prescribed output
    Given all twelve DEMO expressions/sequences from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0

  Scenario: E7 — a vector made self-referential, or cyclic across pairs and vectors together, terminates cleanly instead of crashing or hanging
    Given a vector set to contain itself, and a pair and a vector each set to contain the other
    When the self-referential vector is compared to itself and displayed, and the cross-type cycle is displayed
    Then equal? terminates instead of hanging, display terminates with an ellipsis instead of crashing with a stack overflow, and the cross-type cycle terminates the same way
