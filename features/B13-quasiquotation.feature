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
    # Evidence: $ cat e1.ml: (display `(+ 1 2))
    #   $ magiclisp eval e1.ml -> (+ 1 2), exit 0

  Scenario: E2 — unquote inserts a single evaluated value in place
    Given a template with one unquote marker, a template with two separate unquote markers, and a template unquoting a variable bound to a list
    When each is displayed
    Then each marked spot is independently evaluated and substituted, and a list-valued unquote is inserted as ONE nested element, not flattened
    # Evidence: $ cat e2a.ml: (define x 10) (display `(a ,x c))
    #   $ magiclisp eval e2a.ml -> (a 10 c), exit 0
    #   $ cat e2b.ml: (define x 1) (define y 2) (display `(,x mid ,y))
    #   $ magiclisp eval e2b.ml -> (1 mid 2), exit 0 (two independent markers)
    #   $ cat e2c.ml: (define mid (list 2 3 4)) (display `(1 ,mid 5))
    #   $ magiclisp eval e2c.ml -> (1 (2 3 4) 5), exit 0 (list inserted as ONE nested element)

  Scenario: E3 — unquote-splicing flattens a list value's elements directly into the surrounding list
    Given the same list-valued variable as E2, spliced instead of unquoted, plus an inline list splice, an empty-list splice, and a splice with elements on both sides
    When each is displayed
    Then the list's elements are spliced in directly (contrasting directly with E2's single-element insertion on the same value), an empty splice contributes zero elements, and surrounding elements remain correctly adjacent
    # Evidence: $ cat e3a.ml: (define mid (list 2 3 4)) (display `(1 ,@mid 5))
    #   $ magiclisp eval e3a.ml -> (1 2 3 4 5), exit 0 (contrast directly against E2c's `(1 ,mid 5)` -> (1 (2 3 4) 5) on the SAME mid)
    #   $ cat e3b.ml: (display `(1 ,@(list 2 3) 4))
    #   $ magiclisp eval e3b.ml -> (1 2 3 4), exit 0
    #   $ cat e3c.ml: (display `(1 ,@(list) 2))
    #   $ magiclisp eval e3c.ml -> (1 2), exit 0 (empty splice: zero elements, no stray empty list)
    #   $ cat e3d.ml: (display `(0 1 ,@(list 2 3) 4 5))
    #   $ magiclisp eval e3d.ml -> (0 1 2 3 4 5), exit 0 (elements on both sides)

  Scenario: E4 — nested quasiquote: only a marker whose level reaches zero is evaluated
    Given a doubly-nested template where a doubly-marked spot brings the nesting level to zero, and a contrasting template where a single marker only lowers the level partway
    When each is displayed
    Then the doubly-marked spot is evaluated while its surrounding inner quasiquote/unquote survive as literal tagged data, and in the contrasting case the singly-marked variable is NOT substituted at all — the level never reaches zero
    # Evidence: $ cat e4a.ml: (define y 5) (display `(a `(b ,,y)))
    #   $ magiclisp eval e4a.ml -> (a (quasiquote (b (unquote 5)))), exit 0
    #   (the doubly-marked y reaches level 0 and IS evaluated; the surrounding inner backquote/marker survive as literal data)
    #   $ cat e4b.ml: (define y 5) (display `(a `(b ,y)))
    #   $ magiclisp eval e4b.ml -> (a (quasiquote (b (unquote y)))), exit 0
    #   (contrast: a single comma only lowers level 2->1, not to 0 — y itself is never substituted, shows as the bare symbol y)
    #   Independently re-verified both cases against the release binary.

  Scenario: E5 — both markers work inside a vector template
    Given a vector template with an unquote marker and a vector template with an unquote-splicing marker
    When each is displayed
    Then unquote substitutes a single value and unquote-splicing flattens a list's elements, exactly as in list templates
    # Evidence: $ cat e5a.ml: (define x 10) (display `#(1 ,x 3))
    #   $ magiclisp eval e5a.ml -> #(1 10 3), exit 0
    #   $ cat e5b.ml: (display `#(1 ,@(list 2 3) 4))
    #   $ magiclisp eval e5b.ml -> #(1 2 3 4), exit 0

  Scenario: E6 — integration: all five verbatim demo expressions produce exactly the prescribed output
    Given all five DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
    # Evidence: $ cat e6.ml
    #   (define mid (list 2 3 4)) (write `(1 ,@mid 5)) (newline)
    #   (define x 10) (display `(a ,x c)) (newline)
    #   (display `(1 ,@(list 2 3) 4)) (newline)
    #   (display `#(1 ,x 3)) (newline)
    #   (define y 5) (display `(a `(b ,,y))) (newline)
    #   $ magiclisp eval e6.ml ->
    #   (1 2 3 4 5) / (a 10 c) / (1 2 3 4) / #(1 10 3) / (a (quasiquote (b (unquote 5))))
    #   exit 0
