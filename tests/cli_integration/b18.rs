//! B18: crash-free robustness across all malformed input.

use std::process::ExitStatus;

use magiclisp::bytecode::Op;
use magiclisp::exitcode::{BAD_ARTIFACT, RUNTIME_ERROR, SOURCE_ERROR, SUCCESS, USAGE_ERROR};

use super::helpers::{
    assert_rejected_as_bad_artifact, compile_good_artifact, run, stderr_of, temp_path, write_source,
};

/// `true` iff the process ended via an ordinary exit code -- `None` on
/// Unix specifically means it was killed by a signal (segfault, SIGABRT
/// from a native stack overflow, etc.), which is exactly the "crash" this
/// whole behaviour exists to rule out, distinct from any exit code at all.
fn exited_cleanly(status: &ExitStatus) -> bool {
    status.code().is_some()
}

const ESTABLISHED_CODES: [i32; 5] = [
    SUCCESS,
    USAGE_ERROR,
    SOURCE_ERROR,
    BAD_ARTIFACT,
    RUNTIME_ERROR,
];

// --- E1: broken source text, five categories, each a clean source-error exit. ---

#[test]
fn e1_unbalanced_parentheses_is_a_source_error_not_a_crash() {
    let file = write_source("b18-e1-parens.ml", "(display (+ 1 2)");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
    assert!(!stderr_of(&out).is_empty());
}

#[test]
fn e1_unterminated_string_is_a_source_error_not_a_crash() {
    let file = write_source("b18-e1-string.ml", "(display \"unterminated");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
    assert!(!stderr_of(&out).is_empty());
}

#[test]
fn e1_raw_unescaped_newline_inside_a_string_is_a_source_error_not_a_crash() {
    // Already established by B1's own E6; reconfirmed here since this
    // slice's own E1 names it as one of its five required categories.
    let file = write_source("b18-e1-newline.ml", "(display \"broken\nstring\")");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
    assert!(!stderr_of(&out).is_empty());
}

#[test]
fn e1_a_whole_number_literal_too_large_for_the_integer_range_is_a_source_error_not_a_crash() {
    let file = write_source(
        "b18-e1-overflow.ml",
        "(display 999999999999999999999999999)",
    );
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
    assert!(!stderr_of(&out).is_empty());
}

#[test]
fn e1_a_stray_misplaced_dot_is_a_source_error_not_a_crash() {
    let file = write_source("b18-e1-dot.ml", "(display (1 . 2 . 3))");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
    assert!(!stderr_of(&out).is_empty());
}

// --- E2: file-format errors at the OUTER container level. ---

#[test]
fn e2_nonzero_disallowed_flags_are_rejected_by_both_run_and_disasm() {
    let mut bytes = compile_good_artifact("b18-e2-flags");
    bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
    assert_rejected_as_bad_artifact(&bytes, "b18-e2-flags-bad");
}

#[test]
fn e2_reconfirms_the_four_previously_established_container_level_categories() {
    // Full individual coverage already lives in B1's own E7; this is a
    // brief reconfirmation that all four still hold, as this slice's own
    // E2 asks for, now alongside the new flags category above.
    let magic_bad = {
        let mut b = compile_good_artifact("b18-e2-magic");
        b[0..4].copy_from_slice(b"NOPE");
        b
    };
    assert_rejected_as_bad_artifact(&magic_bad, "b18-e2-magic-bad");

    let version_bad = {
        let mut b = compile_good_artifact("b18-e2-version");
        b[4] = 200;
        b
    };
    assert_rejected_as_bad_artifact(&version_bad, "b18-e2-version-bad");

    let truncated = {
        let b = compile_good_artifact("b18-e2-truncated");
        b[..b.len() - 4].to_vec()
    };
    assert_rejected_as_bad_artifact(&truncated, "b18-e2-truncated-bad");

    let pointer_bad = {
        let mut b = compile_good_artifact("b18-e2-pointer");
        b[8..12].copy_from_slice(&999u32.to_le_bytes());
        b
    };
    assert_rejected_as_bad_artifact(&pointer_bad, "b18-e2-pointer-bad");
}

// --- E3: a VALID outer container, but structurally broken on the inside. ---

/// The byte range of the entry chunk's own `code` section within a
/// `compile_good_artifact`-produced file: that helper's fixed source,
/// `(display 1)`, compiles to exactly one (the entry) function, unnamed
/// and zero-arity, so the layout is a 16-byte header followed by
/// arity(4), has_rest(1), and name_flag(1), landing on a 4-byte code_len
/// field starting at byte 22, with the code itself immediately following
/// -- see `bytecode::encode`'s own per-function layout. Computed from the
/// file's own declared length rather than hardcoded, so this stays
/// correct if the compiled instruction sequence for this source ever
/// changes shape.
fn entry_code_range(bytes: &[u8]) -> std::ops::Range<usize> {
    let code_len = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
    26..26 + code_len
}

