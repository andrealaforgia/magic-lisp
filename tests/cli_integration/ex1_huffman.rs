//! EX1: a genuine Huffman compressor/decompressor pair, written entirely as
//! MagicLisp programs (`examples/huffman/{compress,decompress}.ml`) and run
//! through the real CLI — never a new Rust implementation. Every test here
//! shells out to the exact pipeline `examples/huffman/README.md` documents
//! (`xxd -p -c 0 ... | magiclisp eval ... | xxd -r -p`), so a passing test
//! is also proof the documented instructions work as written (E4/E5).

use std::path::PathBuf;
use std::process::Command;

use super::helpers::temp_path;

fn huffman_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/huffman")
}

/// Runs the exact shell pipeline the README documents for one direction
/// (`xxd -p -c 0 <input> | magiclisp eval <script> | xxd -r -p > <output>`),
/// returning the produced output file's raw bytes.
fn run_documented_pipeline(input_path: &PathBuf, script: &str) -> Vec<u8> {
    let output_path = temp_path(&format!("ex1-out-{script}"));
    let script_path = huffman_dir().join(script);
    let shell = format!(
        "xxd -p -c 0 {input:?} | {bin:?} eval {script:?} | xxd -r -p > {out:?}",
        input = input_path,
        bin = env!("CARGO_BIN_EXE_magiclisp"),
        script = script_path,
        out = output_path,
    );
    let result = Command::new("sh")
        .arg("-c")
        .arg(&shell)
        .output()
        .expect("sh should run");
    assert!(
        result.status.success(),
        "pipeline failed (sh exit {:?}), stderr: {}",
        result.status.code(),
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(&output_path).expect("output file should have been written")
}

fn compress(input_bytes: &[u8], label: &str) -> Vec<u8> {
    let input_path = temp_path(&format!("ex1-in-{label}"));
    std::fs::write(&input_path, input_bytes).unwrap();
    run_documented_pipeline(&input_path, "compress.ml")
}

fn decompress(compressed_bytes: &[u8], label: &str) -> Vec<u8> {
    let compressed_path = temp_path(&format!("ex1-compressed-{label}"));
    std::fs::write(&compressed_path, compressed_bytes).unwrap();
    run_documented_pipeline(&compressed_path, "decompress.ml")
}

fn assert_round_trips(original: &[u8], label: &str) {
    let compressed = compress(original, label);
    let restored = decompress(&compressed, label);
    assert_eq!(
        restored, original,
        "{label}: decompressed output does not match the original byte-for-byte"
    );
}

/// A skewed-frequency English-ish text: a handful of words repeated many
/// times, so a handful of byte values dominate -- exactly the shape
/// Huffman coding is meant to exploit.
fn skewed_text(repeats: usize) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..repeats {
        s.push_str("the quick the lazy the the the of of of a a ");
        if i % 17 == 0 {
            s.push_str("xylophone zephyr ");
        }
    }
    s.into_bytes()
}

// --- E1: compress mode invoked from the command line against a real input
// file writes a distinct compressed output file (a genuine transformation,
// not a copy). ---

#[test]
fn e1_compress_produces_a_distinct_output_file_not_a_copy_of_the_input() {
    let input = skewed_text(40);
    let compressed = compress(&input, "e1");
    assert_ne!(
        compressed, input,
        "compressed output should be a genuine transformation of the input"
    );
}

// --- E2: decompress mode reproduces the original input byte-for-byte,
// across several genuinely different input shapes. ---

#[test]
fn e2_round_trips_a_skewed_frequency_text_file_byte_for_byte() {
    assert_round_trips(&skewed_text(60), "e2-skewed");
}

#[test]
fn e2_round_trips_an_empty_file_byte_for_byte() {
    assert_round_trips(&[], "e2-empty");
}

#[test]
fn e2_round_trips_a_file_containing_only_one_distinct_repeated_byte() {
    assert_round_trips(&[0x7A; 500], "e2-single-byte");
}

#[test]
fn e2_round_trips_arbitrary_binary_byte_values_not_just_printable_ascii() {
    // Every possible byte value 0-255, repeated, deliberately including
    // non-printable and non-ASCII byte values -- Huffman coding operates
    // over raw bytes regardless of whether they're printable text.
    let mut data = Vec::new();
    for _ in 0..8 {
        data.extend(0u8..=255u8);
    }
    assert_round_trips(&data, "e2-binary");
}

// --- E3: the encoding is genuinely Huffman -- a skewed-frequency input
// compresses measurably smaller, not a fixed-width/identity transform. ---

#[test]
fn e3_a_skewed_frequency_input_compresses_measurably_smaller_than_the_original() {
    let input = skewed_text(200);
    let compressed = compress(&input, "e3");
    assert!(
        compressed.len() < input.len(),
        "expected genuine compression: input {} bytes, compressed {} bytes",
        input.len(),
        compressed.len()
    );
    // Not just marginally smaller -- a real reduction, proving frequency-
    // based variable-length coding, not an incidental few bytes of overhead
    // difference.
    assert!(
        compressed.len() < input.len() * 3 / 4,
        "expected a substantial reduction for skewed input: input {} bytes, compressed {} bytes",
        input.len(),
        compressed.len()
    );
}

// --- E4: the README's documented instructions are self-sufficient. Since
// every test above already runs the exact commands the README documents
// (not an equivalent internal shortcut), a passing suite already
// demonstrates E4/E5 -- this test additionally pins that the README exists
// and actually names the commands/files a reader needs. ---

#[test]
fn e4_the_readme_documents_the_exact_commands_and_files_a_new_user_needs() {
    let readme = std::fs::read_to_string(huffman_dir().join("README.md"))
        .expect("examples/huffman/README.md should exist");
    for needle in [
        "xxd -p -c 0",
        "xxd -r -p",
        "magiclisp eval",
        "compress.ml",
        "decompress.ml",
        "cmp",
    ] {
        assert!(
            readme.contains(needle),
            "README should mention {needle:?} so a new user can follow it unaided"
        );
    }
}

// --- E5: integration -- compress then decompress a real file from the
// command line, following the documented instructions exactly, reproduces
// the original byte-for-byte. ---

#[test]
fn e5_the_documented_compress_then_decompress_pipeline_round_trips_a_real_file_end_to_end() {
    let input = skewed_text(100);
    let compressed = compress(&input, "e5");
    assert_ne!(
        compressed, input,
        "compression should be a genuine transformation"
    );
    assert!(
        compressed.len() < input.len(),
        "skewed input should compress smaller: input {} bytes, compressed {} bytes",
        input.len(),
        compressed.len()
    );
    let restored = decompress(&compressed, "e5");
    assert_eq!(
        restored, input,
        "the full documented compress-then-decompress pipeline should reproduce the original exactly"
    );
}
