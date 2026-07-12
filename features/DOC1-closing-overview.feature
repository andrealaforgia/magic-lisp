Feature: DOC1 — the project's closing write-up
  As a newcomer who has read nothing else yet
  I want a single committed document giving an honest, accurate, end-to-end account of what
  MagicLisp is and what was delivered
  So that I can understand the whole project's scope and how to run it without having to
  piece it together from the SPEC, the README, and the test suite myself

  # Owner-requested project closeout (not a frozen-spec behaviour; cites no SPEC section).
  # Delivered as OVERVIEW.md alongside README.md. Because this is a prose document rather than
  # executable system behaviour, its "evidence" is the Examiner's own line-by-line cross-check
  # of every concrete claim against SPEC.md's normative text and the already-passing B1-B23/EX1
  # test and feature suites -- not a new automated fixture, per the behaviour's own note that
  # structure/format is downstream's to choose.

  Scenario: E1 — the document states what MagicLisp is and does at a glance
    Given OVERVIEW.md's opening
    When it is read on its own, with no other context
    Then a reader immediately understands MagicLisp is a small Scheme-like Lisp with its own reader, macro expander, compiler, MLBC bytecode format, and VM, without needing to read further to get that much
    # Evidence: OVERVIEW.md's first paragraph states this directly and completely in one
    #   sentence. Examiner confirmed by reading only that paragraph in isolation.

  Scenario: E2 — the document honestly and completely summarises delivered scope, with facts that match SPEC.md and the test suites
    Given OVERVIEW.md's "What was delivered" section
    When each concrete claim it makes is checked against SPEC.md and the committed test/feature suites
    Then the language surface, the read-expand-compile-bytecode-VM pipeline, the MLBC container and its round-trip guarantee, the tooling (disassembler, REPL, five-verb CLI, exit-code contract), the conformance suite, and the non-functional guarantees (performance floors, memory/cycle-safety) are all genuinely addressed with accurate figures, not approximations or invented ones
    # Evidence: Examiner cross-checked every concrete figure against SPEC.md directly --
    #   quasiquotation "at least depth 2" (SPEC §3, verbatim match), the five verbs
    #   compile/run/eval/disasm/repl and exit codes 0/64/65/66/70 (SPEC §9.1, verbatim match),
    #   the disassembler's stated field list -- index, name, arity, variadic flag, upvalue
    #   count, constant pool in write form, absolute jump targets (SPEC §7.5, verbatim match),
    #   the ten conformance samples' descriptions (SPEC §9.5 table, matches), the 10s/20s/5s
    #   performance ceilings (SPEC §10.1, matches B21's evidence), and the cyclic/acyclic
    #   memory-boundedness claims (SPEC §10.2, matches B22's evidence). No claim found that
    #   overstates or invents a capability.

  Scenario: E3 — the document points the reader to how to run the tool, including the Huffman example
    Given OVERVIEW.md's "How to run it" section
    When a reader looks for how to run MagicLisp and specifically the Huffman example
    Then exact CLI commands are given for the tool itself, and the Huffman example is pointed to via examples/huffman/README.md rather than having its run instructions duplicated
    # Evidence: OVERVIEW.md gives the five CLI invocation forms directly, then explicitly
    #   references "See examples/huffman/README.md for its exact run commands" rather than
    #   restating them -- matches the behaviour's own "reference rather than duplicate" note.

  Scenario: E4 — every substantive claim is truthful and traceable to a real, committed artifact
    Given every concrete claim OVERVIEW.md makes about what was delivered
    When each is traced back to its source
    Then each is backed by an actual passing test/feature scenario or a guarantee SPEC.md itself states normatively, with nothing overstated or invented
    # Evidence: same cross-check as E2, plus OVERVIEW.md's own closing "Where this stands"
    #   section explicitly states this constraint about itself ("backed by a real, committed,
    #   currently-passing test or feature scenario, or a guarantee SPEC.md itself states
    #   normatively; nothing here claims more than that") -- Examiner found no counterexample
    #   across the whole document.

  Scenario: E5 — integration: read start to finish, the document holds together as an accurate closing account
    Given OVERVIEW.md read start to finish as a newcomer would, plus README.md and examples/huffman/README.md where it explicitly points
    When the whole document is taken together
    Then it gives an accurate, honest, end-to-end account of what MagicLisp is, what was delivered across B1-B23 and EX1, and how to run it -- holding together as one coherent closing account, not just isolated correct facts
    # Evidence: Examiner read OVERVIEW.md start to finish in one pass and independently
    #   verified every figure and claim it makes (see E2/E4); found the document coherent,
    #   proportionate (no section over- or under-weighted relative to what it covers), and
    #   consistent with the actually-committed B1-B23/EX1 state as of commit c2977b5.
