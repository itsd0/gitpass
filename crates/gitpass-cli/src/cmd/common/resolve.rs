use super::output::field_value;
use anyhow::{bail, Context, Result};
use crate::cli::Field;
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session};
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Default output file name for `attach get` without `-o`: only the final path component of the
/// stored name (which comes from synced vault data and must not be trusted), so names like
/// `../../.ssh/authorized_keys` or `/etc/passwd` can never write outside the current directory.
pub(crate) fn safe_attachment_filename(name: &str) -> PathBuf {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    match std::path::Path::new(base).file_name() {
        Some(f) if !f.is_empty() => PathBuf::from(f),
        _ => PathBuf::from("attachment"),
    }
}

pub(crate) fn find_attachment<'a>(attachments: &'a [gitpass_core::item::AttachmentMeta], target: &str) -> Result<&'a gitpass_core::item::AttachmentMeta> {
    if let Some(a) = attachments.iter().find(|a| a.id == target) {
        return Ok(a);
    }
    let by_name: Vec<_> = attachments.iter().filter(|a| a.name == target).collect();
    match by_name.len() {
        1 => Ok(by_name[0]),
        0 => {
            let by_prefix: Vec<_> = attachments.iter().filter(|a| a.id.starts_with(target)).collect();
            match by_prefix.len() {
                1 => Ok(by_prefix[0]),
                0 => bail!("no attachment matches '{target}'"),
                n => bail!("{n} attachments match prefix '{target}'; use the full attachment id"),
            }
        }
        n => bail!("{n} attachments named '{target}'; specify the attachment id"),
    }
}

pub(crate) fn resolve_item_across_vaults(s: &Session, vault: Option<&str>, item: &str) -> Result<(String, String)> {
    if let Some(v) = vault {
        let vid = s.resolve_vault(v)?;
        let iid = s.resolve_item(&vid, item)?;
        return Ok((vid, iid));
    }
    if let Some((v, q)) = item.split_once('/') {
        let vid = s.resolve_vault(v)?;
        let iid = s.resolve_item(&vid, q)?;
        return Ok((vid, iid));
    }
    let mut matches = Vec::new();
    for (vid, _) in s.list_vaults()? {
        if let Ok(m) = s.find_items(&vid, item) {
            for (iid, it) in m {
                matches.push((vid.clone(), iid, it.title.clone()));
            }
        }
    }
    match matches.len() {
        0 => bail!("no item matches '{item}'"),
        1 => Ok((matches[0].0.clone(), matches[0].1.clone())),
        _ => bail!("multiple items match '{item}'; specify --vault or <vault>/<item>"),
    }
}

/// Parse `--rotate-every`: `30d`, `90d`, `180d`, `365d` (the `d` is optional) or `none`.
pub(crate) fn parse_rotation(v: &str) -> Result<Option<u32>> {
    let t = v.trim().to_lowercase();
    if t == "none" || t == "off" { return Ok(None); }
    let days = t.strip_suffix('d').unwrap_or(&t).parse::<u32>().ok().filter(|d| gitpass_core::merge::ROTATION_DAYS.contains(d));
    match days {
        Some(d) => Ok(Some(d)),
        None => bail!("--rotate-every must be 30d, 90d, 180d, 365d or none (got '{v}')"),
    }
}

/// Ranked lookup; errors (listing candidates) unless one clear winner or --pick.
pub(crate) fn pick_record(s: &Session, vid: &str, query: &str, pick: Option<usize>, include_archived: bool) -> Result<(String, ItemRecord)> {
    let mut recs = s.list_records(vid)?;
    if !include_archived {
        recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
    }
    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_default();
    let ranked = gitpass_core::search::rank_by_candidate(
        query,
        recs.into_iter().map(|(id, _, rec)| (id, rec)),
        |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
    );

    if ranked.is_empty() {
        bail!("no item matches '{query}'");
    }
    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, id, rec)) in ranked.iter().enumerate() {
                    eprintln!("{:>3}. {id}  {}  {}  (score {sc})", i + 1, rec.item.title, rec.item.username.clone().unwrap_or_default());
                }
                bail!("{} items match '{query}'; use --pick <n> or the item id", ranked.len());
            }
        },
    };
    let (_, id, rec) = ranked.into_iter().nth(idx).unwrap();
    Ok((id, rec))
}

pub(crate) fn pick_item(s: &Session, vid: &str, query: &str, pick: Option<usize>) -> Result<(String, Item)> {
    let (id, rec) = pick_record(s, vid, query, pick, false)?;
    Ok((id, rec.item.clone()))
}

