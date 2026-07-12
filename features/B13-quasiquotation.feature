Feature: B13 — Quasiquotation
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want a templating mechanism that treats a backquoted structure as literal data except at marked spots
  So that I have a stepping stone toward macros on top of B1-B12

  # Builds on B1-B12. Macros themselves are the next behaviour, not this one. How templates
  # are expanded internally (e.g. what code they compile down to) is not observable and not
  # part of this behaviour.

  Scenario: E1 — a template with no markers is literal data, not evaluated as code
    Given a backquoted template containing an expression that would evaluate very differently if it were code
    When it is displayed
    Then it shows the literal written structure, not the result of evaluating it

  Scenario: E2 — unquote inserts a single evaluated value in place
    Given a template with one unquote marker, a template with two separate unquote markers, and a template unquoting a variable bound to a list
    When each is displayed
    Then each marked spot is independently evaluated and substituted, and a list-valued unquote is inserted as ONE nested element, not flattened

  Scenario: E3 — unquote-splicing flattens a list value's elements directly into the surrounding list
    Given the same list-valued variable as E2, spliced instead of unquoted, plus an inline list splice, an empty-list splice, and a splice with elements on both sides
    When each is displayed
    Then the list's elements are spliced in directly (contrasting directly with E2's single-element insertion on the same value), an empty splice contributes zero elements, and surrounding elements remain correctly adjacent

  Scenario: E4 — nested quasiquote: only a marker whose level reaches zero is evaluated
    Given a doubly-nested template where a doubly-marked spot brings the nesting level to zero, and a contrasting template where a single marker only lowers the level partway
    When each is displayed
    Then the doubly-marked spot is evaluated while its surrounding inner quasiquote/unquote survive as literal tagged data, and in the contrasting case the singly-marked variable is NOT substituted at all — the level never reaches zero

  Scenario: E5 — both markers work inside a vector template
    Given a vector template with an unquote marker and a vector template with an unquote-splicing marker
    When each is displayed
    Then unquote substitutes a single value and unquote-splicing flattens a list's elements, exactly as in list templates

  Scenario: E6 — integration: all five verbatim demo expressions produce exactly the prescribed output
    Given all five DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
