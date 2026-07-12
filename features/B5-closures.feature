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

  Scenario: E2 — captured variables are shared live storage, not a value snapshot
    Given a factory returning a getter closure and a setter closure over one shared local
    When the setter is called with a value and then the getter is called
    Then the getter observes the value the setter wrote

  Scenario: E3 — each call to the creator function produces a fresh, independent variable
    Given two independent closures created from two separate calls to the same counter-factory function
    When calls to the two closures are interleaved (first counter twice, then second counter once)
    Then each counter's state is independent of the other's

  Scenario: E4 — pairs can be constructed and each half retrieved back out correctly
    Given a pair constructed from two distinguishable values
    When each half is retrieved
    Then each half matches its original position, not swapped or merged

  Scenario: E5 — integration: both verbatim demo programs produce exactly the prescribed output
    Given the counter-factory DEMO and the pair-factory DEMO from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output followed by a trailing newline, and exits 0
