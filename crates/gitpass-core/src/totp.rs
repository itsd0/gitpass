//! RFC 6238 TOTP (HMAC-SHA1/256/512) with otpauth:// URI or raw base32 secret.
use anyhow::{anyhow, bail, Result};
use hmac::{Hmac, Mac};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo { Sha1, Sha256, Sha512 }

#[derive(Debug, Clone)]
pub struct TotpSpec { pub secret: Zeroizing<Vec<u8>>, pub digits: u32, pub period: u64, pub algo: Algo }

/// RFC 4648 base32 decode (case-insensitive, ignores spaces/dashes and '=' padding).
pub fn base32_decode(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u64, 0u32);
    for c in s.chars().filter(|c| !matches!(c, ' ' | '-' | '=')) {
        let v = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u64 - 'A' as u64,
            c @ '2'..='7' => c as u64 - '2' as u64 + 26,
            _ => bail!("invalid base32 character {c:?}"),
        };
        buf = (buf << 5) | v; bits += 5;
        if bits >= 8 { bits -= 8; out.push((buf >> bits) as u8); buf &= (1 << bits) - 1; }
    }
    if out.is_empty() { bail!("empty TOTP secret"); }
    Ok(out)
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes(); let mut o = Vec::new(); let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) { o.push(v); i += 3; continue; }
        }
        o.push(b[i]); i += 1;
    }
    String::from_utf8_lossy(&o).into_owned()
}

/// Parse Item.totp: `otpauth://totp/...?secret=...&digits=&period=&algorithm=` or a raw base32 secret.
pub fn parse(s: &str) -> Result<TotpSpec> {
    let s = s.trim();
    let mut spec = TotpSpec { secret: Zeroizing::new(vec![]), digits: 6, period: 30, algo: Algo::Sha1 };
    if s.len() >= 10 && s[..10].eq_ignore_ascii_case("otpauth://") {
        let rest = &s[10..];
        // Apple Passwords exports `otpauth://totp?secret=...` with no label, so accept "totp/" and "totp?".
        let lower = rest.to_ascii_lowercase();
        if !(lower.starts_with("totp/") || lower.starts_with("totp?")) { bail!("only otpauth://totp URIs are supported"); }
        let q = rest.split_once('?').map(|x| x.1).ok_or_else(|| anyhow!("otpauth URI has no parameters"))?;
        let mut secret = None;
        for kv in q.split('&') {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            let v = pct_decode(v);
            match k.to_ascii_lowercase().as_str() {
                "secret" => secret = Some(base32_decode(&v)?),
                "digits" => spec.digits = v.parse().map_err(|_| anyhow!("bad digits"))?,
                "period" => spec.period = v.parse().map_err(|_| anyhow!("bad period"))?,
                "algorithm" => spec.algo = match v.to_ascii_uppercase().as_str() {
                    "SHA1" => Algo::Sha1, "SHA256" => Algo::Sha256, "SHA512" => Algo::Sha512,
                    o => bail!("unsupported algorithm {o}") },
                _ => {}
            }
        }
        spec.secret = Zeroizing::new(secret.ok_or_else(|| anyhow!("otpauth URI missing secret"))?);
    } else {
        spec.secret = Zeroizing::new(base32_decode(s)?);
    }
    if !(6..=10).contains(&spec.digits) { bail!("digits must be 6..=10"); }
    if spec.period == 0 { bail!("period must be > 0"); }
    Ok(spec)
}

fn hmac_of<M: Mac + hmac::digest::KeyInit>(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut m = <M as hmac::digest::KeyInit>::new_from_slice(key).expect("hmac accepts any key length");
    m.update(msg); m.finalize().into_bytes().to_vec()
}

/// RFC 4226 HOTP.
pub fn hotp(secret: &[u8], counter: u64, digits: u32, algo: Algo) -> String {
    let msg = counter.to_be_bytes();
    let h = match algo {
        Algo::Sha1 => hmac_of::<Hmac<sha1::Sha1>>(secret, &msg),
        Algo::Sha256 => hmac_of::<Hmac<sha2::Sha256>>(secret, &msg),
        Algo::Sha512 => hmac_of::<Hmac<sha2::Sha512>>(secret, &msg),
    };
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]) as u64;
    format!("{:0width$}", bin % 10u64.pow(digits), width = digits as usize)
}

/// Code at unix time `t` and seconds remaining in its window.
pub fn code_at(spec: &TotpSpec, t: u64) -> (String, u64) {
    (hotp(&spec.secret, t / spec.period, spec.digits, spec.algo), spec.period - t % spec.period)
}

pub fn code_now(spec: &TotpSpec) -> (String, u64) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    code_at(spec, t)
}

#[cfg(test)]
mod label_less_uri_tests {
    use super::*;
    #[test]
    fn parses_otpauth_totp_without_label() {
        let a = parse("otpauth://totp?secret=JBSWY3DPEHPK3PXP&digits=6").unwrap();
        let b = parse("otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(&a.secret[..], &b.secret[..]);
        assert_eq!(a.digits, 6);
        assert!(parse("otpauth://hotp?secret=JBSWY3DPEHPK3PXP").is_err());
    }
}
