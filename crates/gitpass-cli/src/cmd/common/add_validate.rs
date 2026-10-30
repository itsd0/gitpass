use super::add_flags::{
    check_no_identity, check_no_ssh, check_no_wifi, validate_card_expiry, validate_date_iso,
    AddFlags,
};
use anyhow::{bail, Result};
use crate::cli::AddCategory;

pub(crate) fn validate_add_flags(flags: &AddFlags) -> Result<()> {
    match flags.category {
        AddCategory::Login => {
            if flags.password_stdin && flags.generate.is_some() {
                bail!("cannot combine --generate and --password-stdin");
            }
            if let Some(g) = flags.generate {
                if g.parse::<usize>().is_err() {
                    bail!("invalid password length '{g}', expected a number");
                }
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is only valid for api-credential items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is only valid for credit-card items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is only valid for credit-card items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::ApiCredential => {
            if flags.totp.is_some() {
                bail!("--totp is only valid for login items");
            }
            if flags.generate.is_some() {
                bail!("--generate is only valid for login, server, and ssh-key items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is only valid for login and server items (use --credential-stdin for API credentials)");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is only valid for credit-card items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is only valid for credit-card items (use --expires for API credentials)");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
            if let Some(exp) = flags.expires {
                validate_date_iso(exp)?;
            }
        }
        AddCategory::SecureNote => {
            if flags.username.is_some() {
                bail!("--username is not supported for secure-note items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for secure-note items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for secure-note items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for secure-note items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for secure-note items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for secure-note items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for secure-note items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            if flags.notes.is_some() && flags.notes_stdin {
                bail!("cannot combine --notes and --notes-stdin");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::CreditCard => {
            if flags.username.is_some() {
                bail!("--username is not supported for credit-card items (use --holder for cardholder name)");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for credit-card items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for credit-card items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for credit-card items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for credit-card items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for credit-card items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is not supported for credit-card items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items (use --expiry for credit cards)");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
            if let Some(exp) = flags.expiry {
                validate_card_expiry(exp)?;
            }
        }
        AddCategory::BankAccount => {
            if flags.username.is_some() {
                bail!("--username is not supported for bank-account items (use --holder for account holder)");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for bank-account items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for bank-account items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for bank-account items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for bank-account items (use --account-number-stdin or --pin-stdin)");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for bank-account items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is not supported for bank-account items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for bank-account items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Server => {
            if flags.password_stdin && flags.generate.is_some() {
                bail!("cannot combine --generate and --password-stdin");
            }
            if let Some(g) = flags.generate {
                if g.parse::<usize>().is_err() {
                    bail!("invalid password length '{g}', expected a number");
                }
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for server items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is only valid for api-credential items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for server items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for server items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Identity => {
            if flags.username.is_some() {
                bail!("--username is not supported for identity items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for identity items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for identity items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for identity items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for identity items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for identity items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for identity items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for identity items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for identity items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Wifi => {
            if flags.ssid.as_deref().unwrap_or("").trim().is_empty() {
                bail!("missing required field: --ssid is required for wifi items");
            }
            if let Some(sec) = flags.security.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                match sec.to_ascii_lowercase().as_str() {
                    "wpa2" | "wpa3" | "wep" | "none" | "wpa" | "wpa/wpa2" | "wpa2/wpa3" => {},
                    _ => bail!("invalid wifi security '{sec}', expected WPA2, WPA3, WEP, or none"),
                }
            }
            if flags.username.is_some() {
                bail!("--username is not supported for wifi items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for wifi items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for wifi items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for wifi items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for wifi items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for wifi items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for wifi items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for wifi items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::SshKey => {
            if flags.generate.is_some() && flags.private_key_file.is_some() {
                bail!("cannot combine --generate and --private-key-file");
            }
            if flags.generate.is_none() && flags.private_key_file.is_none() {
                bail!("missing required option for ssh-key items: specify either --generate ed25519 or --private-key-file <PATH>");
            }
            if let Some(g) = flags.generate {
                let g_norm = g.trim().to_ascii_lowercase();
                if g_norm != "ed25519" && g != "24" {
                    bail!("unsupported key type '{g}' for --generate: gitpass currently supports generating 'ed25519' SSH keys");
                }
            }
            if flags.username.is_some() {
                bail!("--username is not supported for ssh-key items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for ssh-key items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for ssh-key items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for ssh-key items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for ssh-key items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for ssh-key items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for ssh-key items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for ssh-key items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
        }
    }
    Ok(())
}
