//! Emergency Kit as a standalone PDF file (issue #384).
//!
//! The Tauri 2 webview on macOS does not implement `window.print()`, so the
//! kit's "Print / Save as PDF" button silently did nothing there. This module
//! generates the PDF natively in Rust (no new dependency: a minimal, hand
//! built PDF 1.4 document with the built-in Helvetica fonts) and the frontend
//! writes it through the native save dialog, so saving no longer depends on
//! the webview's print support at all.
//!
//! Deliberate limits, stated so a reviewer can judge them:
//! - Text only: no QR code image. The Secret Key itself is printed as text,
//!   which is what recovery needs; the QR on the printable sheet is a
//!   convenience for scanning, not a recovery input.
//! - Latin text only: PDF literal strings here use the built-in fonts, so any
//!   character outside printable ASCII is replaced with `?` (a vault path with
//!   non-ASCII characters will show `?` in its place in the PDF).

use super::super::dto::EmergencyKitData;
use super::App;
use anyhow::{bail, Context, Result};

const PAGE_W: f32 = 595.0; // A4
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 54.0;
const LINE_GAP: f32 = 6.0;

struct Line { text: String, size: f32, bold: bool }

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if (' '..='~').contains(&c) { c } else { '?' }).collect()
}

fn escape_pdf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => { out.push('\\'); out.push(c); }
            _ => out.push(c),
        }
    }
    out
}

/// Word-wrap, chunking any single word longer than the line (Secret Keys and
/// vault paths are long unbroken strings and must not be dropped).
fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let mut cur = String::new();
        for word in raw.split(' ') {
            if word.len() > max_chars {
                if !cur.is_empty() { lines.push(std::mem::take(&mut cur)); }
                let mut rest = word;
                while rest.len() > max_chars {
                    lines.push(rest[..max_chars].to_string());
                    rest = &rest[max_chars..];
                }
                cur = rest.to_string();
                continue;
            }
            if cur.is_empty() { cur = word.to_string(); }
            else if cur.len() + 1 + word.len() <= max_chars { cur.push(' '); cur.push_str(word); }
            else { lines.push(std::mem::take(&mut cur)); cur = word.to_string(); }
        }
        lines.push(cur);
    }
    lines
}

fn push_wrapped(lines: &mut Vec<Line>, text: &str, size: f32, bold: bool) {
    // Helvetica averages ~0.5 * size per character; err on the narrow side so
    // long values never run off the right edge.
    let max_chars = ((PAGE_W - 2.0 * MARGIN) / (size * 0.5)) as usize;
    for l in wrap(text, max_chars.max(10)) {
        lines.push(Line { text: l, size, bold });
    }
}

fn kit_lines(d: &EmergencyKitData) -> Vec<Line> {
    let mut lines = Vec::new();
    push_wrapped(&mut lines, "gitpass Emergency Kit", 20.0, true);
    push_wrapped(&mut lines, "Keep this sheet somewhere safe and offline. With it you can open your vault on a new device.", 11.0, false);
    lines.push(Line { text: String::new(), size: 11.0, bold: false });
    push_wrapped(&mut lines, &format!("Created: {}", d.created_at), 11.0, false);
    if let Some(dev) = &d.device_name {
        push_wrapped(&mut lines, &format!("Device: {dev}"), 11.0, false);
    }
    push_wrapped(&mut lines, &format!("Account ID: {}", d.account_id), 11.0, false);
    push_wrapped(&mut lines, &format!("Vault folder: {}", d.vault_path), 11.0, false);
    push_wrapped(&mut lines, &format!("Git remote: {}", d.remote.as_deref().unwrap_or("No remote configured yet")), 11.0, false);
    lines.push(Line { text: String::new(), size: 11.0, bold: false });
    push_wrapped(&mut lines, "Secret Key", 14.0, true);
    match &d.secret_key {
        Some(k) => push_wrapped(&mut lines, k, 13.0, true),
        None => push_wrapped(&mut lines, "(not shown)", 11.0, false),
    }
    lines.push(Line { text: String::new(), size: 11.0, bold: false });
    push_wrapped(&mut lines, "Master password: write it by hand: ______________________", 11.0, false);
    push_wrapped(&mut lines, "gitpass never prints or stores your master password.", 11.0, false);
    lines.push(Line { text: String::new(), size: 11.0, bold: false });
    push_wrapped(&mut lines, "How to open your vault on a new device", 14.0, true);
    push_wrapped(&mut lines, "1. Install gitpass and choose \"Vault folder & Secret Key\" on the unlock screen.", 11.0, false);
    push_wrapped(&mut lines, "2. Clone your vault from the Git remote above (or copy the vault folder).", 11.0, false);
    push_wrapped(&mut lines, "3. Enter the Secret Key from this kit and your master password.", 11.0, false);
    push_wrapped(&mut lines, "4. Keep this kit offline. If it is lost or exposed, rotate your Secret Key in Settings.", 11.0, false);
    lines.push(Line { text: String::new(), size: 11.0, bold: false });
    push_wrapped(&mut lines, "gitpass - end-to-end encrypted passwords in your own Git repository. Anyone with this kit and your master password can open your vault.", 9.0, false);
    lines
}

