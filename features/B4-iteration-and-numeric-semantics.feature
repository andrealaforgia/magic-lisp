Feature: B4 — General iteration and correct numeric semantics
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want a general-purpose iteration form and correct integer/float reading, printing, and division rules
  So that this iteration's language core is complete on top of B1-B3

  # Builds on B1-B3. How numbers are represented internally, how the iteration form is
  # compiled, or the float-formatting/radix-parsing algorithm used are not observable
  # from outside the CLI and are not part of this behaviour.

  Scenario: E1 — a general iteration form with loop variables, a step, a test, and a result
    Given a do-style loop with variables i and s, i stepping by 1 and s accumulating i each pass, stopping when i reaches 5
    When "(display (do ((i 0 (+ i 1)) (s 0 (+ s i))) ((= i 5) s)))" is evaluated
    Then it displays "10"

  Scenario: E2 — number literals are recognised as float or integer, including radix prefixes
    Given literals in plain decimal, exponent, hex, binary, and octal forms
    When each is read and displayed
    Then each shows the correct value: a decimal point or exponent yields a float, radix-prefixed digits yield the correct integer

  Scenario: E3 — an integer literal too large for the integer range is a read error
    Given an integer literal one digit past the maximum representable integer
    When it is read
    Then it is rejected with a read error and the source-error exit code, not a silent wrap or crash

  Scenario: E4 — float display formatting rules
    Given a variety of float values: a non-round-trip-trivial decimal, a whole-number float,
      an ordinary-magnitude float, a very large and a very small-magnitude float, the three
      IEEE special values, and negative zero versus positive zero
    When each is displayed
    Then each prints per the formatting rules: shortest round-trip decimal text; a trailing
      ".0" for whole-number floats; plain decimal within the ordinary range and an alternate
      form outside it; a recognisable dedicated form for +inf/-inf/nan; negative zero
      distinct from positive zero

  Scenario: E5 — integer arithmetic overflow wraps around
    Given the maximum representable integer plus one
    When "(display (+ 9223372036854775807 1))" is evaluated
    Then the result wraps to the minimum representable integer, not an error and not a bignum

  Scenario: E6 — +, -, *, / variadic arg-count edge cases
    Given zero-argument and single-argument calls to each of +, -, *, /
    When each is evaluated
    Then (+) yields 0, (*) yields 1, (-) and (/) with zero arguments are errors,
      (- x) negates x, and (/ x) inverts x

  Scenario: E7 — division's integer-vs-float result rule, every branch
    Given exact whole-number division, inexact whole-number division, a whole number divided
      by a float, an integer divided by exact zero, and a float divided by zero
    When each is evaluated
    Then exact whole-number division yields an integer, inexact division yields a float,
      any float operand yields a float even when exact, integer-divided-by-zero is a runtime
      failure with a distinct exit code, and float-divided-by-zero succeeds per IEEE rules

  Scenario: E8 — integration: all six verbatim demo programs produce exactly the prescribed output
    Given each of the six DEMO programs from the behaviour spec
    When each is run
    Then each produces exactly its prescribed output followed by a trailing newline, and exits 0
