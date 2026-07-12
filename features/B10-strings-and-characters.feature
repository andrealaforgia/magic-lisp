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

  Scenario: E2 — string equality and ordering, both directions
    Given equal and unequal string pairs, and pairs ordered each way
    When string=?, string<?, and string>? are applied
    Then each returns #t on its matching direction and #f on the reverse, proving genuine comparison rather than a stub

  Scenario: E3 — conversions between strings, symbols, and character lists, with a round trip
    Given a symbol, a string, a list of characters, and a round trip through string->symbol and back
    When each conversion is applied
    Then each produces the correct corresponding value, and the round trip reproduces the original string exactly

  Scenario: E4 — string case conversion, both directions, including a Unicode-aware case
    Given a lowercase string, an uppercase string, and a string containing a German sharp-s (whose uppercase form expands to two letters)
    When string-upcase and string-downcase are applied
    Then each direction produces the correct result, including correct Unicode case-folding that changes the string's length

  Scenario: E5 — character conversion, comparison, and predicates, each shown both ways
    Given characters for code-point conversion, equal and unequal character pairs, ordered pairs each way, and matching/non-matching characters for each predicate
    When char->integer, integer->char, char=?, char<?, char-alphabetic?, char-numeric?, and char-whitespace? are applied
    Then code-point conversion round-trips correctly, and every comparison/predicate is correct in BOTH directions, not just the matching case

  Scenario: E6 — character literals read correctly, verified via their code points
    Given a plain character literal and the named forms for space, newline, and tab
    When each is converted to its code point
    Then each yields the correct numeric value, unambiguously confirming the literal was read correctly

  Scenario: E7 — length and indexing count by displayed character, not by byte
    Given a string containing one plain letter and one accented (multi-byte) character
    When its length is measured and each position is retrieved
    Then the length counts exactly two characters, and each position retrieves its correct, distinct character (not swapped, not split by byte)

  Scenario: E8 — integration: all seventeen verbatim demo expressions produce exactly the prescribed output
    Given all seventeen DEMO expressions from the behaviour spec run together in one program
    When it is run
    Then each line of output matches its prescribed value exactly, and the process exits 0
