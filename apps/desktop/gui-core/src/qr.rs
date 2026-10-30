//! QR code scanning and decoding for TOTP `otpauth://totp/...` URIs.
//! Supports decoding from screen capture, clipboard image, and image bytes.

use anyhow::{anyhow, bail, Result};
use gitpass_core::totp;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScannedTotp {
    pub uri: String,
    pub secret: String,
    pub issuer: Option<String>,
    pub account: Option<String>,
    pub digits: u32,
    pub period: u64,
    pub algorithm: String,
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut o = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                o.push(v);
                i += 3;
                continue;
            }
        }
        o.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&o).into_owned()
}

/// Parse an `otpauth://totp/...` URI into a validated `ScannedTotp`.
/// Strictly accepts only `otpauth://totp/...` (or `otpauth://totp?...`).
pub fn parse_totp_uri(raw: &str) -> Result<ScannedTotp> {
    let s = raw.trim();
    if s.len() < 10 || !s[..10].eq_ignore_ascii_case("otpauth://") {
        bail!("This QR code isn't a one-time code");
    }
    let rest = &s[10..];
    let lower = rest.to_ascii_lowercase();
    if !(lower.starts_with("totp/") || lower.starts_with("totp?")) {
        bail!("This QR code isn't a one-time code");
    }

    // Validate using gitpass-core's totp parser (checks secret, digits, period, algorithm)
    let spec = totp::parse(s).map_err(|_| anyhow!("This QR code isn't a one-time code"))?;

    // Extract path/label and query parameters
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, q),
        None => (rest, ""),
    };

    let raw_label = if let Some(stripped) = path.strip_prefix("totp/") {
        stripped
    } else if let Some(stripped) = path.strip_prefix("totp") {
        stripped
    } else {
        path
    };
    let label = pct_decode(raw_label.trim_matches('/'));

    let mut issuer_from_label = None;
    let mut account_from_label = None;
    if !label.is_empty() {
        if let Some((l_iss, l_acc)) = label.split_once(':') {
            let iss = l_iss.trim();
            let acc = l_acc.trim();
            if !iss.is_empty() {
                issuer_from_label = Some(iss.to_string());
            }
            if !acc.is_empty() {
                account_from_label = Some(acc.to_string());
            }
        } else {
            account_from_label = Some(label.trim().to_string());
        }
    }

    let mut issuer_from_query = None;
    let mut secret = String::new();
    for kv in query.split('&') {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        let v = pct_decode(v);
        if k.eq_ignore_ascii_case("issuer") {
            let iss = v.trim();
            if !iss.is_empty() {
                issuer_from_query = Some(iss.to_string());
            }
        } else if k.eq_ignore_ascii_case("secret") {
            secret = v.trim().to_string();
        }
    }

    let issuer = issuer_from_query.or(issuer_from_label);
    let account = account_from_label;
    let algorithm = match spec.algo {
        totp::Algo::Sha1 => "SHA1",
        totp::Algo::Sha256 => "SHA256",
        totp::Algo::Sha512 => "SHA512",
    }
    .to_string();

    Ok(ScannedTotp {
        uri: s.to_string(),
        secret,
        issuer,
        account,
        digits: spec.digits,
        period: spec.period,
        algorithm,
    })
}

/// Decode a QR code from a DynamicImage.
pub fn decode_qr_image(img: &image::DynamicImage) -> Result<ScannedTotp> {
    let luma = img.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(luma);
    let grids = prepared.detect_grids();
    if grids.is_empty() {
        bail!("No QR code found");
    }
    for grid in &grids {
        let mut content = Vec::new();
        if grid.decode_to(&mut content).is_ok() {
            let s = String::from_utf8(content).map_err(|_| anyhow!("This QR code isn't a one-time code"))?;
            return parse_totp_uri(&s);
        }
    }
    bail!("No QR code found");
}

/// Decode a QR code from encoded image bytes (PNG, JPEG, etc.).
pub fn decode_qr_bytes(bytes: &[u8]) -> Result<ScannedTotp> {
    let img = image::load_from_memory(bytes).map_err(|_| anyhow!("No QR code found"))?;
    decode_qr_image(&img)
}

