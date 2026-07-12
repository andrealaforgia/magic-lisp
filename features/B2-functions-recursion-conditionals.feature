Feature: B2 — Define and call functions, including recursion, guarded by a conditional
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want named functions, recursion, conditionals, and the operators needed to write real programs like factorial
  So that I can build on B1's read-compile-run pipeline with actual language semantics

  # Builds on B1: read -> compile -> run -> correct stdout/exit code. This slice adds
  # named functions, calling them (including self-calls), conditionals, and the special
  # forms/operators needed for programs like factorial. No other special forms, data
  # types, or operators beyond those named below are in scope. Internal representation,
  # calling convention, and control-flow encoding are not observable and not part of
  # this behaviour.

  Scenario: E1 — quote and its shorthand evaluate to the literal datum, unevaluated
    Given the expressions "(display (quote (+ 1 2)))" and "(display '(+ 1 2))"
    When each is evaluated
    Then both display the literal list "(+ 1 2)", not the number "3"

  Scenario: E2 — if with and without an else branch
    Given the four combinations of a true/false condition with a two-branch or one-branch if
    When each is evaluated
    Then (if #t "then" "else") yields "then", (if #f "then" "else") yields "else",
      (if #t "then") yields "then", and (if #f "then") yields the unspecified value
      which produces no visible output when displayed
    And all four exit 0

  Scenario: E3 — define binds values and functions with flexible parameter lists
    Given a top-level value binding, a fixed-arity function, a fixed-plus-rest function, and an all-rest function
    When each is defined and called
    Then the fixed-arity call returns the correct value, the fixed-plus-rest call collects
      the extra arguments into the rest parameter, and the all-rest call collects every
      argument into its single parameter

  Scenario: E4 — lambda produces callable values with the same parameter-list flexibility
    Given a lambda with a fixed-plus-rest formals shape, invoked immediately and also bound via define and called later
    When each is called
    Then the rest parameter correctly collects the extra arguments in both cases

  Scenario: E5 — begin runs expressions in order and yields the value of the last one
    Given "(begin (display 1) (display 2) 3)" wrapped in an outer display
    When it is evaluated
    Then the side effects appear in order and only the final expression's value (3) is the begin's own result

  Scenario: E6 — redefining a top-level name replaces it, resolved at call time not define time
    Given a function X defined first, a function A defined afterward that calls X, a call to A, then a redefinition of X, then another call to A
    When A is called before and after X's redefinition
    Then A returns the old X's result the first time and the new X's result the second time

  Scenario: E7 — a function can call itself and terminates correctly at its base case
    Given a recursive factorial function
    When it is called with an argument requiring multiple recursive steps (not just the base case)
    Then it terminates and returns the mathematically correct result

  Scenario: E8 — call arguments are evaluated left-to-right before the call is applied
    Given a function `tap` that displays its argument and returns it, called as two arguments to an outer +, itself displayed
    When "(display (+ (tap 1) (tap 2)))" is evaluated
    Then the visible output is "123" — tap(1)'s effect, then tap(2)'s effect, then the outer display of the sum

  Scenario: E9 — only #f is falsy; every other value, including 0 and the empty list, is truthy
    Given "(if 0 "truthy" "falsy")" and "(if '() "truthy" "falsy")"
    When each is evaluated
    Then both take the then-branch and yield "truthy"

  Scenario: E10 — subtraction, multiplication, and comparisons accept 2+ args, and comparisons check the whole chain
    Given `-` and `*` called with 2 and with 4+ numeric arguments, and each of `=`, `<`, `<=`, `>`, `>=`
      called with 2 args, with 4 args holding across the whole sequence, and with a
      chain-breaking case where the two endpoints alone would give the wrong answer
    When each is evaluated
    Then `-`/`*` compute the correct variadic result, and each comparison operator correctly
      reports true only when the relation holds across every adjacent pair in the sequence

  Scenario: E11 — integer overflow wraps around instead of erroring or promoting to bignum
    Given the maximum representable integer plus one
    When "(display (+ 9223372036854775807 1))" is evaluated
    Then the result wraps to the minimum representable integer, not an error and not a bignum

  Scenario: E12 — displayed values format as ordinary decimal numbers and #t/#f booleans
    Given a negative whole number and both boolean values
    When each is displayed
    Then the number prints in ordinary decimal form and the booleans print as "#t" and "#f"

  Scenario: E13 — integration: the behaviour's two verbatim demo programs produce exactly the prescribed output
    Given a program that defines `fact` twice at the top level (a stub, then the real
      recursive definition using if/</*/-), then displays (fact 10) followed by a newline
    When it is run
    Then stdout is exactly "3628800\n" and the process exits 0, proving redefinition (E6),
      recursion (E7), conditionals (E2), and variadic arithmetic (E10) all compose correctly together
    Given a program that displays the result of (if #f #f)
    When it is run
    Then no visible output is produced for that value and the process exits 0
