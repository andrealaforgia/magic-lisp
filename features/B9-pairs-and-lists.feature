Feature: B9 — Pairs and lists
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want to build, take apart, search, transform, and reduce lists, and apply a function to a list of arguments
  So that lists become a fully usable data structure on top of B1-B8

  # Builds on B1-B8 (pairs already exist minimally from B5). Converting a list to a vector
  # or to a string is explicitly deferred to later behaviours — not in scope here. How
  # lists/pairs are represented or traversed internally is not observable and not part of
  # this behaviour.

  Scenario: E1 — pair mutation and multi-level accessor composition
    Given a constructed pair, and nested pairs reachable via 2- and 3-level accessor composition
    When each half of the pair is mutated in place, and the accessors are applied to the nested pairs
    Then the mutations are observed afterward, and each accessor correctly reaches the value at its composed depth

  Scenario: E2 — list construction, length, append, reverse, indexing, tail, and last pair
    Given lists built from a sequence of values
    When length, append, reverse, list-ref, list-tail, and last-pair are applied
    Then each returns the correct value, list-tail at 0 returns the list unchanged, and last-pair returns the final PAIR (still cons-shaped), not just the bare last element

  Scenario: E3 — member, memv, and memq search a list at three strictness levels
    Given a list containing a compound element, searched by identity (memq), by eqv?-level (memv), and by structural equality (member)
    When each search is applied to a matching element
    Then member finds a structurally-equal-but-different-object element that memq cannot, memv is demonstrated present and correct, and all three agree on simple values

  Scenario: E4 — assoc, assv, and assq search an association list at three strictness levels
    Given an association list containing a compound key, searched by identity (assq), by eqv?-level (assv), and by structural equality (assoc)
    When each search is applied to a matching key
    Then assoc finds a structurally-equal-but-different-object key that assq cannot, assv is demonstrated present and correct, and all three agree on simple keys

  Scenario: E5 — map, for-each, and filter, with for-each's side-effect-only nature proven distinct from map
    Given a function applied to one list, to two lists in parallel, and used as a side-effecting iteration, plus a predicate used to keep matching elements
    When map, for-each, and filter are each applied
    Then map produces a new list (including the two-list parallel case), filter keeps only matching elements, and for-each's own expression value is NOT a list (displays as nothing) even though its side effects still occur in order — unlike map on the same transformation

  Scenario: E6 — fold-left, fold-right, and reduce have genuinely distinct evaluation orders
    Given a non-commutative operation folded over the same list from the left and from the right, and reduce given a non-identity initial value on both an empty and a non-empty list
    When each reduction is applied
    Then fold-left and fold-right produce different results on the same non-commutative input (proving real left/right evaluation order), and reduce ignores its initial value on a non-empty list (seeding from the list's own first element) while using it as the result for an empty list

  Scenario: E7 — apply flattens direct arguments plus a trailing list, at both edges
    Given a function called with two direct arguments plus a trailing list, with just a trailing list, and with an empty trailing list
    When apply is used in each case
    Then all arguments are passed as one flat set regardless of how many came directly versus from the list

  Scenario: E8 — quoted list literals read to exactly the structure written, including nested and dotted forms
    Given a quoted literal containing a nested list, and quoted dotted (improper) pair literals
    When the literals are read and inspected
    Then the nested literal is structurally identical to the equivalent hand-built cons structure and its nested part is reachable, and the dotted literals display and are recognized as improper (not proper lists)

  Scenario: E9 — integration: all fourteen verbatim demo expressions produce exactly the prescribed output
    Given all fourteen DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
