Feature: B10 — Strings and characters
  As a user running a MagicLisp program through the `magiclisp` CLI
  I want to measure, index, slice, join, compare, and case-convert strings, convert between strings/symbols/character lists, and inspect/convert characters
  So that strings and characters become fully usable on top of B1-B9

  # Builds on B1-B9. Strings do not change in place once created. No string/character
  # operations beyond those named below are in scope. How strings/characters are stored or
  # encoded internally is not observable and not part of this behaviour — only that
  # indexing and length behave correctly by displayed character.

  Scenario: E1 — length, indexing, sub-range extraction, joining, and out-of-bounds errors
    Given a string, a position within it, a sub-range within it, three or more strings to join, and positions/ranges outside its bounds
    When length, character retrieval, sub-range extraction, and joining are applied
    Then each returns the correct value, three-or-more-string joining works, and out-of-bounds retrieval/extraction is a clean runtime error
    # Evidence: $ cat b10-e1.ml
    #   (display (string-length "hello"))           ; 5
    #   (display (string-ref "hello" 1))             ; e
    #   (display (substring "hello" 1 4))            ; ell
    #   (display (string-append "foo" "bar" "baz"))  ; foobarbaz  (3+ strings)
    #   $ magiclisp eval b10-e1.ml -> 5/e/ell/foobarbaz, exit 0
    #   $ magiclisp eval b10-e1-oob-ref.ml   ; (display (string-ref "hello" 5))
    #   -> "error: runtime error: string-ref index 5 is out of range", exit 70
    #   $ magiclisp eval b10-e1-oob-sub.ml   ; (display (substring "hello" 1 10))
    #   -> "error: runtime error: substring range 1..10 is out of bounds", exit 70

  Scenario: E2 — string equality and ordering, both directions
    Given equal and unequal string pairs, and pairs ordered each way
    When string=?, string<?, and string>? are applied
    Then each returns #t on its matching direction and #f on the reverse, proving genuine comparison rather than a stub
    # Evidence: $ cat b10-e2.ml
    #   (display (string=? "abc" "abc"))  ; #t
    #   (display (string=? "abc" "abd"))  ; #f
    #   (display (string<? "abc" "abd"))  ; #t
    #   (display (string<? "abd" "abc"))  ; #f
    #   (display (string>? "abd" "abc"))  ; #t
    #   (display (string>? "abc" "abd"))  ; #f
    #   $ magiclisp eval b10-e2.ml -> #t/#f/#t/#f/#t/#f, exit 0

  Scenario: E3 — conversions between strings, symbols, and character lists, with a round trip
    Given a symbol, a string, a list of characters, and a round trip through string->symbol and back
    When each conversion is applied
    Then each produces the correct corresponding value, and the round trip reproduces the original string exactly
    # Evidence: $ cat b10-e3.ml
    #   (display (symbol->string (quote hello)))                        ; hello
    #   (display (string->symbol "world"))                              ; world
    #   (display (list->string (list #\h #\i)))                         ; hi
    #   (display (string->list "ab"))                                   ; (a b)
    #   (display (symbol->string (string->symbol "round-trip")))        ; round-trip
    #   $ magiclisp eval b10-e3.ml -> hello/world/hi/(a b)/round-trip, exit 0

  Scenario: E4 — string case conversion, both directions, including a Unicode-aware case
    Given a lowercase string, an uppercase string, and a string containing a German sharp-s (whose uppercase form expands to two letters)
    When string-upcase and string-downcase are applied
    Then each direction produces the correct result, including correct Unicode case-folding that changes the string's length
    # Evidence: $ cat b10-e4.ml
    #   (display (string-upcase "abc"))       ; ABC
    #   (display (string-downcase "ABC"))     ; abc
    #   (display (string-upcase "straße"))    ; STRASSE  (ß -> SS, Unicode-correct case folding)
    #   $ magiclisp eval b10-e4.ml -> ABC/abc/STRASSE, exit 0

  Scenario: E5 — character conversion, comparison, and predicates, each shown both ways
    Given characters for code-point conversion, equal and unequal character pairs, ordered pairs each way, and matching/non-matching characters for each predicate
    When char->integer, integer->char, char=?, char<?, char-alphabetic?, char-numeric?, and char-whitespace? are applied
    Then code-point conversion round-trips correctly, and every comparison/predicate is correct in BOTH directions, not just the matching case
    # Evidence: $ cat b10-e5.ml
    #   (display (char->integer #\A))          ; 65
    #   (display (integer->char 66))           ; B
    #   (display (char=? #\a #\a))             ; #t
    #   (display (char=? #\a #\b))             ; #f
    #   (display (char<? #\a #\b))             ; #t
    #   (display (char-alphabetic? #\a))       ; #t
    #   (display (char-numeric? #\5))          ; #t
    #   (display (char-whitespace? #\space))   ; #t
    #   (display (char-whitespace? #\a))       ; #f
    #   $ magiclisp eval b10-e5.ml -> 65/B/#t/#f/#t/#t/#t/#t/#f, exit 0
    #   Independently re-verified the non-matching direction against the release binary:
    #   (char-alphabetic? #\5) -> #f, (char-numeric? #\a) -> #f, (char<? #\b #\a) -> #f

  Scenario: E6 — character literals read correctly, verified via their code points
    Given a plain character literal and the named forms for space, newline, and tab
    When each is converted to its code point
    Then each yields the correct numeric value, unambiguously confirming the literal was read correctly
    # Evidence: $ cat b10-e6.ml
    #   (display (char->integer #\a))         ; 97
    #   (display (char->integer #\space))     ; 32
    #   (display (char->integer #\newline))   ; 10
    #   (display (char->integer #\tab))       ; 9
    #   $ magiclisp eval b10-e6.ml -> 97/32/10/9, exit 0

  Scenario: E7 — length and indexing count by displayed character, not by byte
    Given a string containing one plain letter and one accented (multi-byte) character
    When its length is measured and each position is retrieved
    Then the length counts exactly two characters, and each position retrieves its correct, distinct character (not swapped, not split by byte)
    # Evidence: $ cat b10-e7.ml
    #   (display (string-length "aé"))    ; 2
    #   (display (string-ref "aé" 0))     ; a
    #   (display (string-ref "aé" 1))     ; é
    #   $ magiclisp eval b10-e7.ml -> 2/a/é, exit 0

  Scenario: E8 — integration: all seventeen verbatim demo expressions produce exactly the prescribed output
    Given all seventeen DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
    # Evidence: $ cat b10-e8.ml (all 17 demo expressions, each displayed then newlined)
    #   $ magiclisp eval b10-e8.ml ->
    #   5 / e / ell / foobar / #t / #t / ABC / hello / world / 65 / B / #t / #t / hi / (a b) / 2 / é
    #   exit 0
