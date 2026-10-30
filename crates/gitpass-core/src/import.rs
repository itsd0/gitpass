//! Data import / export.
//!
//! Importers parse vendor export files into [`ItemRecord`]s (category, typed sections, tags,
//! favorite/archived) and [`import_items`] writes them through the public [`Session`] API.
//! Column lookup in CSV formats is case-insensitive and accepts several aliases per field, because
//! vendors have changed their header names across versions. Any non-empty column that is not mapped
//! is appended to `notes` as `Column: value` so nothing is silently dropped. Folders / groups /
//! vendor vaults become tags (gitpass imports everything into the one vault picked by the user).

//! Split from the former single-file `import.rs` (see issue #383); this file is
//! now the module root, with the split-out parsers in `import/` alongside it,
//! and re-exports the previous public API unchanged.

mod bitwarden;
mod common;
mod csv;
mod detect;
mod export;
mod onepif;
mod pux;

pub use bitwarden::bitwarden_json_needs_password;
pub use common::{iso_to_epoch, parse_time};
pub use detect::{detect_format, detect_format_bytes};
pub use export::{
    export_csv, export_encrypted, export_json, export_kdf_default, export_records_encrypted,
    export_records_json, is_encrypted_export, ExportRecord,
};

use bitwarden::parse_bitwarden_json;
use csv::{
    parse_csv, APPLE_PASSWORDS_CSV, BITWARDEN_CSV, CHROME_CSV, GENERIC_CSV, KEEPASS_CSV,
    LASTPASS_CSV, PROTONPASS_CSV,
};
use export::parse_gitpass_json;
use onepif::parse_onepif;
use pux::{parse_1pux, parse_1pux_json};
use crate::merge::ItemRecord;
use crate::store::{Item, Session};
use anyhow::{anyhow, bail, Result};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat { BitwardenJson, BitwardenCsv, GenericCsv, Pux, OnePif, KeepassCsv, ChromeCsv, ApplePasswordsCsv, GitpassJson, LastpassCsv, ProtonPassCsv }

impl std::str::FromStr for ImportFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "bitwarden-json" => Self::BitwardenJson,
            "bitwarden-csv" => Self::BitwardenCsv,
            "generic-csv" | "1password-csv" => Self::GenericCsv,
            "1pux" | "1password-1pux" => Self::Pux,
            "1pif" | "onepif" => Self::OnePif,
            "keepass-csv" | "keepassxc-csv" => Self::KeepassCsv,
            "chrome-csv" => Self::ChromeCsv,
            "apple-passwords-csv" | "apple-passwords" | "apple-csv" | "apple" | "safari-csv" | "safari" => Self::ApplePasswordsCsv,
            "gitpass-json" => Self::GitpassJson,
            "lastpass-csv" | "lastpass" => Self::LastpassCsv,
            "protonpass-csv" | "proton-pass-csv" | "protonpass" | "proton-pass" => Self::ProtonPassCsv,
            o => bail!("unknown import format '{o}' (bitwarden-json, bitwarden-csv, generic-csv, 1pux, 1pif, keepass-csv, chrome-csv, apple-passwords-csv, gitpass-json, lastpass-csv, protonpass-csv)"),
        })
    }
}

/// Result of parsing a file: records plus human-readable warnings (skipped rows, unknown columns...).
#[derive(Default)]
pub struct Parsed { pub items: Vec<ItemRecord>, pub warnings: Vec<String> }

/// Parse text `data` in `format`. `passphrase` is needed for encrypted gitpass-json exports and for
/// password-protected Bitwarden JSON exports (it is the export password there).
/// (.1pux is a zip archive: use [`parse_bytes`].)
pub fn parse(format: ImportFormat, data: &str, passphrase: Option<&str>) -> Result<Parsed> {
    let data = data.strip_prefix('\u{feff}').unwrap_or(data); // tolerate UTF-8 BOM (Excel / other password managers)
    match format {
        ImportFormat::BitwardenJson => parse_bitwarden_json(data, passphrase),
        ImportFormat::GitpassJson => parse_gitpass_json(data, passphrase),
        ImportFormat::BitwardenCsv => parse_csv(data, &BITWARDEN_CSV),
        ImportFormat::GenericCsv => parse_csv(data, &GENERIC_CSV),
        ImportFormat::KeepassCsv => parse_csv(data, &KEEPASS_CSV),
        ImportFormat::ChromeCsv => parse_csv(data, &CHROME_CSV),
        ImportFormat::ApplePasswordsCsv => parse_csv(data, &APPLE_PASSWORDS_CSV),
        ImportFormat::Pux => parse_1pux_json(data),
        ImportFormat::OnePif => parse_onepif(data),
        ImportFormat::LastpassCsv => parse_csv(data, &LASTPASS_CSV),
        ImportFormat::ProtonPassCsv => parse_csv(data, &PROTONPASS_CSV),
    }
}

/// Parse raw file bytes (handles the .1pux zip container; other formats must be UTF-8 text).
pub fn parse_bytes(format: ImportFormat, data: &[u8], passphrase: Option<&str>) -> Result<Parsed> {
    if format == ImportFormat::Pux { return parse_1pux(data); }
    let s = std::str::from_utf8(data).map_err(|_| anyhow!("file is not UTF-8 text; is this the right format?"))?;
    parse(format, s, passphrase)
}

/// If `path` is a directory containing `data.1pif`, resolve to `data.1pif`, otherwise keep `path`.
pub fn resolve_import_path(path: &std::path::Path) -> std::path::PathBuf {
    if path.is_dir() && path.join("data.1pif").exists() {
        path.join("data.1pif")
    } else {
        path.to_path_buf()
    }
}

// ---------------------------------------------------------------- write into a vault

#[derive(Default, Debug, Clone, Copy)]
pub struct ImportOpts { pub dry_run: bool, pub allow_duplicates: bool }

#[derive(Default, Debug)]
pub struct ImportReport {
    pub parsed: usize,
    pub added: usize,
    /// Titles of skipped duplicates (same title + username + url as an existing or earlier item).
    pub duplicates: Vec<String>,
    pub warnings: Vec<String>,
}

fn dedup_key(it: &Item) -> (String, String, String) {
    let n = |x: &Option<String>| x.as_deref().unwrap_or("").trim().to_lowercase();
    (it.title.trim().to_lowercase(), n(&it.username), n(&it.url))
}

/// Add parsed records to vault `vid`. With `dry_run`, nothing is written but counts are computed.
/// Duplicates (same title + username + url, case-insensitive, against live items already in the vault
/// or earlier rows of the same file) are skipped unless `allow_duplicates`.
pub fn import_items(s: &Session, vid: &str, parsed: Parsed, opts: ImportOpts) -> Result<ImportReport> {
    let mut seen: HashSet<_> = s.list_records(vid)?.iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| dedup_key(&r.item)).collect();
    let mut r = ImportReport { parsed: parsed.items.len(), warnings: parsed.warnings, ..Default::default() };
    for rec in parsed.items {
        if !seen.insert(dedup_key(&rec.item)) && !opts.allow_duplicates { r.duplicates.push(rec.item.title.clone()); continue; }
        if !opts.dry_run { s.add_record(vid, rec)?; }
        r.added += 1;
    }
    Ok(r)
}
