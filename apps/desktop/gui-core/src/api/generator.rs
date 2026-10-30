use anyhow::Result;

/// Separators the desktop generator offers for word passphrases; anything else is a bad request.
pub const WORD_SEPARATORS: [char; 4] = ['-', '.', '_', ' '];

/// Generate a password (`Words`: `length` words from the EFF large wordlist (7,776 words) joined by `separator`, ~12.9 bits/word;
/// `Pin`: `length` digits). Defaults, limits and the separator rule come from gitpass-core (an omitted
/// separator is `-`, or `.` with symbols off); an out-of-range length or an unoffered separator is an error.
pub fn generate(r: &gitpass_core::generate::Request) -> Result<String> {
    if let Some(c) = r.separator {
        if !WORD_SEPARATORS.contains(&c) {
            anyhow::bail!("invalid word separator {c:?}: use one of -, ., _, space");
        }
    }
    let opts = gitpass_core::generate::Options::for_request(r).map_err(|m| anyhow::anyhow!(m))?;
    Ok(gitpass_core::generate::generate(&opts))
}

