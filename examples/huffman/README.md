# Huffman compression example

A genuine Huffman compressor/decompressor pair, written entirely as MagicLisp
programs (`compress.ml` / `decompress.ml`) and run through the real `magiclisp`
CLI — no separate Rust tool involved.

## Why hex, and why `xxd`

MagicLisp programs can only read/write plain UTF-8 text through standard
input and output (there is no file I/O, and no way to emit an arbitrary raw
byte through `display`). To compress or decompress an arbitrary file —
including genuinely binary data — these programs read and write the file's
bytes as a hex string (two ASCII hex characters per byte), and the
[`xxd`](https://linux.die.net/man/1/xxd) utility (installed by default on
macOS and most Linux distributions) does the conversion to and from real
binary files at the shell level. The compression algorithm itself, including
all of the bit-packing, is 100% MagicLisp.

## Usage

From the repository root, with a release build available (`cargo build
--release`):

```sh
MLBIN=target/release/magiclisp

# Compress: any-file -> hex -> compress.ml -> hex -> any-file
xxd -p -c 0 your-input-file   | $MLBIN eval examples/huffman/compress.ml   | xxd -r -p > compressed.huff

# Decompress: reverses the exact same steps
xxd -p -c 0 compressed.huff   | $MLBIN eval examples/huffman/decompress.ml | xxd -r -p > restored-file

# Confirm it's an exact match
cmp your-input-file restored-file && echo "byte-for-byte identical"
```

That's the whole workflow — two commands to compress, two to decompress
(the `xxd` calls and the `magiclisp eval` calls), piped together. It works
on any file: plain text, already-compressed binary data, images, anything.

## What to expect

- Files with a skewed byte-frequency distribution (ordinary text is a good
  example) compress measurably smaller — that's the actual Huffman coding at
  work, not a copy.
- Files with a near-uniform byte distribution (e.g. already-compressed data,
  or random bytes) may not shrink, or may even grow slightly, because of the
  header that carries the frequency table. That's expected and correct —
  Huffman coding cannot beat close to 8 bits/symbol when every symbol is
  roughly equally likely.
- An empty input file, and a file containing only one repeated byte value,
  are both handled as explicit special cases and round-trip correctly.
- Decompression always reproduces the original file exactly, byte for byte,
  regardless of what the bytes are (text or arbitrary binary).

## How it works, briefly

`compress.ml` counts how often each byte value (0–255) occurs in the input,
builds a Huffman tree from those frequencies (rarer bytes get longer bit
codes, common bytes get shorter ones), and writes out a small header
describing the frequency table followed by the bit-packed encoded data.
`decompress.ml` reads that same header, rebuilds an *identical* tree from the
frequency table (deterministically — the tree itself is never transmitted,
only the frequencies), and walks the encoded bits back into the original
bytes. See the comments at the top of each `.ml` file for the exact byte
layout of the compressed format.