/// Render the kit as PDF bytes. Panics never: all text is sanitized first.
pub fn render_emergency_kit_pdf(d: &EmergencyKitData) -> Vec<u8> {
    let lines = kit_lines(d);
    // Paginate: lines per page from the running y position.
    let mut pages: Vec<Vec<&Line>> = Vec::new();
    let mut cur: Vec<&Line> = Vec::new();
    let mut y = PAGE_H - MARGIN;
    for l in &lines {
        let h = l.size + LINE_GAP;
        if y - h < MARGIN && !cur.is_empty() {
            pages.push(std::mem::take(&mut cur));
            y = PAGE_H - MARGIN;
        }
        cur.push(l);
        y -= h;
    }
    if !cur.is_empty() { pages.push(cur); }
    if pages.is_empty() { pages.push(Vec::new()); }

    // Object numbering: 1 catalog, 2 pages, 3 Helvetica, 4 Helvetica-Bold,
    // then per page: page object, contents object.
    let n_pages = pages.len();
    let mut objects: Vec<(usize, Vec<u8>)> = Vec::new();
    objects.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
    let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", 5 + 2 * i)).collect();
    objects.push((2, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), n_pages).into_bytes()));
    objects.push((3, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec()));
    objects.push((4, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_vec()));
    for (i, page) in pages.iter().enumerate() {
        let page_obj = 5 + 2 * i;
        let contents_obj = page_obj + 1;
        objects.push((page_obj, format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {contents_obj} 0 R >>"
        ).into_bytes()));
        let mut stream = String::new();
        let mut y = PAGE_H - MARGIN;
        for l in page {
            y -= l.size + LINE_GAP;
            let font = if l.bold { "F2" } else { "F1" };
            stream.push_str(&format!("BT /{font} {} Tf 1 0 0 1 {} {} Tm ({}) Tj ET\n",
                l.size, MARGIN, y, escape_pdf(&sanitize(&l.text))));
        }
        objects.push((contents_obj, format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream).into_bytes()));
    }

    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = vec![0; objects.len() + 1];
    for (num, body) in &objects {
        offsets[*num] = out.len();
        out.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    let total = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {total}\n0000000000 65535 f \n").as_bytes());
    for off in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {total} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes());
    out
}

impl App {
    /// Write the Emergency Kit PDF for `data` to `dest_path` (".pdf" is
    /// appended when the chosen path has no such suffix, as native save
    /// dialogs do not always add it). The Secret Key must already be
    /// revealed in `data`; a kit PDF without its key would be useless, so
    /// that case is an error rather than a silently incomplete file.
    /// No vault session is needed: the caller already holds the kit data.
    pub fn save_emergency_kit_pdf(&self, data: &EmergencyKitData, dest_path: &str) -> Result<()> {
        if data.secret_key.as_deref().map(str::is_empty).unwrap_or(true) {
            bail!("reveal the Secret Key before saving the kit as PDF");
        }
        let mut path = dest_path.to_string();
        if !path.to_lowercase().ends_with(".pdf") { path.push_str(".pdf"); }
        let bytes = render_emergency_kit_pdf(data);
        std::fs::write(&path, bytes).with_context(|| format!("cannot write {path}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EmergencyKitData {
        EmergencyKitData {
            account_id: "acc-1".into(),
            vault_path: "/tmp/vault (test)".into(),
            remote: Some("git@example.invalid:repo.git".into()),
            device_name: Some("Mac".into()),
            created_at: "2026-10-26".into(),
            secret_key_available: true,
            secret_key: Some("A3-TEST-KEY-123".into()),
        }
    }

    #[test]
    fn pdf_header_and_contents() {
        let pdf = render_emergency_kit_pdf(&sample());
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("A3-TEST-KEY-123"));
        assert!(text.contains("acc-1"));
        assert!(text.contains("/Type /Page"));
        assert!(text.contains("%%EOF"));
    }

    #[test]
    fn pdf_escapes_parens_and_replaces_non_ascii() {
        let mut d = sample();
        d.vault_path = "/tmp/v (a) \\ b é".into();
        let pdf = render_emergency_kit_pdf(&d);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/tmp/v \\(a\\) \\\\ b ?"));
    }

    #[test]
    fn long_secret_key_wraps_instead_of_running_off_page() {
        let mut d = sample();
        d.secret_key = Some("K".repeat(300));
        let pdf = render_emergency_kit_pdf(&d);
        let text = String::from_utf8_lossy(&pdf);
        // No single text-showing line carries the whole 300-char key.
        assert!(!text.contains(&"K".repeat(300)));
        assert!(text.contains(&"K".repeat(50)));
    }

    #[test]
    fn save_writes_file_and_refuses_hidden_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kit");
        let app = App::new(dir.path().join("settings"), std::sync::Arc::new(crate::api::MemClipboard::default()));
        app.save_emergency_kit_pdf(&sample(), path.to_str().unwrap()).unwrap();
        let written = std::fs::read(dir.path().join("kit.pdf")).unwrap();
        assert!(written.starts_with(b"%PDF-1.4\n"));
        let mut hidden = sample();
        hidden.secret_key = None;
        assert!(app.save_emergency_kit_pdf(&hidden, path.to_str().unwrap()).is_err());
    }
}
