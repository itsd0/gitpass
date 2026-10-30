use anyhow::{bail, Result};
use crate::cli::AddCategory;
use std::path::PathBuf;

pub(crate) fn validate_date_iso(val: &str) -> Result<()> {
    let parts: Vec<&str> = val.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        bail!("invalid expires date format '{val}', expected YYYY-MM-DD");
    }
    let (_y, m, d): (i32, u32, u32) = match (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
        (Ok(y), Ok(m), Ok(d)) => (y, m, d),
        _ => bail!("invalid expires date format '{val}', expected YYYY-MM-DD"),
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        bail!("invalid expires date '{val}': month must be 01-12, day 01-31");
    }
    Ok(())
}

pub(crate) fn validate_card_expiry(val: &str) -> Result<()> {
    let parts: Vec<&str> = val.split('/').collect();
    if parts.len() != 2 {
        bail!("invalid card expiry '{val}', expected MM/YY");
    }
    let (m, _y): (u32, u32) = match (parts[0].parse(), parts[1].parse()) {
        (Ok(m), Ok(y)) => (m, y),
        _ => bail!("invalid card expiry '{val}', expected MM/YY"),
    };
    if !(1..=12).contains(&m) {
        bail!("invalid card expiry month in '{val}', expected 01-12");
    }
    if !(parts[1].len() == 2 || parts[1].len() == 4) {
        bail!("invalid card expiry year in '{val}', expected 2 or 4 digits (MM/YY)");
    }
    Ok(())
}

pub(crate) struct AddFlags<'a> {
    pub(crate) category: AddCategory,
    pub(crate) username: &'a Option<String>,
    pub(crate) url: &'a Option<String>,
    pub(crate) totp: &'a Option<String>,
    pub(crate) generate: &'a Option<String>,
    pub(crate) password_stdin: bool,
    pub(crate) credential_stdin: bool,
    pub(crate) expires: &'a Option<String>,
    pub(crate) notes: &'a Option<String>,
    pub(crate) notes_stdin: bool,
    pub(crate) holder: &'a Option<String>,
    pub(crate) expiry: &'a Option<String>,
    pub(crate) bank: &'a Option<String>,
    pub(crate) account_type: &'a Option<String>,
    pub(crate) routing: &'a Option<String>,
    pub(crate) branch_phone: &'a Option<String>,
    pub(crate) branch_address: &'a Option<String>,
    pub(crate) account_number_stdin: bool,
    pub(crate) pin_stdin: bool,
    pub(crate) admin_url: &'a Option<String>,
    pub(crate) provider: &'a Option<String>,
    pub(crate) first_name: &'a Option<String>,
    pub(crate) last_name: &'a Option<String>,
    pub(crate) email: &'a Option<String>,
    pub(crate) phone: &'a Option<String>,
    pub(crate) address: &'a Option<String>,
    pub(crate) ssid: &'a Option<String>,
    pub(crate) security: &'a Option<String>,
    pub(crate) private_key_file: &'a Option<PathBuf>,
}

pub(crate) fn check_no_identity(flags: &AddFlags) -> Result<()> {
    if flags.first_name.is_some()
        || flags.last_name.is_some()
        || flags.email.is_some()
        || flags.phone.is_some()
        || flags.address.is_some()
    {
        bail!("identity flags are only valid for identity items");
    }
    Ok(())
}

pub(crate) fn check_no_wifi(flags: &AddFlags) -> Result<()> {
    if flags.ssid.is_some() || flags.security.is_some() {
        bail!("wifi flags are only valid for wifi items");
    }
    Ok(())
}

pub(crate) fn check_no_ssh(flags: &AddFlags) -> Result<()> {
    if flags.private_key_file.is_some() {
        bail!("--private-key-file is only valid for ssh-key items");
    }
    Ok(())
}
