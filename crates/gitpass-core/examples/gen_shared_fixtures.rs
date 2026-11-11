//! Regenerate (or verify) the shared encrypted test-vault fixtures
//! (issue #451). See `gitpass_core::test_fixtures` for what the fixtures
//! contain and why.
//!
//! ```sh
//! # Regenerate the committed fixtures + expected.json files:
//! cargo run --locked --example gen_shared_fixtures
//!
//! # Verify the generator's output matches the committed expected.json
//! # (what CI runs, via the `shared_fixtures` integration test):
//! cargo run --locked --example gen_shared_fixtures -- --check
//!
//! # Regenerate into a different directory:
//! cargo run --locked --example gen_shared_fixtures -- /tmp/fixtures-out
//! ```
//!
//! Only the decrypted content (`expected.json`) is reproducible:
//! salts, nonces, account ids and device ids are random per generation,
//! so the encrypted bytes always differ. Regeneration therefore rewrites
//! every fixture file; review the `expected.json` diff, which is the
//! part that must stay stable unless the content intentionally changed.

use gitpass_core::test_fixtures;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut check = false;
    let mut out: Option<PathBuf> = None;
    for a in args.by_ref() {
        match a.as_str() {
            "--check" => check = true,
            "--help" | "-h" => {
                println!("usage: gen_shared_fixtures [--check] [OUT_DIR]");
                println!("default OUT_DIR: crates/gitpass-core/tests/fixtures/shared");
                return Ok(());
            }
            other => {
                if out.is_some() {
                    anyhow::bail!("unexpected argument {other}");
                }
                out = Some(PathBuf::from(other));
            }
        }
    }
    let out = out.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("shared")
    });
    if check {
        test_fixtures::check_all(&out)?;
        println!("generator output matches committed expected.json in {}", out.display());
    } else {
        test_fixtures::generate_all(&out)?;
        println!("wrote shared fixtures for v2-v5 to {}", out.display());
    }
    Ok(())
}
