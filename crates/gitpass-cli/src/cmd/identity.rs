use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use gitpass_core::store;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Identity { cmd: IdentityCmd::Show } => {
            let s = unlock(cli, root)?;
            let (pk, fpr) = s.identity()?;
            println!("public key:  {}", gitpass_core::keys::encode_pub(&pk));
            println!("fingerprint: {fpr}");
            println!("file:        {}", root.join(store::IDENTITY_PUB_FILE).display());
        }
        _ => unreachable!(),
    }
    Ok(())
}
