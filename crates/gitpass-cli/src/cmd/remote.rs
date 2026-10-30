use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use gitpass_core::store::UnlockOpts;
use gitpass_core::store::Session;
use anyhow::Context;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Remote { cmd } => match cmd {
            RemoteCmd::Add { url, allow_local } => {
                print!("{}", gitpass_core::remote::add(root, url, *allow_local)?);
                println!("connected: origin = {}", url.trim());
            }
            RemoteCmd::Show => {
                let i = gitpass_core::remote::show(root)?;
                match &i.url {
                    Some(u) => println!("origin: {u}\nbranch: {}\nahead: {}  behind: {}", i.branch.as_deref().unwrap_or("?"), i.ahead, i.behind),
                    None => println!("not connected (run: gitpass remote add git@github.com:YOU/REPO.git)"),
                }
            }
            RemoteCmd::Remove => { gitpass_core::remote::remove(root)?; println!("disconnected: removed 'origin' (local files and history kept)"); }
        },
        Cmd::Sync { merge } => {
            let (_s, pw, sk) = unlock_full(cli, root)?;
            if *merge {
                drop(_s);
                let opts = UnlockOpts { accept_rollback: cli.accept_rollback, target_kdf: gitpass_core::store::read_format(root)?.kdf };
                let r = gitpass_core::sync::sync_merge(root, &pw, &sk, &opts)?;
                print!("{}", r.log);
                for (vid, iid, m) in &r.merged {
                    println!("merged {vid}/{iid}: remote={:?} local={:?}", m.from_remote, m.from_local);
                    if m.has_conflicts() { println!("  CONFLICT (equal clocks, kept local): {:?}", m.conflicts); }
                }
                for (vid, iid) in &r.reapplied { println!("re-applied local-only item {vid}/{iid}"); }
            } else {
                print!("{}", gitpass_core::sync::sync(root)?);
            }
            // verify-only: no device/KDF writes after the push (keeps the working tree clean)
            let s = Session::reopen_verify(root, &pw, &sk, cli.accept_rollback)
                .context("post-pull integrity check failed; the pre-pull backup is in .gitpass-backup-*/")?;
            println!("post-pull integrity verified (manifest epoch {})", s.epoch()?);
        }
        _ => unreachable!(),
    }
    Ok(())
}
