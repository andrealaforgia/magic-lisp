Feature: B16 — The disassembler
  As a developer running `magiclisp disasm` against a compiled file
  I want a full, human-auditable inspection of every function, its constants, and its instructions
  So that a compiled file's contents are fully observable, completing B1's minimal dump on top of B1-B15

  # Builds on B1's minimal instruction dump. Layout/spacing is free, but which fields
  # appear, the instruction names used, and how jump targets are expressed are normative
  # (SPEC.md sections 7.4/7.5) and must match the spec exactly. How offsets are computed or
  # how column layout is chosen is not observable/prescribed.

  Scenario: E1 — every function's header shows index, name-or-distinct-placeholder, arity, variadic flag, and upvalue count
    Given a program defining a named function that returns a closure over one of its parameters, called from the top level
    When it is compiled and disassembled
    Then the dump shows three functions: the top-level entry (its own distinct placeholder), the named outer function (arity 1, non-variadic), and the anonymous inner closure (a DIFFERENT placeholder than the top-level's, reporting exactly 1 captured upvalue)
    # Evidence: $ cat demo1.ml
    #   (define (add-n n) (lambda (x) (+ x n)))
    #   (display ((add-n 4) 3)) (newline)
    #   $ magiclisp compile demo1.ml -o demo1.mlbc && magiclisp disasm demo1.mlbc
    #   == function 0: name=<anonymous>, arity=1, variadic=false, upvalues=1 ==   (inner closure)
    #   == function 1: name=add-n, arity=1, variadic=false, upvalues=0 ==         (named outer function)
    #   == function 2: name=<toplevel>, arity=0, variadic=false, upvalues=0 ==    (entry function)
    #   "<toplevel>" and "<anonymous>" are two distinct placeholder strings, never confused.
    #   Independently re-verified byte-for-byte against the release binary.

  Scenario: E2 — every function's constant pool shows index, type, and machine-readable value, across multiple distinct types
    Given a program whose constant pool contains both symbols and a number
    When it is compiled and disassembled
    Then each constant entry shows its index, its type label, and its value in write form, with the type label correctly varying across at least two distinct types
    # Evidence: $ cat demo2.ml
    #   (define (sign n) (if (< n 0) (quote neg) (quote pos)))
    #   (display (sign -2)) (newline)
    #   $ magiclisp compile demo2.ml -o demo2.mlbc && magiclisp disasm demo2.mlbc
    #   function 0's constants: "0: Symbol <", "1: Int 0", "2: Symbol neg", "3: Symbol pos"
    #   (both symbols in write form, plus an Int-typed constant showing the type field varies correctly)

  Scenario: E3 — every instruction line shows a numeric offset, mnemonic, and operands, with all required instruction kinds present
    Given the same closure-over-parameter program
    When it is compiled and disassembled
    Then the inner function's instructions include reading a local, reading a captured upvalue, and a return (with the arithmetic itself expressed as an ordinary global-procedure call — GET_GLOBAL + TAIL_CALL — since `+` is an established first-class, redefinable procedure, not a special-cased operator, per B7), the top-level function's instructions include constructing a closure, defining a global, reading a global, loading a constant, making a call, discarding a value, and a halt, and every line in both dumps carries a numeric offset
    # Evidence (from demo1.ml's disasm above):
    #   function 0 (inner): GET_GLOBAL (fetches +), GET_LOCAL, GET_UPVALUE depth=1 slot=0, TAIL_CALL 2, RETURN
    #   function 2 (top-level): MAKE_FUNCTION, DEF_GLOBAL, POP, GET_GLOBAL, CONST, CALL (x3), POP, GET_GLOBAL, CALL, POP, HALT
    #   Every code line begins with a 4-digit hex offset; none missing.
    #   Independently re-verified byte-for-byte against the release binary.

  Scenario: E4 — a jump instruction's target is an absolute offset landing on a real instruction boundary
    Given the conditional-branch program's compiled and disassembled form
    When the conditional-jump instruction's target value is cross-referenced against the dump's own offset column
    Then the target value exactly matches another instruction's own offset elsewhere in the same dump, proving it's a genuine absolute address, not a relative displacement or an arbitrary number
    # Evidence (from demo2.ml's disasm above):
    #   "000e  JUMP_IF_FALSE -> 001d" ... "001d  CONST  3  ; pos"
    #   The jump's target (001d) exactly matches the CONST instruction's own offset.

  Scenario: E5 — integration: both verbatim demo programs' full dumps exhibit every described property together
    Given both DEMO programs from the behaviour spec, each compiled and disassembled
    When the full dumps are inspected
    Then demo 1's three-function structure (correct placeholders/arity/variadic/upvalue-count, required instructions in both the inner and top-level functions, all lines offset) and demo 2's absolute boundary-landing jump target plus its two-symbol constant pool all hold at once
    # Evidence: full dump text for both demo1.ml and demo2.ml, as shown in E1-E4 above,
    #   pasted verbatim and independently re-verified byte-for-byte against the release binary.
