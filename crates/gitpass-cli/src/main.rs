mod cli;
mod cmd;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd, print_completions};
use cmd::common::root;

fn main() {
    if let Err(e) = run() { eprintln!("error: {e:#}"); std::process::exit(1); }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Cmd::Completions { shell } = &cli.cmd {
        print_completions(*shell);
        return Ok(());
    }
    let root = root(&cli)?;
    match &cli.cmd {
        Cmd::Init { .. } | Cmd::Unlock | Cmd::Passwd | Cmd::RotateSecretKey { .. }
            | Cmd::RotateAccountKey { .. } | Cmd::Migrate => cmd::keys::run(&cli, &root, &cli.cmd),
        Cmd::Vault { .. } => cmd::vault::run(&cli, &root, &cli.cmd),
        Cmd::Ssh { cmd } => cmd::ssh::ssh_cmd(&cli, &root, cmd),
        Cmd::Identity { .. } => cmd::identity::run(&cli, &root, &cli.cmd),
        Cmd::Attach { .. } => cmd::attach::run(&cli, &root, &cli.cmd),
        Cmd::Add { .. } | Cmd::Get { .. } | Cmd::Show { .. } | Cmd::Copy { .. } | Cmd::Totp { .. } | Cmd::Find { .. } | Cmd::List { .. } | Cmd::Edit { .. } | Cmd::Archive { .. } | Cmd::Unarchive { .. } | Cmd::Undelete { .. } | Cmd::Mv { .. } | Cmd::Rm { .. } | Cmd::History { .. } | Cmd::Restore { .. } | Cmd::Ref { .. } => cmd::item::run(&cli, &root, &cli.cmd),
        Cmd::Gen { .. } | Cmd::Run { .. } | Cmd::Inject { .. } => cmd::tools::run(&cli, &root, &cli.cmd),
        Cmd::Remote { .. } | Cmd::Sync { .. } => cmd::remote::run(&cli, &root, &cli.cmd),
        Cmd::Device { .. } => cmd::device::run(&cli, &root, &cli.cmd),
        Cmd::Import { .. } | Cmd::Export { .. } => cmd::import_export::run(&cli, &root, &cli.cmd),
        Cmd::Audit { .. } => cmd::audit::run(&cli, &root, &cli.cmd),
        Cmd::Completions { .. } => unreachable!(),
    }
}
