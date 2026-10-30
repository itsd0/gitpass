use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use anyhow::Context;
use zeroize::Zeroizing;
use anyhow::bail;
use gitpass_core::store::Item;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Import { vault, from, file, dry_run, allow_duplicates } => {
            use gitpass_core::import;
            let resolved = import::resolve_import_path(file);
            let data = Zeroizing::new(std::fs::read(&resolved).with_context(|| format!("reading {}", resolved.display()))?);
            let fmt: import::ImportFormat = match from.as_deref() {
                Some(f) if !f.is_empty() => f.parse()?,
                _ => import::detect_format_bytes(&data)
                    .ok_or_else(|| anyhow::anyhow!("could not auto-detect format; specify --from <format>"))?,
            };
            let pass = if fmt == import::ImportFormat::GitpassJson && import::is_encrypted_export(&String::from_utf8_lossy(&data)) {
                Some(prompt_secret("GITPASS_EXPORT_PASSPHRASE", "Export passphrase: ")?) } else { None };
            let parsed = import::parse_bytes(fmt, &data, pass.as_deref().map(|s| s.as_str()))?;
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let r = import::import_items(&s, &vid, parsed, import::ImportOpts { dry_run: *dry_run, allow_duplicates: *allow_duplicates })?;
            for w in &r.warnings { eprintln!("warning: {w}"); }
            for d in &r.duplicates { eprintln!("duplicate skipped: {d}"); }
            println!("{}parsed {}, {} {}, {} duplicate(s) skipped", if *dry_run { "[dry run] " } else { "" },
                r.parsed, if *dry_run { "would add" } else { "added" }, r.added, r.duplicates.len());
        }
        Cmd::Export { vault, format, out, encrypted } => {
            use gitpass_core::import;
            let out = &std::path::absolute(out)?;
            if *encrypted && *format == ExportFmt::Csv { bail!("--encrypted only supports --format gitpass-json"); }
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let recs: Vec<_> = s.list_records(&vid)?.into_iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| r).collect();
            let items: Vec<Item> = recs.iter().map(|r| r.item.clone()).collect();
            let body = if *encrypted {
                let p = prompt_secret("GITPASS_EXPORT_PASSPHRASE", "Export passphrase: ")?;
                if std::env::var("GITPASS_EXPORT_PASSPHRASE").is_err() {
                    let p2 = prompt_secret("__none__", "Confirm export passphrase: ")?;
                    if *p != *p2 { bail!("passphrases do not match"); }
                }
                Zeroizing::new(import::export_records_encrypted(&recs, &p, &import::export_kdf_default())?)
            } else {
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                eprintln!("!! WARNING: PLAINTEXT EXPORT. {} will contain ALL passwords,", out.display());
                eprintln!("!! TOTP secrets and notes UNENCRYPTED. Anyone who reads the file owns");
                eprintln!("!! these accounts. Prefer --encrypted. Delete the file when done.");
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                if !cli.yes && !confirm("Write plaintext export?")? { bail!("aborted (pass --yes to confirm)"); }
                Zeroizing::new(match format { ExportFmt::GitpassJson => import::export_records_json(&recs)?, ExportFmt::Csv => import::export_csv(&items)? })
            };
            gitpass_core::fsutil::atomic_write(out, body.as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o600))?; }
            println!("exported {} item(s) to {}{}", items.len(), out.display(), if *encrypted { " (encrypted)" } else { " (PLAINTEXT)" });
        }
        _ => unreachable!(),
    }
    Ok(())
}
