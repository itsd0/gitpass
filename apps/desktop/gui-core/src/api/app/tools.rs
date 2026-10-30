use super::super::display::*;
use super::super::dto::*;
use super::super::passkey::passkey_site_url;
use super::App;
use anyhow::{Result};
use gitpass_core::audit::{self, AuditOpts, Finding, PwnedRangeClient};
use gitpass_core::item::{Category};
use gitpass_core::merge::ItemRecord;
use std::collections::{BTreeMap, BTreeSet, HashMap};

impl App {
    // ---------------- tools
    pub fn get_generator_history(&self) -> Result<Vec<GeneratorHistoryEntryDto>> {
        self.with(|u| {
            let gh = u.session.read_generator_history()?;
            Ok(gh.entries.iter().map(|e| GeneratorHistoryEntryDto {
                id: e.id.clone(),
                value: e.value.clone(),
                created_at: e.created_at,
                source: e.source.clone(),
                host: e.host.clone(),
            }).collect())
        })
    }

    pub fn delete_generator_history_entry(&self, id: &str) -> Result<bool> {
        self.with(|u| {
            crate::GuiState::writable(u)?;
            u.session.delete_generator_history_entry(id)
        })
    }

    pub fn clear_generator_history(&self) -> Result<()> {
        self.with(|u| {
            crate::GuiState::writable(u)?;
            u.session.clear_generator_history()
        })
    }

    pub fn record_generated_password(&self, value: &str, source: &str, host: Option<String>) -> Result<Option<GeneratorHistoryEntryDto>> {
        let remember = self.get_settings().remember_generated_passwords;
        if !remember {
            return Ok(None);
        }
        self.with(|u| {
            crate::GuiState::writable(u)?;
            let e = u.session.record_generated_password(value, source, host)?;
            Ok(Some(GeneratorHistoryEntryDto {
                id: e.id.clone(),
                value: e.value.clone(),
                created_at: e.created_at,
                source: e.source.clone(),
                host: e.host.clone(),
            }))
        })
    }

    pub fn password_strength(&self, password: &str) -> u8 { strength(password) }

    /// The site's standard change-password URL for an item. Derived in Rust so the webview never
    /// hands an arbitrary URL to the OS; `None` when the item has no usable http(s) URL.
    pub fn item_change_password_url(&self, vault_id: &str, item_id: &str) -> Result<Option<String>> {
        self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            Ok(r.item.url.as_deref().and_then(audit::change_password_url))
        })
    }

    /// The passkey's site URL (`https://<rp_id>`), derived in Rust so the webview never
    /// hands an arbitrary URL to the OS; `None` when the item has no usable rp_id.
    pub fn item_passkey_site_url(&self, vault_id: &str, item_id: &str) -> Result<Option<String>> {
        self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            Ok(passkey_site_url(
                r.find_field(gitpass_core::passkey::F_RP_ID).and_then(|f| f.value).as_deref(),
                r.item.url.as_deref(),
            ))
        })
    }

    /// Watchtower over live login items, reusing core `audit` (HIBP only when `hibp`).
    pub fn watchtower(&self, hibp: bool) -> Result<WatchtowerReport> {
        // Network breach check only when the caller asks AND the user opted in via Settings.hibpCheck.
        let settings = self.get_settings();
        let hibp = hibp && settings.hibp_check;
        let unused_months = settings.watchtower_unused_months;
        let client: Option<&dyn PwnedRangeClient> = if hibp { Some(self.hibp.as_ref() as &dyn PwnedRangeClient) } else { None };
        self.with(|u| {
            let records = Self::all_records(&u.session)?;
            let passkey_sites: BTreeSet<String> = records.iter()
                .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category == Category::Passkey)
                .filter_map(|(_, _, r)| r.find_field(gitpass_core::passkey::F_RP_ID).and_then(|f| f.value))
                .collect();
            let items: Vec<(String, String, ItemRecord)> = records.into_iter()
                .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category != Category::Passkey)
                .collect();
            let last_used = u.session.all_last_used();
            let opts = AuditOpts { unused_months: (unused_months > 0).then_some(unused_months), last_used: last_used.clone(), ..AuditOpts::default() };
            let rep = audit::audit_with_passkeys(&items, &passkey_sites, &opts, client)?;
            let mut out = WatchtowerReport { score: 100, weak: vec![], reused: vec![], old: vec![], breached: vec![], missing_totp: vec![],
                totp_available: vec![], passkey_available: vec![], expired: vec![], expiring_soon: vec![],
                insecure_urls: vec![],
                duplicates: vec![],
                weak_ssh_keys: vec![],
                reused_old_password: vec![],
                unused: vec![],
                rotation_due: vec![],
                rotation_soon: vec![],
                weak_reasons: HashMap::new(),
                ignored: vec![],
                checked_at: gitpass_core::device::now_iso8601(), hibp_checked: hibp };
            let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            let mut dup_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for f in &rep.per_item {
                for x in &f.findings {
                    match x {
                        Finding::Weak { reason } => {
                            out.weak.push(f.id.clone());
                            out.weak_reasons.insert(f.id.clone(), reason.clone());
                        }
                        Finding::Reused { group, .. } => groups.entry(*group).or_default().push(f.id.clone()),
                        Finding::Old { .. } => out.old.push(f.id.clone()),
                        Finding::Pwned { .. } => out.breached.push(f.id.clone()),
                        Finding::MissingTotp => out.missing_totp.push(f.id.clone()),
                        Finding::TotpAvailable { .. } => out.totp_available.push(f.id.clone()),
                        Finding::PasskeyAvailable { .. } => out.passkey_available.push(f.id.clone()),
                        Finding::Expired { .. } => out.expired.push(f.id.clone()),
                        Finding::ExpiringSoon { .. } => out.expiring_soon.push(f.id.clone()),
                        Finding::InsecureUrl { .. } => out.insecure_urls.push(f.id.clone()),
                        Finding::Duplicate { group } => dup_groups.entry(group.clone()).or_default().push(f.id.clone()),
                        Finding::WeakSshKey { .. } => out.weak_ssh_keys.push(f.id.clone()),
                        Finding::ReusedOldPassword { other_title, retired_on, same_item } => out.reused_old_password.push(ReusedOldPasswordItem {
                            item_id: f.id.clone(), other_title: other_title.clone(), retired_on: retired_on.clone(), same_item: *same_item,
                        }),
                        Finding::Unused { days } => out.unused.push(UnusedItem {
                            item_id: f.id.clone(), days: *days, last_used: last_used.contains_key(&f.id),
                        }),
                        Finding::RotationDue { every_days, days } => out.rotation_due.push(RotationItem { item_id: f.id.clone(), every_days: *every_days, days: *days }),
                        Finding::RotationSoon { every_days, in_days } => out.rotation_soon.push(RotationItem { item_id: f.id.clone(), every_days: *every_days, days: *in_days }),
                    }
                }
            }
            out.reused = groups.into_values().collect();
            out.duplicates = dup_groups.into_values().collect();
            for f in &rep.ignored_per_item {
                for x in &f.findings {
                    out.ignored.push(IgnoredFindingItem {
                        item_id: f.id.clone(),
                        vault_id: f.vault.clone(),
                        title: f.title.clone(),
                        kind: x.kind().to_string(),
                        description: x.to_string(),
                    });
                }
            }
            let with_pw = items.iter().filter(|(_, _, i)| audit::is_score_password_item(i)).count();
            out.score = audit::score(&rep, with_pw);
            Ok(out)
        })
    }

}