#[test]
fn e3_an_instruction_byte_that_isnt_a_real_opcode_is_reported_not_crashed() {
    let mut bytes = compile_good_artifact("b18-e3-opcode");
    let code_range = entry_code_range(&bytes);
    // The entry chunk's own last instruction is HALT -- overwrite just
    // that one byte with a value no opcode in this format uses.
    let last = code_range.end - 1;
    assert_eq!(bytes[last], Op::Halt as u8, "test's own layout assumption");
    bytes[last] = 250;

    let artifact = temp_path("b18-e3-opcode-bad.mlbc");
    std::fs::write(&artifact, &bytes).unwrap();

    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert!(exited_cleanly(&run_out.status), "run must not crash");
    assert!(
        matches!(
            run_out.status.code(),
            Some(RUNTIME_ERROR) | Some(BAD_ARTIFACT)
        ),
        "run exit code: {:?}, stderr: {}",
        run_out.status.code(),
        stderr_of(&run_out)
    );
    assert!(!stderr_of(&run_out).is_empty());

    let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
    assert!(exited_cleanly(&disasm_out.status), "disasm must not crash");
}

#[test]
fn e3_an_out_of_range_constant_index_is_reported_not_crashed() {
    let mut bytes = compile_good_artifact("b18-e3-const-index");
    let code_range = entry_code_range(&bytes);
    // The entry chunk's own first instruction is GET_GLOBAL <idx>: its
    // operand is the 4 bytes immediately after that one opcode byte.
    let opcode_pos = code_range.start;
    assert_eq!(
        bytes[opcode_pos],
        Op::GetGlobal as u8,
        "test's own layout assumption"
    );
    let operand = opcode_pos + 1..opcode_pos + 5;
    bytes[operand].copy_from_slice(&u32::MAX.to_le_bytes());

    let artifact = temp_path("b18-e3-const-index-bad.mlbc");
    std::fs::write(&artifact, &bytes).unwrap();

    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert!(exited_cleanly(&run_out.status), "run must not crash");
    assert!(
        matches!(
            run_out.status.code(),
            Some(RUNTIME_ERROR) | Some(BAD_ARTIFACT)
        ),
        "run exit code: {:?}, stderr: {}",
        run_out.status.code(),
        stderr_of(&run_out)
    );
    assert!(!stderr_of(&run_out).is_empty());

    let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
    assert!(exited_cleanly(&disasm_out.status), "disasm must not crash");
}

#[test]
fn e3_an_entry_function_ending_mid_instruction_is_reported_not_crashed() {
    let mut bytes = compile_good_artifact("b18-e3-truncated-instruction");
    let code_range = entry_code_range(&bytes);
    // Replace the final HALT (0 operand bytes) with CONST (needs 4
    // operand bytes) at that same last position -- the chunk's own code
    // now ends mid-instruction, with no operand bytes left to read.
    let last = code_range.end - 1;
    assert_eq!(bytes[last], Op::Halt as u8, "test's own layout assumption");
    bytes[last] = Op::Const as u8;

    let artifact = temp_path("b18-e3-truncated-instruction-bad.mlbc");
    std::fs::write(&artifact, &bytes).unwrap();

    let run_out = run(&["run", artifact.to_str().unwrap()]);
    assert!(exited_cleanly(&run_out.status), "run must not crash");
    assert!(
        matches!(
            run_out.status.code(),
            Some(RUNTIME_ERROR) | Some(BAD_ARTIFACT)
        ),
        "run exit code: {:?}, stderr: {}",
        run_out.status.code(),
        stderr_of(&run_out)
    );
    assert!(!stderr_of(&run_out).is_empty());

    let disasm_out = run(&["disasm", artifact.to_str().unwrap()]);
    assert!(exited_cleanly(&disasm_out.status), "disasm must not crash");
}

// --- E4: command-line misuse. ---

#[test]
fn e4_an_unrecognized_verb_is_a_usage_error_not_a_crash() {
    // Already established by B1's own E4; brief reconfirmation here.
    let file = write_source("b18-e4-unknown.ml", "(display 1)");
    let out = run(&["frobnicate", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(USAGE_ERROR));
    assert!(exited_cleanly(&out.status));
}

#[test]
fn e4_a_missing_required_argument_is_a_usage_error_not_a_crash() {
    let out = run(&["eval"]);
    assert_eq!(out.status.code(), Some(USAGE_ERROR));
    assert!(exited_cleanly(&out.status));
}

// --- E5: a genuine breadth sweep, not just the named categories. ---

/// Malformed MagicLisp source snippets spanning many distinct ways source
/// text can be broken -- deliberately broader than E1's five named
/// categories (deep/asymmetric nesting, empty/whitespace-only/comment-only
/// input, malformed escapes, bad radix literals, delimiter mismatches,
/// stray reader-only tokens, and more).
fn malformed_source_snippets() -> Vec<&'static str> {
    vec![
        "(display (+ 1 2)",
        "(display \"unterminated",
        "(display \"broken\nstring\")",
        "(display 999999999999999999999999999)",
        "(display (1 . 2 . 3))",
        "(",
        "(((((((((((",
        ")",
        "))))))))))",
        "",
        "   \n\t  ",
        "; just a comment, nothing else",
        "(display \"bad escape \\q\")",
        "(display \"unterminated escape \\",
        "#(1 2",
        "#(",
        "#\\",
        "#z",
        "#xzz",
        "#bzz",
        "#ozz",
        "(display .)",
        "(. )",
        "(display (quote )",
        "(display (lambda))",
        "(define)",
        "(display (+ 1 2",
        "\"",
        "(\"",
        "(display #t",
        "((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((",
    ]
}

