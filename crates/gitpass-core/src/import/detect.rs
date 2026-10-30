use super::ImportFormat;
use serde_json::Value;

/// Auto-detect the import format from text content or headers.
pub fn detect_format(data: &str) -> Option<ImportFormat> {
    // Check .1pif separator and typeName
    if data.contains("\"typeName\"") && data.lines().any(|l| {
        let t = l.trim();
        t.starts_with("***") && t.ends_with("***") && t.len() >= 6
    }) {
        return Some(ImportFormat::OnePif);
    }
    let data = data.strip_prefix('\u{feff}').unwrap_or(data);
    let trimmed = data.trim_start();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if v.get("gitpass_export").is_some() || v.get("gitpass_encrypted_export").is_some() {
                return Some(ImportFormat::GitpassJson);
            }
            if v.get("items").is_some() || v.get("folders").is_some() || v.get("collections").is_some()
                || v.get("encKeyValidation_DO_NOT_EDIT").is_some()
                || (v.get("encrypted").and_then(Value::as_bool) == Some(true) && v.get("data").is_some()) {
                return Some(ImportFormat::BitwardenJson);
            }
        }
    }
    // Check CSV header
    let first_line = trimmed.lines().next()?;
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::Headers).from_reader(first_line.as_bytes());
    let headers: Vec<String> = rdr.headers().ok()?.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
    if headers.is_empty() { return None; }

    // Bitwarden CSV distinctive headers
    if headers.iter().any(|h| ["login_username", "login_uri", "login_password", "login_totp", "reprompt"].contains(&h.as_str())) {
        return Some(ImportFormat::BitwardenCsv);
    }

    // LastPass CSV: url,username,password,totp,extra,name,grouping,fav
    if headers.contains(&"grouping".to_string()) || (headers.contains(&"extra".to_string()) && headers.contains(&"fav".to_string())) {
        return Some(ImportFormat::LastpassCsv);
    }

    // Proton Pass CSV: type,name,url,email,username,password,note,totp,createTime,modifyTime,vault
    if (headers.contains(&"createtime".to_string()) && headers.contains(&"modifytime".to_string()))
        || (headers.contains(&"vault".to_string()) && headers.contains(&"email".to_string()))
        || (headers.contains(&"type".to_string()) && headers.contains(&"email".to_string()) && headers.contains(&"username".to_string()))
    {
        return Some(ImportFormat::ProtonPassCsv);
    }

    // KeePass CSV distinctive headers
    if headers.iter().any(|h| ["group", "account", "login name", "web site", "comments", "icon", "last modified"].contains(&h.as_str())) {
        return Some(ImportFormat::KeepassCsv);
    }

    // Apple Passwords vs generic CSV (Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes):
    // Apple Passwords CSV header: Title,URL,Username,Password,Notes,OTPAuth
    if headers.contains(&"otpauth".to_string()) {
        if headers.iter().any(|h| ["favorite", "archived", "tags"].contains(&h.as_str())) {
            return Some(ImportFormat::GenericCsv);
        }
        if headers.contains(&"title".to_string()) || headers.contains(&"url".to_string()) {
            return Some(ImportFormat::ApplePasswordsCsv);
        }
    }

    // Generic CSV (without otpauth) or containing favorite/archived/tags/notesplain
    if headers.iter().any(|h| ["favorite", "archived", "tags", "notesplain"].contains(&h.as_str())) {
        return Some(ImportFormat::GenericCsv);
    }

    // Chrome CSV: name,url,username,password
    if headers.contains(&"url".to_string())
        && headers.contains(&"username".to_string())
        && headers.contains(&"password".to_string())
        && (headers.contains(&"name".to_string())
            || headers.contains(&"origin".to_string())
            || headers.contains(&"origin_url".to_string()))
    {
        return Some(ImportFormat::ChromeCsv);
    }

    None
}

/// Auto-detect the import format from raw file bytes.
pub fn detect_format_bytes(data: &[u8]) -> Option<ImportFormat> {
    if data.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        if let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(data)) {
            if z.by_name("export.data").is_ok() {
                return Some(ImportFormat::Pux);
            }
        }
    }
    let s = std::str::from_utf8(data).ok()?;
    detect_format(s)
}
