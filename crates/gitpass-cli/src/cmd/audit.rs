use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Audit { vault, hibp, max_age_days, expiring_days, unused_months, json, include_ignored } => {
            use gitpass_core::{audit, passkey, Category};
            let s = unlock(cli, root)?;
            let vids: Vec<(String, String)> = match vault {
                Some(v) => { let id = s.resolve_vault(v)?; vec![(id.clone(), s.vault_meta(&id)?.name.clone())] }
                None => s.list_vaults()?.into_iter().map(|(id, m)| (id, m.name.clone())).collect(),
            };
            let mut all = vec![];
            let mut passkeys = std::collections::BTreeSet::new();
            for (vid, name) in &vids {
                let mut rows = vec![];
                for (iid, _, rec) in s.list_records(vid)? {
                    if rec.deleted_at.is_some() || rec.archived { continue; }
                    if rec.category == Category::Passkey {
                        if let Some(v) = rec.find_field(passkey::F_RP_ID).and_then(|f| f.value) { passkeys.insert(v); }
                    } else {
                        rows.push((name.clone(), iid, rec));
                    }
                }
                rows.sort_by(|a, b| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()));
                all.extend(rows);
            }
            let client = UreqHibp(ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build());
            let opts = audit::AuditOpts { max_age_days: *max_age_days, expiring_days: *expiring_days, include_ignored: *include_ignored,
                unused_months: (*unused_months > 0).then_some(*unused_months), last_used: s.all_last_used(), ..Default::default() };
            let r = audit::audit_with_passkeys(&all, &passkeys, &opts, if *hibp { Some(&client) } else { None })?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&r)?);
                return Ok(());
            }
            println!("audited {} item(s) in {} vault(s){}", r.items_checked, vids.len(), if *hibp { " (with HIBP)" } else { "" });
            let unused_label = if *unused_months > 0 { format!(">{unused_months}mo") } else { "off".to_string() };
            println!("  weak: {}  reused: {} ({} group(s))  duplicates: {}  old (>{max_age_days}d): {}  expired: {}  expiring soon (<={expiring_days}d): {}  insecure URLs: {}  missing TOTP: {}  2FA available: {}  passkeys available: {}{}  weak SSH keys: {}  unused ({unused_label}): {}  rotation due: {}  rotation soon: {}  ignored: {}",
                r.weak, r.reused, r.reused_groups, r.duplicates, r.old, r.expired, r.expiring_soon, r.insecure_urls, r.missing_totp, r.totp_available, r.passkey_available,
                if *hibp { format!("  pwned: {}", r.pwned) } else { String::new() },
                r.weak_ssh_keys,
                r.unused,
                r.rotation_due,
                r.rotation_soon,
                r.ignored);
            for f in &r.per_item {
                println!("\n[{}] {} ({})", f.vault, f.title, &f.id[..f.id.len().min(8)]);
                for x in &f.findings { println!("  - {x}"); }
            }
            if *include_ignored && !r.ignored_per_item.is_empty() {
                println!("\nignored findings ({}):", r.ignored);
                for f in &r.ignored_per_item {
                    println!("\n[{}] {} ({}) [ignored]", f.vault, f.title, &f.id[..f.id.len().min(8)]);
                    for x in &f.findings { println!("  - {x} (ignored)"); }
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