/// Deterministic byte-level corruptions of an otherwise-valid artifact:
/// every offset in the file, each independently overwritten with a fixed
/// out-of-range byte value -- not a sample, the FULL sweep across the
/// whole file, spanning both the outer container (header) and the inner
/// code/constant sections alike.
fn corrupted_artifacts() -> Vec<Vec<u8>> {
    let good = compile_good_artifact("b18-e5-sweep-base");
    (0..good.len())
        .map(|i| {
            let mut bytes = good.clone();
            // 0xFF is not a valid magic byte, not a supported version, and
            // not a defined opcode -- deliberately hostile at every
            // possible byte role this position could play.
            bytes[i] = 0xFF;
            bytes
        })
        .collect()
}

#[test]
fn e5_a_genuine_breadth_of_malformed_source_never_crashes_and_always_lands_on_an_established_code()
{
    let snippets = malformed_source_snippets();
    let mut passed = 0;
    for (i, src) in snippets.iter().enumerate() {
        let file = write_source(&format!("b18-e5-src-{i}.ml"), src);
        let out = run(&["eval", file.to_str().unwrap()]);
        assert!(
            exited_cleanly(&out.status),
            "snippet {i:?} ({src:?}) was killed rather than exiting cleanly"
        );
        assert!(
            ESTABLISHED_CODES.contains(&out.status.code().unwrap()),
            "snippet {i:?} ({src:?}) exited with an unrecognized code {:?}",
            out.status.code()
        );
        passed += 1;
    }
    assert_eq!(passed, snippets.len());
    eprintln!(
        "B18 E5 source sweep: {passed}/{} malformed source snippets, \
         100% exited cleanly on an established exit code",
        snippets.len()
    );
}

#[test]
fn e5_a_genuine_breadth_of_corrupted_artifacts_never_crashes_and_always_lands_on_an_established_code()
 {
    let variants = corrupted_artifacts();
    let mut passed = 0;
    for (i, bytes) in variants.iter().enumerate() {
        let artifact = temp_path(&format!("b18-e5-artifact-{i}.mlbc"));
        std::fs::write(&artifact, bytes).unwrap();

        for verb in ["run", "disasm"] {
            let out = run(&[verb, artifact.to_str().unwrap()]);
            assert!(
                exited_cleanly(&out.status),
                "corrupting byte offset {i} was killed rather than exiting cleanly ({verb})"
            );
            assert!(
                ESTABLISHED_CODES.contains(&out.status.code().unwrap()),
                "corrupting byte offset {i} ({verb}) exited with an unrecognized code {:?}",
                out.status.code()
            );
        }
        passed += 1;
    }
    assert_eq!(passed, variants.len());
    eprintln!(
        "B18 E5 artifact-corruption sweep: {passed}/{} byte offsets (x2 verbs each), \
         100% exited cleanly on an established exit code",
        variants.len()
    );
}

// --- E6 (integration): the DEMO cases from the behaviour spec, verbatim. ---

#[test]
fn e6_demo_1_three_open_parentheses_is_a_source_error() {
    let file = write_source("b18-e6-1.ml", "(((");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
}

#[test]
fn e6_demo_2_an_opening_quote_with_no_closing_quote_is_a_source_error() {
    let file = write_source("b18-e6-2.ml", "(display \"no closing quote)");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
}

#[test]
fn e6_demo_3_an_oversized_whole_number_literal_is_a_source_error() {
    let file = write_source("b18-e6-3.ml", "(display 123456789012345678901234567890)");
    let out = run(&["eval", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(SOURCE_ERROR));
    assert!(exited_cleanly(&out.status));
}

#[test]
fn e6_demo_4_arbitrary_bytes_handed_to_run_is_a_file_format_error() {
    let artifact = temp_path("b18-e6-4.mlbc");
    std::fs::write(&artifact, b"this is not a valid MLBC artifact at all").unwrap();
    let out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(BAD_ARTIFACT));
    assert!(exited_cleanly(&out.status));
}

#[test]
fn e6_demo_5_a_truncated_valid_prefix_artifact_is_a_file_format_error() {
    let good = compile_good_artifact("b18-e6-5");
    let truncated = good[..good.len() / 2].to_vec();
    let artifact = temp_path("b18-e6-5.mlbc");
    std::fs::write(&artifact, &truncated).unwrap();
    let out = run(&["run", artifact.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(BAD_ARTIFACT));
    assert!(exited_cleanly(&out.status));
}

#[test]
fn e6_demo_6_an_unrecognized_verb_is_a_usage_error() {
    let out = run(&["bogus-verb"]);
    assert_eq!(out.status.code(), Some(USAGE_ERROR));
    assert!(exited_cleanly(&out.status));
}
