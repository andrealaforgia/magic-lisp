Feature: B7 — The numeric library
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want integer division variants, sign/rounding/predicate operations, transcendental math, type conversions, and number/text conversion
  So that this iteration starts standard-library breadth on top of B1-B6's arithmetic

  # Builds on B1-B6. No numeric operations beyond those named in the scenarios below are
  # in scope for this behaviour. How each operation is implemented internally (algorithm,
  # precision strategy) is not observable and not part of this behaviour — only that every
  # operation exists as a callable, passable procedure and produces the correct result.

  Scenario: E1 — quotient, remainder, and modulo, including the floor-vs-truncate distinction and division by zero
    Given the same negative-dividend and negative-divisor inputs applied to remainder and modulo
    When quotient, remainder, and modulo are evaluated
    Then quotient and remainder truncate toward zero while modulo floors, giving a different sign on negative inputs
    And dividing by zero with any of the three is a clean runtime error

  Scenario: E2 — abs, min, max, and the sign/parity predicates, each shown both ways
    Given abs of a negative number, min/max over 2 and over 4+ arguments, and each predicate on a satisfying and a non-satisfying input
    When each is evaluated
    Then abs/min/max compute correctly and every predicate returns #t on its satisfying input and #f otherwise

  Scenario: E3 — floor, ceiling, round, truncate preserve numeric type and round half to even
    Given a positive and a negative non-integer float, and a whole-number integer, applied to floor/ceiling/round/truncate
    When each is evaluated
    Then floor/ceiling/truncate/round on the negative float show all four can differ, round-to-even holds at exact halfway points, and the integer input comes back unchanged (not promoted to float)

  Scenario: E4 — expt, sqrt, and the transcendental functions, with the integer-power exactness rule
    Given a whole number raised to a non-negative whole-number power, a perfect-square whole number's square root, and known-value spot-checks of exp/log/sin/cos/tan/atan
    When each is evaluated
    Then the integer power is an exact whole number, the perfect-square square root is still a float, and each transcendental function produces the mathematically correct float

  Scenario: E5 — number/integer/float predicates, exact/inexact conversions, and the non-finite error case
    Given values for number?/integer?/float?, an integer converted to float and a float converted to exact, and +inf/-inf/nan converted to exact
    When each is evaluated
    Then each predicate is type-based and correct both ways, the conversions produce the correct value (truncating toward zero for float-to-exact), and converting a non-finite float to exact is a clean runtime error naming the specific non-finite value

  Scenario: E6 — number/text conversion, with invalid text yielding a distinguishable #f
    Given a numeric string, a non-numeric string, and a round trip through number->string and back
    When each is parsed or converted
    Then a valid numeric string parses to the correct number, an invalid one yields #f (not an error), and the round trip reproduces the original value

  Scenario: E7 — every numeric operation is a first-class procedure value, not special syntax
    Given a user-defined higher-order function that calls its procedure argument, tried with a representative operation from each category (division family, predicate, rounding, conversion, abs)
    When each is passed as a plain argument and invoked indirectly
    Then it produces exactly what calling it directly would

  Scenario: E8 — integration: all thirteen verbatim demo expressions produce exactly the prescribed output
    Given all thirteen DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
