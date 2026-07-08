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
    # Evidence: $ cat b9-e1.ml
    #   (define p (cons 1 2)) (display (car p)) (display (cdr p))   ; 1, 2
    #   (set-car! p 99) (set-cdr! p 100) (display (car p)) (display (cdr p)) ; 99, 100
    #   (display (cadr (cons 1 (cons 2 3))))    ; 2  (first-of-rest)
    #   (display (cddr (cons 1 (cons 2 3))))    ; 3  (rest-of-rest)
    #   (display (caar (cons (cons 10 20) 3)))  ; 10 (first-of-first)
    #   (display (caddr (cons 1 (cons 2 (cons 3 4))))) ; 3 (3-level composition)
    #   $ magiclisp eval b9-e1.ml -> 1/2/99/100/2/3/10/3, exit 0

  Scenario: E2 — list construction, length, append, reverse, indexing, tail, and last pair
    Given lists built from a sequence of values
    When length, append, reverse, list-ref, list-tail, and last-pair are applied
    Then each returns the correct value, list-tail at 0 returns the list unchanged, and last-pair returns the final PAIR (still cons-shaped), not just the bare last element
    # Evidence: $ cat b9-e2.ml
    #   (display (length (quote (a b c))))              ; 3
    #   (display (append (list 1 2) (list 3 4)))         ; (1 2 3 4)
    #   (display (reverse (list 1 2 3)))                 ; (3 2 1)
    #   (display (list-ref (list 10 20 30) 1))           ; 20
    #   (display (list-ref (list 10 20 30) 2))           ; 30
    #   (display (list-tail (list 1 2 3) 0))             ; (1 2 3)
    #   (display (list-tail (list 1 2 3) 2))             ; (3)
    #   (display (last-pair (list 1 2 3)))               ; (3)  -- the final pair, not bare 3
    #   $ magiclisp eval b9-e2.ml -> 3/(1 2 3 4)/(3 2 1)/20/30/(1 2 3)/(3)/(3), exit 0

  Scenario: E3 — member, memv, and memq search a list at three strictness levels
    Given a list containing a compound element, searched by identity (memq), by eqv?-level (memv), and by structural equality (member)
    When each search is applied to a matching element
    Then member finds a structurally-equal-but-different-object element that memq cannot, memv is demonstrated present and correct, and all three agree on simple values
    # Evidence: $ cat b9-e3.ml
    #   (display (member 2 (list 1 2 3)))                                 ; (2 3)
    #   (display (member (list 1 2) (list (list 1 2) 3)))                 ; ((1 2) 3)  -- equal?-level finds it
    #   (display (memq (list 1 2) (list (list 1 2) 3)))                   ; #f          -- eq?-level cannot
    #   $ magiclisp eval b9-e3.ml -> (2 3)/((1 2) 3)/#f, exit 0
    #   $ magiclisp eval (display (memv 2 (list 1 2 3)))                  ; (2 3) (memv present and correct)
    #   Independently verified memv against the release binary.

  Scenario: E4 — assoc, assv, and assq search an association list at three strictness levels
    Given an association list containing a compound key, searched by identity (assq), by eqv?-level (assv), and by structural equality (assoc)
    When each search is applied to a matching key
    Then assoc finds a structurally-equal-but-different-object key that assq cannot, assv is demonstrated present and correct, and all three agree on simple keys
    # Evidence: $ cat b9-e4.ml
    #   (display (assoc 2 (list (cons 1 (quote a)) (cons 2 (quote b)))))          ; (2 . b)
    #   (display (assoc (list 1 2) (list (cons (list 1 2) (quote a)))))           ; ((1 2) . a) -- equal?-level finds it
    #   (display (assq (list 1 2) (list (cons (list 1 2) (quote a)))))            ; #f           -- eq?-level cannot
    #   $ magiclisp eval b9-e4.ml -> (2 . b)/((1 2) . a)/#f, exit 0
    #   $ magiclisp eval (display (assv 2 (list (cons 1 'a) (cons 2 'b))))        ; (2 . b) (assv present and correct)
    #   Independently verified assv against the release binary.

  Scenario: E5 — map, for-each, and filter, with for-each's side-effect-only nature proven distinct from map
    Given a function applied to one list, to two lists in parallel, and used as a side-effecting iteration, plus a predicate used to keep matching elements
    When map, for-each, and filter are each applied
    Then map produces a new list (including the two-list parallel case), filter keeps only matching elements, and for-each's own expression value is NOT a list (displays as nothing) even though its side effects still occur in order — unlike map on the same transformation
    # Evidence: $ cat b9-e5.ml
    #   (display (map (lambda (x) (* x x)) (list 1 2 3)))         ; (1 4 9)
    #   (display (map + (list 1 2 3) (list 10 20 30)))            ; (11 22 33)
    #   (display (filter odd? (list 1 2 3 4 5)))                  ; (1 3 5)
    #   (for-each (lambda (x) (display x)) (list 1 2 3)) (newline) ; 123 (side effects, in order)
    #   $ magiclisp eval b9-e5.ml -> (1 4 9)/(11 22 33)/(1 3 5)/123, exit 0
    #   Independently verified the contrast: (display (for-each (lambda (x) x) (list 1 2 3)))
    #   produces NO visible output (unspecified value), while (display (map (lambda (x) x) (list 1 2 3)))
    #   on the same list produces "(1 2 3)" — proving for-each's own value is not a transformed list.

  Scenario: E6 — fold-left, fold-right, and reduce have genuinely distinct evaluation orders
    Given a non-commutative operation folded over the same list from the left and from the right, and reduce given a non-identity initial value on both an empty and a non-empty list
    When each reduction is applied
    Then fold-left and fold-right produce different results on the same non-commutative input (proving real left/right evaluation order), and reduce ignores its initial value on a non-empty list (seeding from the list's own first element) while using it as the result for an empty list
    # Evidence: $ cat b9-e6.ml
    #   (display (fold-left + 0 (list 1 2 3 4)))              ; 10
    #   (display (fold-right cons (quote ()) (list 1 2 3)))   ; (1 2 3)
    #   (display (fold-left - 0 (list 1 2 3)))                ; -6   ((((0-1)-2)-3)
    #   (display (fold-right - 0 (list 1 2 3)))               ; 2    (1-(2-(3-0)))  -- differs from fold-left, proving order
    #   (display (reduce + 0 (list 1 2 3 4)))                 ; 10
    #   (display (reduce + 99 (quote ())))                    ; 99   (empty-list fallback to initial value)
    #   $ magiclisp eval b9-e6.ml -> 10/(1 2 3)/-6/2/10/99, exit 0
    #   Independently verified the self-seeding property: (reduce + 99 (list 1 2 3)) -> 6
    #   (ignores 99 entirely, seeds from 1) vs (fold-left + 99 (list 1 2 3)) -> 105
    #   (uses 99 as the true starting accumulator) — same non-empty list, same initial value,
    #   different mechanism proven.

  Scenario: E7 — apply flattens direct arguments plus a trailing list, at both edges
    Given a function called with two direct arguments plus a trailing list, with just a trailing list, and with an empty trailing list
    When apply is used in each case
    Then all arguments are passed as one flat set regardless of how many came directly versus from the list
    # Evidence: $ cat b9-e7.ml
    #   (display (apply + 1 2 (list 3 4)))    ; 10
    #   (display (apply + (list 1 2 3)))      ; 6   (zero direct arguments)
    #   (display (apply + 1 2 (list)))        ; 3   (empty trailing list)
    #   $ magiclisp eval b9-e7.ml -> 10/6/3, exit 0

  Scenario: E8 — quoted list literals read to exactly the structure written, including nested and dotted forms
    Given a quoted literal containing a nested list, and quoted dotted (improper) pair literals
    When the literals are read and inspected
    Then the nested literal is structurally identical to the equivalent hand-built cons structure and its nested part is reachable, and the dotted literals display and are recognized as improper (not proper lists)
    # Evidence: $ cat b9-e8.ml
    #   (display (equal? (quote (1 (2 3) 4)) (cons 1 (cons (cons 2 (cons 3 (quote ()))) (cons 4 (quote ())))))) ; #t
    #   (display (car (cadr (quote (1 (2 3) 4)))))    ; 2  (reaches into the nested list)
    #   (display (quote (a . b)))                     ; (a . b)
    #   (display (quote (1 2 . 3)))                   ; (1 2 . 3)
    #   (display (list? (quote (1 2 . 3))))           ; #f (correctly recognized as improper)
    #   $ magiclisp eval b9-e8.ml -> #t/2/(a . b)/(1 2 . 3)/#f, exit 0

  Scenario: E9 — integration: all fourteen verbatim demo expressions produce exactly the prescribed output
    Given all fourteen DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
    # Evidence: $ cat b9-e9.ml (all 14 demo expressions, each displayed then newlined)
    #   $ magiclisp eval b9-e9.ml ->
    #   1 / 2 / 3 / (1 2 3 4) / (3 2 1) / (1 4 9) / (11 22 33) / (1 3 5) / 10 / (1 2 3) / 10 / 10 / (2 . b) / (2 3)
    #   exit 0