/// Decode a QR code from a base64 string or data URL.
pub fn decode_qr_base64(data: &str) -> Result<ScannedTotp> {
    let raw = if let Some((_, b64)) = data.split_once(";base64,") {
        b64
    } else {
        data
    };
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(raw.trim()))
        .map_err(|_| anyhow!("No QR code found"))?;
    decode_qr_bytes(&bytes)
}

/// Decode a QR code from an image on the clipboard without writing to disk.
pub fn scan_totp_clipboard(clipboard: &dyn crate::api::Clipboard) -> Result<ScannedTotp> {
    if let Some(img_data) = clipboard.get_image() {
        if let Some(rgba) = image::RgbaImage::from_raw(img_data.width as u32, img_data.height as u32, img_data.bytes) {
            let dyn_img = image::DynamicImage::ImageRgba8(rgba);
            return decode_qr_image(&dyn_img);
        }
    }
    bail!("No QR code found");
}

/// Capture screen selection interactively via macOS `/usr/sbin/screencapture -i -x`
/// into a private temp dir (mode 0700), decode it, and delete the image file right away.
pub fn scan_totp_screen() -> Result<ScannedTotp> {
    scan_totp_screen_with(None)
}

pub fn scan_totp_screen_with(custom_cmd: Option<&Path>) -> Result<ScannedTotp> {
    let temp_dir = tempfile::Builder::new()
        .prefix("gitpass-qr-")
        .tempdir()
        .map_err(|e| anyhow!("failed to create secure temp dir: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(temp_dir.path(), std::fs::Permissions::from_mode(0o700));
    }

    let capture_file = temp_dir.path().join("capture.png");

    let status = if let Some(cmd) = custom_cmd {
        std::process::Command::new(cmd)
            .arg(&capture_file)
            .status()
    } else {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("/usr/sbin/screencapture")
                .args(["-i", "-x"])
                .arg(&capture_file)
                .status()
        }
        #[cfg(not(target_os = "macos"))]
        {
            bail!("Screen capture is only supported on macOS");
        }
    };

    let res = match status {
        Ok(s) if s.success() && capture_file.exists() => {
            let bytes = std::fs::read(&capture_file);
            let _ = std::fs::remove_file(&capture_file);
            match bytes {
                Ok(b) if !b.is_empty() => decode_qr_bytes(&b),
                _ => bail!("Screen capture was cancelled"),
            }
        }
        _ => {
            let _ = std::fs::remove_file(&capture_file);
            bail!("Screen capture was cancelled");
        }
    };

    if capture_file.exists() {
        let _ = std::fs::remove_file(&capture_file);
    }
    res
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn fixture_bytes(name: &str) -> Vec<u8> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("qr")
            .join(name);
        fs::read(&p).unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()))
    }

    #[test]
    fn test_decode_valid_totp_fixture() {
        let bytes = fixture_bytes("valid_totp.png");
        let res = decode_qr_bytes(&bytes).expect("should decode valid totp QR");
        assert_eq!(res.secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(res.issuer.as_deref(), Some("Example"));
        assert_eq!(res.account.as_deref(), Some("alice@example.com"));
        assert_eq!(res.digits, 6);
        assert_eq!(res.period, 30);
        assert_eq!(res.algorithm, "SHA1");
        assert!(res.uri.starts_with("otpauth://totp/"));
    }

    #[test]
    fn test_decode_not_totp_fixture() {
        let bytes = fixture_bytes("not_totp.png");
        let err = decode_qr_bytes(&bytes).unwrap_err();
        assert_eq!(err.to_string(), "This QR code isn't a one-time code");
    }

    #[test]
    fn test_decode_no_qr_fixture() {
        let bytes = fixture_bytes("no_qr.png");
        let err = decode_qr_bytes(&bytes).unwrap_err();
        assert_eq!(err.to_string(), "No QR code found");
    }

    #[test]
    fn test_scan_totp_clipboard_image() {
        let bytes = fixture_bytes("valid_totp.png");
        let img = image::load_from_memory(&bytes).expect("load image");
        let rgba = img.to_rgba8();
        let clip_img = crate::api::ClipboardImage {
            width: rgba.width() as usize,
            height: rgba.height() as usize,
            bytes: rgba.into_raw(),
        };
        let clip = crate::api::MemClipboard::default();
        clip.set_test_image(clip_img);
        let res = scan_totp_clipboard(&clip).expect("clipboard scan should succeed");
        assert_eq!(res.secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(res.issuer.as_deref(), Some("Example"));
    }

    #[cfg(unix)]
    #[test]
    fn test_screen_capture_removal_on_success_error_and_cancel() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = tempfile::tempdir().unwrap();

        // 1. Success script: copies valid_totp.png to output path ($1) and exits 0
        let valid_src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("qr")
            .join("valid_totp.png");
        let success_script = temp_dir.path().join("success_capture.sh");
        let mut f = fs::File::create(&success_script).unwrap();
        writeln!(f, "#!/bin/sh\ncp '{}' \"$1\"\nexit 0", valid_src.display()).unwrap();
        drop(f);
        fs::set_permissions(&success_script, fs::Permissions::from_mode(0o755)).unwrap();

        let res = scan_totp_screen_with(Some(&success_script));
        assert!(res.is_ok(), "scan should succeed: {:?}", res.err());
        let totp = res.unwrap();
        assert_eq!(totp.secret, "JBSWY3DPEHPK3PXP");

        // 2. Non-totp QR script: copies not_totp.png and exits 0
        let not_totp_src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("qr")
            .join("not_totp.png");
        let not_totp_script = temp_dir.path().join("not_totp_capture.sh");
        let mut f = fs::File::create(&not_totp_script).unwrap();
        writeln!(f, "#!/bin/sh\ncp '{}' \"$1\"\nexit 0", not_totp_src.display()).unwrap();
        drop(f);
        fs::set_permissions(&not_totp_script, fs::Permissions::from_mode(0o755)).unwrap();

        let err = scan_totp_screen_with(Some(&not_totp_script)).unwrap_err();
        assert_eq!(err.to_string(), "This QR code isn't a one-time code");

        // 3. No-QR script: copies no_qr.png and exits 0
        let no_qr_src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("qr")
            .join("no_qr.png");
        let no_qr_script = temp_dir.path().join("no_qr_capture.sh");
        let mut f = fs::File::create(&no_qr_script).unwrap();
        writeln!(f, "#!/bin/sh\ncp '{}' \"$1\"\nexit 0", no_qr_src.display()).unwrap();
        drop(f);
        fs::set_permissions(&no_qr_script, fs::Permissions::from_mode(0o755)).unwrap();

        let err = scan_totp_screen_with(Some(&no_qr_script)).unwrap_err();
        assert_eq!(err.to_string(), "No QR code found");

        // 4. Cancel script: exits with code 1 (user pressed Escape) without writing file
        let cancel_script = temp_dir.path().join("cancel_capture.sh");
        let mut f = fs::File::create(&cancel_script).unwrap();
        writeln!(f, "#!/bin/sh
exit 1").unwrap();
        drop(f);
        fs::set_permissions(&cancel_script, fs::Permissions::from_mode(0o755)).unwrap();

        let err = scan_totp_screen_with(Some(&cancel_script)).unwrap_err();
        assert_eq!(err.to_string(), "Screen capture was cancelled");
    }

    #[test]
    fn test_parse_totp_uri_variants() {
        let a = parse_totp_uri("otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(a.issuer.as_deref(), Some("GitHub"));
        assert_eq!(a.account.as_deref(), Some("octocat"));
        assert_eq!(a.secret, "JBSWY3DPEHPK3PXP");

        let b = parse_totp_uri("otpauth://totp/octocat?secret=JBSWY3DPEHPK3PXP&issuer=Google&digits=8&period=60&algorithm=SHA256").unwrap();
        assert_eq!(b.issuer.as_deref(), Some("Google"));
        assert_eq!(b.account.as_deref(), Some("octocat"));
        assert_eq!(b.digits, 8);
        assert_eq!(b.period, 60);
        assert_eq!(b.algorithm, "SHA256");

        assert_eq!(
            parse_totp_uri("https://example.com").unwrap_err().to_string(),
            "This QR code isn't a one-time code"
        );
        assert_eq!(
            parse_totp_uri("otpauth://hotp/acct?secret=JBSWY3DPEHPK3PXP&counter=1").unwrap_err().to_string(),
            "This QR code isn't a one-time code"
        );
    }
}
