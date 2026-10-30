use anyhow::Result;
use crate::cli::Field;
use gitpass_pasteboard::PasteboardMarkers;

/// Clipboard markers for a copied field: passwords and TOTP codes are secrets; the rest are plain.
pub(crate) fn field_markers(f: Field) -> Option<PasteboardMarkers> {
    match f { Field::Password | Field::Totp => Some(PasteboardMarkers::SECRET), Field::Username | Field::Url | Field::Notes | Field::Title => None }
}

pub(crate) fn do_copy(val: &str, secs: u64, quiet: bool, label: &str, markers: Option<PasteboardMarkers>) -> Result<()> {
    match gitpass_cli::clip::system() {
        Ok(mut cb) => {
            if secs == 0 { gitpass_cli::clip::set_with_markers(cb.as_mut(), val, markers)?; eprintln!("copied {label} to clipboard (not auto-cleared)"); return Ok(()); }
            eprintln!("copied {label} to clipboard; clearing in {secs}s (keep this running, Ctrl-C leaves it)");
            let cleared = gitpass_cli::clip::copy_then_clear(cb.as_mut(), val, markers, std::time::Duration::from_secs(secs), std::thread::sleep)?;
            eprintln!("{}", if cleared { "clipboard cleared" } else { "clipboard changed since copy; left untouched" });
        }
        Err(e) => {
            eprintln!("warning: clipboard unavailable ({e}); {}", if quiet { "value not shown (--quiet)" } else { "printing to stdout" });
            if !quiet { println!("{val}"); }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::field_markers;
    use crate::cli::Field;
    use gitpass_pasteboard::PasteboardMarkers;

    #[test]
    fn copied_fields_are_classified_as_secret_or_plain() {
        assert_eq!(field_markers(Field::Password), Some(PasteboardMarkers::SECRET));
        assert_eq!(field_markers(Field::Totp), Some(PasteboardMarkers::SECRET));
        for f in [Field::Username, Field::Url, Field::Notes, Field::Title] {
            assert_eq!(field_markers(f), None);
        }
    }
}
