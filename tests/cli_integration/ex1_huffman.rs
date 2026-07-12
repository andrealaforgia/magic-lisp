//! EX1: a genuine Huffman compressor/decompressor pair, written entirely as
//! MagicLisp programs (`examples/huffman/{compress,decompress}.ml`) and run
//! through the real CLI — never a new Rust implementation. Every test here
//! shells out to the exact pipeline `examples/huffman/README.md` documents
//! (`xxd -p -c 0 ... | magiclisp eval ... | xxd -r -p`), so a passing test
//! is also proof the documented instructions work as written (E4/E5).

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::helpers::{assert_within_release_ceiling, temp_path};

fn huffman_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/huffman")
}

/// Spawns `cmd`, optionally feeding it `stdin_data`, and returns its
/// captured stdout -- panicking with its stderr on a non-zero exit. Writes
/// stdin on a separate thread (mirroring `helpers.rs::run_with_stdin`'s own
/// documented deadlock-avoidance reasoning) before waiting for the child.
fn spawn_capture(cmd: &mut Command, stdin_data: Option<&[u8]>) -> Vec<u8> {
    let mut child = cmd
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("process should spawn");
    let writer = stdin_data.map(|data| {
        let mut stdin = child.stdin.take().unwrap();
        let data = data.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        })
    });
    let output = child
        .wait_with_output()
        .expect("process should run to completion");
    if let Some(w) = writer {
        w.join().expect("stdin writer thread should not panic");
    }
    assert!(
        output.status.success(),
        "process failed (exit {:?}), stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Runs the exact pipeline the README documents for one direction
/// (`xxd -p -c 0 <input> | magiclisp eval <script> | xxd -r -p`), wiring the
/// three processes together natively via `Stdio` pipes rather than a shell
/// string (warden security-review msg #53, Low: the prior `sh -c` version
/// built its command via Rust's Debug-formatting, which escapes Rust-string
/// characters but not shell metacharacters -- not exploitable with this
/// file's fixed test-literal inputs, but a latent footgun for a future
/// caller with a less-predictable path; removing the shell removes the
/// whole class of risk rather than just escaping around it). Returns the
/// final stage's raw stdout bytes.
fn run_documented_pipeline(input_path: &PathBuf, script: &str) -> Vec<u8> {
    let script_path = huffman_dir().join(script);
    let hex_in = spawn_capture(
        Command::new("xxd").args(["-p", "-c", "0"]).arg(input_path),
        None,
    );
    let hex_out = spawn_capture(
        Command::new(env!("CARGO_BIN_EXE_magiclisp"))
            .arg("eval")
            .arg(&script_path),
        Some(&hex_in),
    );
    spawn_capture(Command::new("xxd").args(["-r", "-p"]), Some(&hex_out))
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

// --- Warden security-review msg #53 (Medium): compress.ml against a real
// file could look like an indefinite hang -- a hash-table-keyed-by-byte-
// value frequency count took 10.7s on a 400KB single-repeated-byte input,
// and a 2MB version didn't finish within a 120s ceiling; a 100KB
// full-alphabet input took 34s, and 200KB didn't finish within 60s.
// Root-caused past the warden's own diagnosis: the dominant cost wasn't
// count-frequencies's hash table (fixed regardless, switching to a fixed
// 256-slot vector) but two independent effects that only showed up at
// realistic file sizes: (1) `string-ref` scans from the start of the
// string on every call, so indexing through a whole hex string by
// position (as an earlier version of parse-hex-bytes did) is quadratic on
// its own; (2) a closure created inside a *named let* whose own captured
// frame includes a reference to a large/growing list or a mutable cursor
// pair gets registered with the cycle-safety collector, and the first
// sweep to fire anywhere nearby then has to walk that whole structure --
// repeated per closure creation, this compounds. Every closure-per-element
// pattern and every such named-let in compress.ml/decompress.ml was
// rewritten as plain top-level recursion instead, which creates no
// closures at all. These two tests exercise both directions at a size
// large enough to have been clearly infeasible before (200KB was
// unfinished within 60s per the warden's own measurement), with a still-
// generous ceiling the fixed implementation clears comfortably.
//
// The timing checks only apply on an optimised release build (qa
// test-design review msg #71, mirroring B21's own established
// `assert_within_release_ceiling` pattern): an ordinary unoptimized debug
// build runs this VM 5-20x slower for reasons unrelated to any real
// regression, so an unconditional ceiling here would be a routine flake
// under plain `cargo test`, not a meaningful guard. Correctness (the
// round-tripped bytes) is still checked unconditionally in both
// profiles. ---

const REPEATED_BYTE_COMPRESS_CEILING: std::time::Duration = std::time::Duration::from_secs(20);
const FULL_ALPHABET_ROUND_TRIP_CEILING: std::time::Duration = std::time::Duration::from_secs(30);

#[test]
fn compression_of_a_large_single_repeated_byte_input_completes_promptly_not_quadratically() {
    let input = vec![0x41u8; 2_000_000];
    let start = std::time::Instant::now();
    let compressed = compress(&input, "perf-repeated");
    let elapsed = start.elapsed();
    assert_within_release_ceiling(
        elapsed,
        REPEATED_BYTE_COMPRESS_CEILING,
        "compression of a 2MB single-repeated-byte input",
    );
    let restored = decompress(&compressed, "perf-repeated");
    assert_eq!(restored, input);
}

#[test]
fn round_trip_of_a_full_alphabet_pseudorandom_input_completes_promptly_not_quadratically() {
    // A small deterministic linear congruential generator -- no external
    // crate, no OS randomness dependency, just a reproducible stream of
    // bytes covering the full 0-255 range without the artificial structure
    // skewed_text()'s repeated words would introduce. This is the shape
    // that most stressed both the hash-table and (once that was fixed) the
    // closure/named-let effects: every byte value appears, so every
    // Huffman code and every closure-creation site in both scripts is
    // actually exercised at scale, unlike a single-repeated-byte input
    // (K=1) or a highly skewed one.
    let mut state: u64 = 42;
    let mut input = Vec::with_capacity(200_000);
    for _ in 0..200_000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        input.push((state >> 56) as u8);
    }
    let start = std::time::Instant::now();
    let compressed = compress(&input, "perf-random");
    let compress_elapsed = start.elapsed();
    assert_within_release_ceiling(
        compress_elapsed,
        FULL_ALPHABET_ROUND_TRIP_CEILING,
        "compression of a 200KB full-alphabet input",
    );

    let start = std::time::Instant::now();
    let restored = decompress(&compressed, "perf-random");
    let decompress_elapsed = start.elapsed();
    assert_within_release_ceiling(
        decompress_elapsed,
        FULL_ALPHABET_ROUND_TRIP_CEILING,
        "decompression of this input's compressed output",
    );
    assert_eq!(restored, input);
}

// --- E4: the README's documented instructions are self-sufficient for a
// new, unaided user. Examiner expectation msg #59 (reinforcing qa
// test-design review msg #52's structural finding): a keyword-presence grep
// only proves certain words appear, not that the documented commands
// actually work -- and nothing would catch the README's prose drifting from
// what's proven to work (e.g. a future edit dropping `-c 0`). This test
// instead extracts the two documented pipeline commands verbatim out of the
// README's own fenced usage block and executes those extracted commands
// directly -- so a change to the documented flags/pipe structure breaks
// this test, not just a missing word. ---

/// Extracts the compress and decompress pipeline command lines (each
/// starting with `xxd -p -c 0`, the way the README's usage block writes
/// them) verbatim out of its first ` ```sh ` fenced code block -- see the E4
/// test above for why this must be extraction, not a hand-maintained copy.
fn extract_documented_commands(readme: &str) -> (String, String) {
    let fence = "```sh";
    let start = readme
        .find(fence)
        .map(|i| i + fence.len())
        .expect("README usage section should have a ```sh fenced code block");
    let block_end = readme[start..]
        .find("```")
        .expect("the ```sh fenced code block should be closed");
    let block = &readme[start..start + block_end];

    let mut compress_line = None;
    let mut decompress_line = None;
    for line in block.lines() {
        let line = line.trim();
        if !line.starts_with("xxd -p -c 0") {
            continue;
        }
        // Checked in this order since "decompress.ml" itself contains the
        // substring "compress.ml".
        if line.contains("decompress.ml") {
            decompress_line = Some(line.to_string());
        } else if line.contains("compress.ml") {
            compress_line = Some(line.to_string());
        }
    }
    (
        compress_line.expect("README usage block should document a compress.ml command"),
        decompress_line.expect("README usage block should document a decompress.ml command"),
    )
}

/// Runs an extracted README command line verbatim, after substituting only
/// its placeholder filenames (`your-input-file` etc. -- not part of the
/// documented flags/pipe structure this test protects, just the names a
/// real user would replace with their own) for real temp-file paths.
/// `$MLBIN` is left untouched in the command text itself and supplied via
/// the `MLBIN` environment variable instead, exactly as the README's own
/// preceding `MLBIN=target/release/magiclisp` line instructs a user to set
/// it. Runs with the repo root as the working directory, matching the
/// README's explicit "From the repository root" framing, so the command's
/// own relative `examples/huffman/*.ml` paths resolve unmodified. A real
/// shell is required here (unlike `run_documented_pipeline`'s native-piped
/// helper) since the whole point is proving this literal, pipe-and-redirect
/// shell text -- extracted from the repo's own trusted README, never from
/// untrusted input -- works as written.
fn run_extracted_command(template: &str, subs: &[(&str, &str)]) {
    let mut cmd = template.to_string();
    for (from, to) in subs {
        cmd = cmd.replace(from, to);
    }
    let result = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("MLBIN", env!("CARGO_BIN_EXE_magiclisp"))
        .output()
        .expect("sh should run");
    assert!(
        result.status.success(),
        "extracted README command failed (sh exit {:?}): {cmd}\nstderr: {}",
        result.status.code(),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn e4_the_documented_readme_commands_extracted_verbatim_compress_then_decompress_a_real_file() {
    let readme = std::fs::read_to_string(huffman_dir().join("README.md"))
        .expect("examples/huffman/README.md should exist");
    let (compress_template, decompress_template) = extract_documented_commands(&readme);

    let input_path = temp_path("e4-your-input-file");
    let compressed_path = temp_path("e4-compressed.huff");
    let restored_path = temp_path("e4-restored-file");
    let input = skewed_text(50);
    std::fs::write(&input_path, &input).unwrap();

    run_extracted_command(
        &compress_template,
        &[
            ("your-input-file", input_path.to_str().unwrap()),
            ("compressed.huff", compressed_path.to_str().unwrap()),
        ],
    );
    assert!(
        std::fs::metadata(&compressed_path).is_ok(),
        "the extracted compress command should have produced its documented output file"
    );

    run_extracted_command(
        &decompress_template,
        &[
            ("compressed.huff", compressed_path.to_str().unwrap()),
            ("restored-file", restored_path.to_str().unwrap()),
        ],
    );
    let restored = std::fs::read(&restored_path)
        .expect("the extracted decompress command should have produced its documented output file");
    assert_eq!(
        restored, input,
        "following the README's own literal documented commands, unmodified apart from \
         substituting its placeholder filenames, should round-trip the file exactly"
    );
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
