use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use anyhow::bail;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Attach { cmd } => match cmd {
            AttachCmd::Add { vault, query, file } => {
                if !file.exists() {
                    bail!("file '{}' does not exist", file.display());
                }
                let data = zeroize::Zeroizing::new(std::fs::read(file)?);
                if data.len() > gitpass_core::item::MAX_ATTACHMENT_BYTES {
                    bail!("attachment too large ({} bytes exceeds {} MiB limit)", data.len(), gitpass_core::item::MAX_ATTACHMENT_BYTES / (1024 * 1024));
                }
                let s = unlock(cli, root)?;
                let vid = s.resolve_vault(vault)?;
                let (iid, rec) = pick_record(&s, &vid, query, None, false)?;
                let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".to_string());
                let mime = gitpass_core::item::mime_for(&name);
                let meta = s.add_attachment(&vid, &iid, &name, mime, &data)?;
                println!("added attachment '{}' ({}, {} bytes) to '{}'", meta.name, meta.id, meta.size, rec.item.title);
            }
            AttachCmd::Ls { vault, query, json } => {
                let s = unlock(cli, root)?;
                let vid = s.resolve_vault(vault)?;
                let (_iid, rec) = pick_record(&s, &vid, query, None, false)?;
                if *json {
                    let atts: Vec<CliAttachment> = rec.attachments.iter().map(|a| CliAttachment {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        size: a.size,
                        mime: a.mime.clone(),
                    }).collect();
                    println!("{}", serde_json::to_string_pretty(&atts)?);
                } else if rec.attachments.is_empty() {
                    println!("no attachments");
                } else {
                    for a in &rec.attachments {
                        println!("{:<36}  {:<10}  {}", a.id, a.size, a.name);
                    }
                }
            }
            AttachCmd::Get { vault, query, target, out, force } => {
                let s = unlock(cli, root)?;
                let vid = s.resolve_vault(vault)?;
                let (iid, rec) = pick_record(&s, &vid, query, None, false)?;
                let att = find_attachment(&rec.attachments, target)?;
                let dest = out.clone().unwrap_or_else(|| safe_attachment_filename(&att.name));
                if dest.exists() && !*force {
                    bail!("destination '{}' already exists (use --force to overwrite)", dest.display());
                }
                let data = zeroize::Zeroizing::new(s.read_attachment(&vid, &iid, &att.id)?);
                if let Some(p) = dest.parent() {
                    if !p.as_os_str().is_empty() {
                        std::fs::create_dir_all(p)?;
                    }
                }
                std::fs::write(&dest, &*data)?;
                println!("saved attachment '{}' to {}", att.name, dest.display());
            }
            AttachCmd::Rm { vault, query, target } => {
                let s = unlock(cli, root)?;
                let vid = s.resolve_vault(vault)?;
                let (iid, rec) = pick_record(&s, &vid, query, None, false)?;
                let att = find_attachment(&rec.attachments, target)?;
                let att_id = att.id.clone();
                let att_name = att.name.clone();
                s.remove_attachment(&vid, &iid, &att_id)?;
                println!("removed attachment '{}' ({}) from '{}'", att_name, att_id, rec.item.title);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
