use anyhow::{anyhow, Result};
use gitpass_core::audit::{self, PwnedRangeClient};
use gitpass_pasteboard::PasteboardMarkers;
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

// ------------------------------------------------------------------ clipboard
pub trait Clipboard: Send + Sync {
    fn set_text(&self, s: &str) -> Result<()>;
    fn get_text(&self) -> Option<String>;
    /// Copy a secret: macOS implementations declare the nspasteboard.org markers so clipboard
    /// managers do not record or sync it. The default (and non-macOS) writes plain text.
    fn set_marked(&self, s: &str, _markers: PasteboardMarkers) -> Result<()> { self.set_text(s) }
    fn get_image(&self) -> Option<ClipboardImage> { None }
    fn set_image(&self, _img: ClipboardImage) -> Result<()> { Ok(()) }
}

/// In-memory clipboard (tests / headless).
#[derive(Default)]
pub struct MemClipboard {
    text: Mutex<Option<String>>,
    markers: Mutex<Option<PasteboardMarkers>>,
    image: Mutex<Option<ClipboardImage>>,
}
impl MemClipboard {
    pub fn markers(&self) -> Option<PasteboardMarkers> { *self.markers.lock().unwrap() }
    pub fn set_test_image(&self, img: ClipboardImage) { *self.image.lock().unwrap() = Some(img); }
}
impl Clipboard for MemClipboard {
    fn set_text(&self, s: &str) -> Result<()> { *self.text.lock().unwrap() = Some(s.to_string()); *self.markers.lock().unwrap() = None; *self.image.lock().unwrap() = None; Ok(()) }
    fn get_text(&self) -> Option<String> { self.text.lock().unwrap().clone() }
    fn set_marked(&self, s: &str, markers: PasteboardMarkers) -> Result<()> {
        self.set_text(s)?;
        *self.markers.lock().unwrap() = Some(markers);
        *self.image.lock().unwrap() = None;
        Ok(())
    }
    fn get_image(&self) -> Option<ClipboardImage> { self.image.lock().unwrap().clone() }
    fn set_image(&self, img: ClipboardImage) -> Result<()> {
        *self.image.lock().unwrap() = Some(img);
        *self.text.lock().unwrap() = None;
        *self.markers.lock().unwrap() = None;
        Ok(())
    }
}

/// Real HIBP range client (k-anonymity; only 5 hex chars of SHA-1 leave the process).
pub struct UreqHibp;
impl PwnedRangeClient for UreqHibp {
    fn range(&self, prefix: &str) -> Result<String> {
        let r = ureq::get(&format!("{}{prefix}", audit::HIBP_RANGE_URL)).set("Add-Padding", "true")
            .set("User-Agent", concat!("gitpass-desktop/", env!("CARGO_PKG_VERSION"))).call()
            .map_err(|e| anyhow!("HIBP request failed: {e}"))?;
        Ok(r.into_string()?)
    }
}

