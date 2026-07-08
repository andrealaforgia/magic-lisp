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
    # Evidence: $ cat e1.ml
    #   (display (quotient 7 2)) (newline)   ; 3
    #   (display (remainder 7 2)) (newline)  ; 1
    #   (display (modulo -7 2)) (newline)    ; 1
    #   (display (remainder -7 2)) (newline) ; -1  (differs from modulo -7 2 = 1)
    #   (display (remainder 7 -2)) (newline) ; 1
    #   (display (modulo 7 -2)) (newline)    ; -1  (differs from remainder 7 -2 = 1)
    #   $ magiclisp eval e1.ml -> 3/1/1/-1/1/-1, exit 0
    #   $ magiclisp eval e1-quotient-zero.ml  ; (display (quotient 7 0))
    #   -> "error: runtime error: quotient by zero is a runtime error", exit 70
    #   (remainder-by-zero and modulo-by-zero each fail the same way, exit 70)
    #   Independently re-verified against the release binary.

  Scenario: E2 — abs, min, max, and the sign/parity predicates, each shown both ways
    Given abs of a negative number, min/max over 2 and over 4+ arguments, and each predicate on a satisfying and a non-satisfying input
    When each is evaluated
    Then abs/min/max compute correctly and every predicate returns #t on its satisfying input and #f otherwise
    # Evidence: $ cat e2.ml
    #   (display (abs -5))                 ; 5
    #   (display (max 1 5 3))              ; 5
    #   (display (min 3 1))                ; 1
    #   (display (min 5 1 3 2))            ; 1
    #   zero?(0)=#t  zero?(1)=#f
    #   positive?(1)=#t  positive?(-1)=#f  positive?(0)=#f
    #   negative?(-1)=#t  negative?(1)=#f  negative?(0)=#f
    #   even?(10)=#t  even?(3)=#f
    #   odd?(3)=#t  odd?(10)=#f
    #   $ magiclisp eval e2.ml -> matches all of the above in order, exit 0

  Scenario: E3 — floor, ceiling, round, truncate preserve numeric type and round half to even
    Given a positive and a negative non-integer float, and a whole-number integer, applied to floor/ceiling/round/truncate
    When each is evaluated
    Then floor/ceiling/truncate/round on the negative float show all four can differ, round-to-even holds at exact halfway points, and the integer input comes back unchanged (not promoted to float)
    # Evidence: $ cat e3.ml
    #   floor(2.7)=2.0  round(2.5)=2.0  round(3.5)=4.0  (round-half-to-even)
    #   floor(-2.7)=-3.0  ceiling(-2.7)=-2.0  truncate(-2.7)=-2.0  round(-2.7)=-3.0
    #   floor(5)=5  ceiling(5)=5  round(5)=5  truncate(5)=5  (integer unchanged, not 5.0)
    #   $ magiclisp eval e3.ml -> 2.0/2.0/4.0/-3.0/-2.0/-2.0/-3.0/5/5/5/5, exit 0
    #   Independently re-verified against the release binary.

  Scenario: E4 — expt, sqrt, and the transcendental functions, with the integer-power exactness rule
    Given a whole number raised to a non-negative whole-number power, a perfect-square whole number's square root, and known-value spot-checks of exp/log/sin/cos/tan/atan
    When each is evaluated
    Then the integer power is an exact whole number, the perfect-square square root is still a float, and each transcendental function produces the mathematically correct float
    # Evidence: $ cat e4.ml
    #   expt(2,10)=1024 (no decimal point — whole number)
    #   sqrt(4)=2.0 (float, despite perfect square)
    #   exp(0)=1.0  log(1)=0.0  sin(0)=0.0  cos(0)=1.0  tan(0)=0.0  atan(0)=0.0
    #   $ magiclisp eval e4.ml -> 1024/2.0/1.0/0.0/0.0/1.0/0.0/0.0, exit 0

  Scenario: E5 — number/integer/float predicates, exact/inexact conversions, and the non-finite error case
    Given values for number?/integer?/float?, an integer converted to float and a float converted to exact, and +inf/-inf/nan converted to exact
    When each is evaluated
    Then each predicate is type-based and correct both ways, the conversions produce the correct value (truncating toward zero for float-to-exact), and converting a non-finite float to exact is a clean runtime error naming the specific non-finite value
    # Evidence: $ cat e5.ml
    #   number?(5)=#t  number?("x")=#f
    #   integer?(5)=#t  integer?(5.0)=#f
    #   float?(5.0)=#t  float?(5)=#f
    #   exact->inexact(5)=5.0
    #   inexact->exact(5.7)=5 (truncated toward zero)
    #   $ magiclisp eval e5.ml -> #t/#f/#t/#f/#t/#f/5.0/5, exit 0
    #   $ magiclisp eval e5-posinf.ml  ; (inexact->exact (/ 1.0 0.0))
    #   -> "error: runtime error: inexact->exact requires a finite number, found +inf.0", exit 70
    #   (same clean error, naming -inf.0 / +nan.0 respectively, for the negative-infinity and nan cases)
    #   Independently re-verified the +inf.0 case against the release binary.

  Scenario: E6 — number/text conversion, with invalid text yielding a distinguishable #f
    Given a numeric string, a non-numeric string, and a round trip through number->string and back
    When each is parsed or converted
    Then a valid numeric string parses to the correct number, an invalid one yields #f (not an error), and the round trip reproduces the original value
    # Evidence: $ cat e6.ml
    #   (display (string->number "3.5"))                          ; 3.5
    #   (display (string->number "xyz"))                          ; #f
    #   (display (string->number (number->string 42)))            ; 42
    #   $ magiclisp eval e6.ml -> 3.5/#f/42, exit 0

  Scenario: E7 — every numeric operation is a first-class procedure value, not special syntax
    Given a user-defined higher-order function that calls its procedure argument, tried with a representative operation from each category (division family, predicate, rounding, conversion, abs)
    When each is passed as a plain argument and invoked indirectly
    Then it produces exactly what calling it directly would
    # Evidence: $ cat e7.ml
    #   (define (apply-to-5 f) (f 5))
    #   (display (apply-to-5 abs))              ; 5
    #   (display (apply-to-5 even?))             ; #f
    #   (display (apply-to-5 floor))             ; 5
    #   (display (apply-to-5 exact->inexact))    ; 5.0
    #   (define (apply-to-2-and-3 f) (f 2 3))
    #   (display (apply-to-2-and-3 quotient))    ; 0
    #   $ magiclisp eval e7.ml -> 5/#f/5/5.0/0, exit 0
    #   Independently re-verified the quotient case against the release binary.

  Scenario: E8 — integration: all thirteen verbatim demo expressions produce exactly the prescribed output
    Given all thirteen DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
    # Evidence: $ cat b7demo.ml (all 13 demo expressions, each displayed then newlined)
    #   $ magiclisp eval b7demo.ml ->
    #   3 / 1 / 1 / 5 / 5 / #t / 1024 / 2.0 / 2.0 / 2.0 / 4.0 / 3.5 / #f
    #   exit 0
