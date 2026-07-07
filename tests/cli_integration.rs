//! Process-level acceptance tests: they invoke the real `magiclisp` binary as a
//! separate OS process, exactly the way a user's shell would, and check its
//! actual stdout/stderr/exit code. This is what proves the CLI as a whole
//! (not just the library functions behind it) satisfies each behaviour's
//! expectations.
//!
//! Split into one module per feature slice (qa test-design reviews, msg
//! #46 and #49) so the suite doesn't grow into a single ever-larger flat
//! file spanning unrelated feature eras.

#[path = "cli_integration/helpers.rs"]
mod helpers;

#[path = "cli_integration/b1.rs"]
mod b1;
#[path = "cli_integration/b2.rs"]
mod b2;
#[path = "cli_integration/b3.rs"]
mod b3;
#[path = "cli_integration/b4.rs"]
mod b4;
#[path = "cli_integration/b5.rs"]
mod b5;
