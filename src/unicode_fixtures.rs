//! Shared multi-byte-character test fixtures, referenced by the unit-test
//! layer (`src/vm.rs`), the CLI-integration layer
//! (`tests/cli_integration/b10.rs`), and the BDD step-definition layer
//! (`tests/features/steps_b10.rs`). Before this module existed, each layer
//! hand-rolled its own copy of the same handful of non-ASCII strings; a
//! Unicode-correctness fix landing in one copy did not visibly propagate to
//! the others, causing the same class of test gap to recur three separate
//! times across B10's review cycle (qa test-design reviews, msgs #167,
//! #170, #171, #186) before finally being centralized here.

/// One plain letter plus one accented (two-byte) character: exactly 2
/// characters, 3 UTF-8 bytes -- distinguishes character-counting from
/// byte-counting for `string-length`/`string-ref`.
pub const TWO_CHAR_ACCENTED: &str = "aé";

/// A 5-character, 6-byte string sharing the same one-accented-character
/// shape at a different length and position within the string.
pub const FIVE_CHAR_ACCENTED: &str = "héllo";

/// German sharp-s: its uppercase form is TWO letters ("SS"), so upcasing it
/// changes the string's length -- a length-changing Unicode case fold, not
/// just a per-character substitution.
pub const GERMAN_SHARP_S: &str = "straße";

/// A single non-ASCII alphabetic character, for `char-alphabetic?` and
/// similar per-character predicates/conversions.
pub const ACCENTED_LETTER: char = 'é';
