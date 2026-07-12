Feature: B8 — Type predicates and the three equality relations
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want to ask what kind of value something is and compare values at three levels of strictness
  So that I can write correct comparisons and type checks on top of B1-B7

  # Builds on B1-B7. No predicates or equality relations beyond those named below are in
  # scope. How values are represented or compared internally is not observable and not
  # part of this behaviour — only that each named predicate/relation exists as a callable
  # procedure and produces the correct result.

  Scenario: E1 — eq? is genuine object identity, not structural sameness
    Given two separately-written same-named symbols, two separately-built pairs with identical contents, the same pair bound to two names, two separately-built strings with identical contents, and the same string bound to two names
    When eq? is applied to each pair
    Then simple values (symbols) compare equal when they're the same value, while separately-built compound values compare unequal and only the literally-same object compares equal

  Scenario: E2 — eqv? compares floats by value, with NaN-equal and signed-zero-unequal wrinkles
    Given an integer and a float of the same magnitude, positive and negative zero, two independently-computed equal floats, and two NaN floats
    When eqv? is applied to each pair
    Then an integer never compares equal to a float, positive and negative zero compare unequal, two NaNs compare equal to each other, and two independently-computed equal floats compare equal

  Scenario: E3 — equal? recurses into pairs/vectors/strings and falls back to eqv? otherwise
    Given two separately-built lists with identical contents, two separately-built strings with identical contents, two separately-built nested lists (a list containing a list), an integer vs a float of the same magnitude, and a large non-circular list built two separate times
    When equal? is applied to each pair
    Then structurally identical containers (including nested ones) compare equal, non-container values fall back to eqv? semantics (so an integer still never equals a float), and the large structure completes without hanging

  Scenario: E4 — not returns true for exactly false, false for everything else
    Given false, the truthy whole number 0, and the truthy empty list
    When not is applied to each
    Then only false yields true; every other value, regardless of type, yields false

  Scenario: E5 — the ten type predicates are correct in both directions
    Given a matching and a non-matching value for each of: empty-list, pair, proper list, symbol, string, character, boolean, procedure, vector, hash table
    When each predicate is applied to its matching and non-matching value
    Then every predicate returns #t on the matching value and #f on the non-matching one, including a proper list returning #f for an improper (dotted) structure

  Scenario: E6 — integration: all twelve verbatim demo expressions produce exactly the prescribed output
    Given all twelve DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