pub(crate) fn resolve_target_item(
    s: &Session,
    vault_flag: Option<&str>,
    pos1: &str,
    pos2: Option<&str>,
    pick: Option<usize>,
    include_archived: bool,
) -> Result<(String, String, ItemRecord)> {
    let (vault_str, query_str) = match pos2 {
        Some(q) => (Some(pos1), q),
        None => match pos1.split_once('/') {
            Some((v, q)) => (Some(v), q),
            None => (vault_flag, pos1),
        },
    };

    if let Some(v) = vault_str {
        let vid = s.resolve_vault(v)?;
        let (iid, rec) = pick_record(s, &vid, query_str, pick, include_archived)?;
        return Ok((vid, iid, rec));
    }

    let mut all_recs = Vec::new();
    for (vid, _) in s.list_vaults()? {
        let mut recs = s.list_records(&vid)?;
        if !include_archived {
            recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
        }
        for (id, _, rec) in recs {
            all_recs.push((vid.clone(), id, rec));
        }
    }

    let ranked = gitpass_core::search::rank_by(
        query_str,
        all_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
        |(_, _, rec)| &rec.item,
    );

    if ranked.is_empty() {
        bail!("no item matches '{query_str}'");
    }

    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, _, (vid, id, rec))) in ranked.iter().enumerate() {
                    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_else(|_| vid.clone());
                    eprintln!(
                        "{:>3}. [{vname}] {id}  {}  {}  (score {sc})",
                        i + 1,
                        rec.item.title,
                        rec.item.username.clone().unwrap_or_default()
                    );
                }
                bail!("{} items match '{query_str}'; use --pick <n> or specify --vault", ranked.len());
            }
        },
    };

    let (_, _, (vid, id, rec)) = ranked.into_iter().nth(idx).unwrap();
    Ok((vid, id, rec))
}

pub(crate) fn resolve_deleted_item(
    s: &Session,
    vault_flag: Option<&str>,
    pos1: &str,
    pos2: Option<&str>,
    pick: Option<usize>,
) -> Result<(String, String, ItemRecord)> {
    let (vault_str, query_str) = match pos2 {
        Some(q) => (Some(pos1), q),
        None => match pos1.split_once('/') {
            Some((v, q)) => (Some(v), q),
            None => (vault_flag, pos1),
        },
    };

    let search_vaults: Vec<String> = if let Some(v) = vault_str {
        vec![s.resolve_vault(v)?]
    } else {
        s.list_vaults()?.into_iter().map(|(v, _)| v).collect()
    };

    let mut deleted_recs = Vec::new();
    let mut non_deleted_recs = Vec::new();

    for vid in &search_vaults {
        for (id, _, rec) in s.list_records(vid)? {
            if rec.deleted_at.is_some() {
                deleted_recs.push((vid.clone(), id, rec));
            } else {
                non_deleted_recs.push((vid.clone(), id, rec));
            }
        }
    }

    let ranked = gitpass_core::search::rank_by(
        query_str,
        deleted_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
        |(_, _, rec)| &rec.item,
    );

    if ranked.is_empty() {
        let active_ranked = gitpass_core::search::rank_by(
            query_str,
            non_deleted_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
            |(_, _, rec)| &rec.item,
        );
        if let Some((_, _, (_, _, active_rec))) = active_ranked.into_iter().next() {
            bail!("item '{}' is not deleted", active_rec.item.title);
        }
        bail!("no deleted item matches '{query_str}'");
    }

    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, _, (vid, id, rec))) in ranked.iter().enumerate() {
                    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_else(|_| vid.clone());
                    eprintln!(
                        "{:>3}. [{vname}] {id}  {}  {}  (score {sc})",
                        i + 1,
                        rec.item.title,
                        rec.item.username.clone().unwrap_or_default()
                    );
                }
                bail!("{} deleted items match '{query_str}'; use --pick <n> or the item id", ranked.len());
            }
        },
    };

    let (_, _, (vid, id, rec)) = ranked.into_iter().nth(idx).unwrap();
    Ok((vid, id, rec))
}

pub(crate) struct SessionResolver<'a>(pub(crate) &'a Session);
impl gitpass_cli::inject::Resolver for SessionResolver<'_> {
    fn resolve(&self, r: &gitpass_cli::inject::SecretRef) -> Result<Zeroizing<String>> {
        let s = self.0;
        let vid = s.resolve_vault(&r.vault).with_context(|| format!("gp://{}/...: vault", r.vault))?;
        let items = s.list_items(&vid)?;
        let exact: Vec<&(String, Item)> = items.iter().filter(|(id, it)| *id == r.item || it.title.eq_ignore_ascii_case(&r.item)).collect();
        let iid = match exact.len() {
            1 => exact[0].0.clone(),
            0 => s.resolve_item(&vid, &r.item).with_context(|| format!("gp://{}/{}: item", r.vault, r.item))?,
            n => bail!("gp://{}/{}: {n} items have that title; use the item id", r.vault, r.item),
        };
        let (_, rec) = s.read_record(&vid, &iid)?;
        let f = r.field.to_ascii_lowercase();
        let std = match f.as_str() {
            "password" => Some(Field::Password), "username" => Some(Field::Username), "url" => Some(Field::Url),
            "notes" => Some(Field::Notes), "title" => Some(Field::Title), "totp" | "otp" => Some(Field::Totp), _ => None,
        };
        if let Some(sf) = std { return field_value(&rec.item, sf); }
        let hits: Vec<&gitpass_core::item::Field> = rec.sections.iter().flat_map(|sec| sec.fields.iter())
            .filter(|x| !x.removed && (x.label.eq_ignore_ascii_case(&r.field) || x.id == r.field)).collect();
        match hits.as_slice() {
            [one] => one.value.clone().map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("field '{}' is empty", r.field)),
            [] => bail!("gp://{}/{}/{}: no such field", r.vault, r.item, r.field),
            _ => bail!("gp://{}/{}/{}: several fields have that label; use the field id", r.vault, r.item, r.field),
        }
    }
}
