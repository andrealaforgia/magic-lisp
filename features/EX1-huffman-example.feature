Feature: EX1 — canonical worked example: a Huffman compression program written in MagicLisp
  As a newcomer evaluating whether MagicLisp is usable for real programs
  I want a genuine, documented, command-line Huffman compressor/decompressor written entirely
  in MagicLisp itself
  So that I have a canonical worked example proving the language can express a real algorithm
  end to end, not just toy snippets

  # Owner-requested (not a frozen-spec behaviour; cites no SPEC section). The whole tool is
  # source in MagicLisp (examples/huffman/compress.ml, decompress.ml), executed by the existing
  # magiclisp CLI -- no new Rust code implements the algorithm. Because MagicLisp has no file
  # I/O, no raw-byte string handling, and no bitwise operators, the documented workflow bridges
  # through hex text at the shell level via `xxd`; the bit-packing itself is 100% MagicLisp
  # arithmetic. See examples/huffman/README.md for the exact commands.

  Scenario: E1 — compressing a real file from the command line produces a genuine, distinct output
    Given a real input file and the documented compress command (xxd -p -c 0 input | magiclisp eval compress.ml | xxd -r -p)
    When it is run from the command line against that file
    Then it writes a compressed output file that is a genuine transformation of the input, not a copy

  Scenario: E2 — decompressing reproduces the original input byte-for-byte, across genuinely different inputs
    Given the compressed output of a real input file and the documented decompress command
    When it is run from the command line
    Then the restored file is byte-for-byte identical to the original input, for a skewed-frequency text file, an empty file, a file containing only one distinct repeated byte value, and a file covering arbitrary binary byte values (not just printable text)

  Scenario: E3 — a skewed-frequency input compresses measurably smaller, proving genuine Huffman coding
    Given an input file with a clearly skewed byte-frequency distribution
    When it is compressed via the documented command
    Then the compressed output is measurably, substantially smaller than the original, reflecting real frequency-based variable-length coding rather than a pass-through

  Scenario: E4 — the documented run instructions are self-sufficient for a new, unaided user
    Given examples/huffman/README.md, read on its own with no other context
    When its usage section's exact commands are followed unaided
    Then a new user successfully compresses then decompresses a file, with no need to read the .ml source or ask for help, and the check fails if the documented commands themselves ever drift from what's proven to work

  Scenario: E5 — integration: the documented pipeline round-trips a real file end to end, now via a real BDD run
    Given the README's full documented pipeline (compress then decompress, exactly as written)
    When it is run end to end against a real file from the command line, executed by a real Cucumber-style step-definition runner (not just ad hoc integration tests)
    Then the restored file is byte-for-byte identical to the original, demonstrating the whole example (algorithm + CLI + docs) works as one coherent, usable deliverable, and this feature file itself now executes rather than sitting decorative
