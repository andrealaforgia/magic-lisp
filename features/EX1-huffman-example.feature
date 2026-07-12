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
    # Evidence: Examiner independently followed examples/huffman/README.md's usage section
    #   unaided (without reading the .ml source first) against a self-constructed 25,301-byte
    #   skewed-frequency file (this repo's README.md + SPEC.md concatenated): the compress
    #   command produced a distinct 16,144-byte output.
    #   Also: tests/cli_integration/ex1_huffman.rs::e1_compress_produces_a_distinct_output_file_
    #   not_a_copy_of_the_input -- green.

  Scenario: E2 — decompressing reproduces the original input byte-for-byte, across genuinely different inputs
    Given the compressed output of a real input file and the documented decompress command
    When it is run from the command line
    Then the restored file is byte-for-byte identical to the original input, for a skewed-frequency text file, an empty file, a file containing only one distinct repeated byte value, and a file covering arbitrary binary byte values (not just printable text)
    # Evidence: Examiner's own independent round trips (distinct fixtures from the Builder's own
    #   tests), each confirmed with `cmp`:
    #     - skewed-frequency 25,301-byte text file -> identical
    #     - empty file -> identical
    #     - 5,000 repeated 0x41 bytes -> identical
    #     - self-constructed full 0-255 byte-range file (7,120 bytes, deterministic PRNG seed 42)
    #       -> identical
    #   Also: tests/cli_integration/ex1_huffman.rs's four e2_* round-trip tests -- all green.

  Scenario: E3 — a skewed-frequency input compresses measurably smaller, proving genuine Huffman coding
    Given an input file with a clearly skewed byte-frequency distribution
    When it is compressed via the documented command
    Then the compressed output is measurably, substantially smaller than the original, reflecting real frequency-based variable-length coding rather than a pass-through
    # Evidence: the Examiner's own 25,301-byte skewed text fixture compressed to 16,144 bytes
    #   (~36% reduction) -- substantial, not marginal.
    #   Cross-check confirming the algorithm isn't merely lucky: a near-uniform, mostly-random
    #   binary fixture (7,120 bytes, full 0-255 range) compressed to 8,406 bytes (grew slightly),
    #   exactly matching the README's own documented expectation that near-uniform data may not
    #   shrink because of the frequency-table header -- real Huffman behaviour, not a trivial
    #   always-shrinks transform.
    #   Also: tests/cli_integration/ex1_huffman.rs::e3_a_skewed_frequency_input_compresses_
    #   measurably_smaller_than_the_original -- green.

  Scenario: E4 — the documented run instructions are self-sufficient for a new, unaided user
    Given examples/huffman/README.md, read on its own with no other context
    When its usage section's exact commands are followed unaided
    Then a new user successfully compresses then decompresses a file, with no need to read the .ml source or ask for help, and the check fails if the documented commands themselves ever drift from what's proven to work
    # Evidence: the Examiner followed the README's usage section exactly, unaided, as a stand-in
    #   for a new user -- reading only the README (not compress.ml/decompress.ml) before running
    #   the commands -- and every fixture above succeeded on the first attempt with no
    #   corrections needed.
    #   Following an Examiner reinforcement (the original check only grepped the README for
    #   keyword substrings, which proves words appear but not that the commands work), the
    #   Builder rewrote it to extract the literal pipeline commands from the README's own fenced
    #   usage block and execute those extracted commands directly -- confirmed by reading
    #   tests/cli_integration/ex1_huffman.rs's extraction code, which parses the ```sh block and
    #   runs it verbatim (only placeholder filenames substituted): green.

  Scenario: E5 — integration: the documented pipeline round-trips a real file end to end, now via a real BDD run
    Given the README's full documented pipeline (compress then decompress, exactly as written)
    When it is run end to end against a real file from the command line, executed by a real Cucumber-style step-definition runner (not just ad hoc integration tests)
    Then the restored file is byte-for-byte identical to the original, demonstrating the whole example (algorithm + CLI + docs) works as one coherent, usable deliverable, and this feature file itself now executes rather than sitting decorative
    # Evidence: Examiner's own end-to-end run of the exact documented pipeline against the
    #   25,301-byte skewed fixture -- `cmp` reports byte-for-byte identical.
    #   This feature file is now registered and executed by tests/features.rs
    #   (`ex1_huffman_example ... ok`, 1.27s) via tests/features/steps_ex1.rs, closing the earlier
    #   "pending step-def wiring" gap -- independently re-run by the Examiner.
    #   Full tests/cli_integration/ex1_huffman.rs suite independently re-run: 10 passed, 0 failed
    #   (9.19s) -- includes two new performance regression tests (a 2MB single-repeated-byte
    #   input and a 200KB full-alphabet pseudorandom input, both completing well within their
    #   ceilings) added after a genuine quadratic slowdown was found and fixed. Examiner's own,
    #   further independent stress check with different parameters again (300KB of seed-7
    #   pseudorandom bytes, not the Builder's own fixture): compress ~5.0s, decompress ~6.5s,
    #   `cmp` identical -- confirms the fix generalises, not just passes the exact committed test.
    #   Full unit suite (`cargo test --release --lib`) also independently re-run: 827 passed, 0
    #   failed, 2 ignored.
