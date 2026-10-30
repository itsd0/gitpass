use super::super::display::*;
use super::super::dto::*;
use super::App;
use crate::Unlocked;
use anyhow::{bail, Context, Result};
use gitpass_core::item::{FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item};
use gitpass_core::{import};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{PathBuf};
use zeroize::Zeroizing;

impl App {
    /// Import a file into a vault. `password` is the export password for a password-protected
    /// Bitwarden JSON export (ignored for other formats); encrypted gitpass exports stay CLI-only.
    pub fn import_file(&self, vault_id: &str, format: &str, path: &str, dry_run: bool, password: Option<&str>) -> Result<ImportResult> {
        let fmt: import::ImportFormat = format.parse()?;
        let path = path.trim();
        let path = if let Some(rest) = path.strip_prefix("~/") { dirs::home_dir().map(|h| h.join(rest).display().to_string()).unwrap_or_else(|| path.to_string()) } else { path.to_string() };
        let resolved = import::resolve_import_path(std::path::Path::new(&path));
        let data = Zeroizing::new(std::fs::read(&resolved).with_context(|| format!("cannot read {}", resolved.display()))?);
        if fmt == import::ImportFormat::GitpassJson && import::is_encrypted_export(&String::from_utf8_lossy(&data)) { bail!("encrypted gitpass exports need a passphrase; import them with the CLI"); }
        let parsed = import::parse_bytes(fmt, &data, password)?;
        let mut categories = BTreeMap::new();
        for r in &parsed.items { *categories.entry(r.category.as_str().to_string()).or_insert(0) += 1; }
        let preview = parsed.items.iter().take(50).map(|r| ImportPreviewItem { title: r.item.title.clone(), category: r.category,
            username: r.item.username.clone(), url: r.item.url.clone(), has_password: r.item.password.is_some() || r.view_sections().iter().flat_map(|s| s.fields.iter()).any(|f| f.kind == FieldKind::Concealed),
            has_totp: has_totp(r), tags: r.tags.clone(), fields: r.view_sections().iter().map(|s| s.fields.len()).sum(), archived: r.archived }).collect();
        let f = move |u: &mut Unlocked| {
            let r = import::import_items(&u.session, vault_id, parsed, import::ImportOpts { dry_run, allow_duplicates: false })?;
            Ok(ImportResult { parsed: r.parsed, added: r.added, duplicates: r.duplicates.len(), duplicate_titles: r.duplicates, warnings: r.warnings, categories, preview })
        };
        let r = if dry_run { self.with(f) } else { self.write(f) }?;
        if !dry_run && r.added > 0 { let _ = self.update_local(|l| l.onboarding.imported = true); }
        Ok(r)
    }
    /// Export a whole vault or an explicit selection to a user-chosen path. `csv` is login fields only;
    /// encrypted exports are gitpass-json. The file is written atomically and, on Unix, mode 0600.
    /// The passphrase and item contents never go into errors or logs.
    pub fn export_items(&self, req: &ExportRequest) -> Result<ExportResult> {
        let format = req.format.as_str();
        if !matches!(format, "gitpass-json" | "csv") { bail!("unknown export format '{}'", req.format); }
        if format == "csv" && req.encrypted { bail!("encrypted export only supports gitpass-json"); }
        let raw = req.path.trim();
        if raw.is_empty() { bail!("export path is required"); }
        let path = if let Some(rest) = raw.strip_prefix("~/") { dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| PathBuf::from(raw)) } else { PathBuf::from(raw) };
        let path = std::path::absolute(&path)?;
        self.with(|u| {
            let recs: Vec<ItemRecord> = match (&req.items, &req.vault_id) {
                (Some(refs), _) if !refs.is_empty() => {
                    let mut seen = BTreeSet::new(); let mut out = vec![];
                    for r in refs {
                        if !seen.insert((r.vault_id.clone(), r.item_id.clone())) { continue; }
                        let (_, rec) = u.session.read_record(&r.vault_id, &r.item_id)?;
                        if rec.deleted_at.is_none() { out.push(rec); }
                    }
                    out
                }
                (Some(_), _) => bail!("no items selected"),
                (None, Some(vid)) => u.session.list_records(vid)?.into_iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| r).collect(),
                (None, None) => bail!("export needs a vault or a list of items"),
            };
            if recs.is_empty() { bail!("nothing to export"); }
            let body = if req.encrypted {
                let pass = Zeroizing::new(req.passphrase.clone().unwrap_or_default());
                Zeroizing::new(import::export_records_encrypted(&recs, &pass, &import::export_kdf_default())?)
            } else if format == "gitpass-json" {
                Zeroizing::new(import::export_records_json(&recs)?)
            } else {
                let items: Vec<Item> = recs.iter().map(|r| r.item.clone()).collect();
                Zeroizing::new(import::export_csv(&items)?)
            };
            gitpass_core::fsutil::atomic_write(&path, body.as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?; }
            Ok(ExportResult { path: path.display().to_string(), items: recs.len(), format: format.to_string(), encrypted: req.encrypted })
        })
    }

}
