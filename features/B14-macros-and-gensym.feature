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
    # Evidence: $ cat e1a.ml
    #   (define-macro (show-literally x) `(quote ,x))
    #   (display (show-literally (undefined-function 1 2))) (newline)
    #   $ magiclisp eval e1a.ml -> (undefined-function 1 2), exit 0
    #   (would have errored if evaluated eagerly at the call site)
    #   $ cat e1b.ml
    #   (define-macro (collect . rest) `(quote ,rest))
    #   (display (collect (a 1) (b 2) (c 3))) (newline)
    #   $ magiclisp eval e1b.ml -> ((a 1) (b 2) (c 3)), exit 0

  Scenario: E2 — the macro's expansion is itself evaluated, and macros are visible in later-defined function bodies
    Given a macro expanding to an arithmetic expression, used inside a function defined after the macro
    When the function is called
    Then real arithmetic happens on the expanded code, proving both genuine evaluation and forward visibility into the later-defined function body
    # Evidence: $ cat e2.ml
    #   (define-macro (double x) `(* ,x 2))
    #   (define (use-it n) (double n))
    #   (display (use-it 5)) (newline)
    #   $ magiclisp eval e2.ml -> 10, exit 0

  Scenario: E3 — recursive macro expansion is bounded at a floor of at least 1000 rounds
    Given a macro that expands into another macro call (two legitimate rounds), a macro engineered to expand into itself forever, and a macro that legitimately re-expands 500 times before settling
    When each is compiled and run
    Then the two-round case completes correctly, the infinite case fails cleanly with a distinct non-zero exit code at a limit of at least 1000 (not a hang or crash), and the 500-round legitimate case completes successfully — proving the raised ceiling supports real additional rounds, not just a relocated failure boundary
    # Evidence: $ cat e3a.ml
    #   (define-macro (my-when test . body) `(if ,test (begin ,@body) #f))
    #   (define-macro (my-unless test . body) `(my-when (not ,test) ,@body))
    #   (my-unless #f (display "hi")) (newline)
    #   $ magiclisp eval e3a.ml -> hi, exit 0 (two rounds: my-unless -> my-when -> if)
    #   $ cat e3b.ml
    #   (define-macro (loop-forever) `(loop-forever))
    #   (loop-forever)
    #   $ magiclisp eval e3b.ml
    #   -> "error: compile error: macro expansion exceeded the maximum supported rounds (1000)
    #      -- possible infinite macro recursion", exit 65
    #   $ cat e3c.ml
    #   (define-macro (count-down n) (if (= n 0) 1 (list (quote count-down) (- n 1))))
    #   (display (count-down 500)) (newline)
    #   $ magiclisp eval e3c.ml -> 1, exit 0 (re-expands 501 times, well under the 1000 ceiling)
    #   Independently re-verified the runaway-failure and 500-round-success cases against the release binary.

  Scenario: E4 — gensym produces symbols distinct from every other symbol, source-written or generated
    Given two separate gensym calls, and a gensym result compared against an ordinary source-written symbol
    When identity is checked in each case
    Then both comparisons report unequal, proving the uniqueness guarantee is genuinely global, not just relative to other gensym calls
    # Evidence: $ cat e4a.ml: (display (eq? (gensym) (gensym))) (newline)
    #   $ magiclisp eval e4a.ml -> #f, exit 0
    #   $ cat e4b.ml: (display (eq? (gensym) (quote g1))) (newline)
    #   $ magiclisp eval e4b.ml -> #f, exit 0

  Scenario: E5 — a local variable shadowing a macro name wins over the macro within that scope
    Given a macro name also bound as an ordinary function parameter, called with a procedure argument
    When the parameter name is used as an operator inside the function body
    Then the local parameter's value is used (its operand IS evaluated normally, unlike the macro's own unevaluated-operand behavior) — the macro never triggers within that scope
    # Evidence: $ cat e5.ml
    #   (define-macro (trap x) `(quote ,x))
    #   (define (f trap) (trap 1))
    #   (display (f (lambda (n) (+ n 100)))) (newline)
    #   $ magiclisp eval e5.ml -> 101, exit 0
    #   (if the macro had won, (trap 1) would expand to (quote 1) and display 1 without ever
    #   calling the passed-in procedure; instead the local parameter's procedure is genuinely
    #   called with its operand evaluated first)

  Scenario: E6 — the swap macro uses gensym internally to avoid colliding with its own operands
    Given two variables and a swap macro that generates its own temporary name via gensym
    When the variables are swapped via the macro and their new values printed
    Then they are correctly swapped, proving macro definition, unevaluated-operand handling, generated-code evaluation, and gensym all work together for a realistic macro
    # Evidence: $ cat e6.ml
    #   (define-macro (swap a b)
    #     (let ((tmp (gensym)))
    #       `(let ((,tmp ,a)) (set! ,a ,b) (set! ,b ,tmp))))
    #   (define x 1) (define y 2)
    #   (swap x y)
    #   (write (list x y)) (newline)
    #   $ magiclisp eval e6.ml -> (2 1), exit 0

  Scenario: E7 — integration: all four verbatim demo expressions produce exactly the prescribed output
    Given all four DEMOs from the behaviour spec run together in one program, in order
    When it is run
    Then each produces exactly its prescribed output, and the process exits 0
    # Evidence: $ cat e7.ml (swap macro demo, double macro demo, my-unless demo, gensym-identity demo, in sequence)
    #   $ magiclisp eval e7.ml ->
    #   (2 1) / 10 / hi / #f
    #   exit 0
