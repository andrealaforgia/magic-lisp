Feature: B14 — Procedural macros and gensym
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want to define new syntax via macros that receive unevaluated operands and build replacement code, plus a fresh-symbol generator
  So that I can complete the macros & quasiquotation iteration on top of B1-B13

  # Builds on B1-B13 (quasiquotation is the natural tool for building macro output). This
  # completes the macros & quasiquotation iteration — no further metaprogramming facilities
  # are in scope here. How macro expansion or fresh-symbol generation is implemented
  # internally is not observable and not part of this behaviour.

  Scenario: E1 — a macro's operands are handed to its body as literal, unevaluated data
    Given a macro whose operand references an undefined name, and a macro with a rest parameter given several trailing operand forms
    When each macro is called
    Then the undefined-name operand is returned as literal data without erroring (proving it was never evaluated), and all trailing forms are collected as unevaluated data via the rest parameter

  Scenario: E2 — the macro's expansion is itself evaluated, and macros are visible in later-defined function bodies
    Given a macro expanding to an arithmetic expression, used inside a function defined after the macro
    When the function is called
    Then real arithmetic happens on the expanded code, proving both genuine evaluation and forward visibility into the later-defined function body

  Scenario: E3 — recursive macro expansion is bounded at a floor of at least 1000 rounds
    Given a macro that expands into another macro call (two legitimate rounds), a macro engineered to expand into itself forever, and a macro that legitimately re-expands 500 times before settling
    When each is compiled and run
    Then the two-round case completes correctly, the infinite case fails cleanly with a distinct non-zero exit code at a limit of at least 1000 (not a hang or crash), and the 500-round legitimate case completes successfully — proving the raised ceiling supports real additional rounds, not just a relocated failure boundary

  Scenario: E4 — gensym produces symbols distinct from every other symbol, source-written or generated
    Given two separate gensym calls, and a gensym result compared against an ordinary source-written symbol
    When identity is checked in each case
    Then both comparisons report unequal, proving the uniqueness guarantee is genuinely global, not just relative to other gensym calls

  Scenario: E5 — a local variable shadowing a macro name wins over the macro within that scope
    Given a macro name also bound as an ordinary function parameter, called with a procedure argument
    When the parameter name is used as an operator inside the function body
    Then the local parameter's value is used (its operand IS evaluated normally, unlike the macro's own unevaluated-operand behavior) — the macro never triggers within that scope

  Scenario: E6 — the swap macro uses gensym internally to avoid colliding with its own operands
    Given two variables and a swap macro that generates its own temporary name via gensym
    When the variables are swapped via the macro and their new values printed
    Then they are correctly swapped, proving macro definition, unevaluated-operand handling, generated-code evaluation, and gensym all work together for a realistic macro

  Scenario: E7 — integration: all four verbatim demo expressions produce exactly the prescribed output
    Given all four DEMOs from the behaviour spec run together in one program, in order
    When it is run
    Then each produces exactly its prescribed output, and the process exits 0
