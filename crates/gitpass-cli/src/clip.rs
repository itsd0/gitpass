//! Clipboard copy with timed auto-clear. `copy` blocks for `clear_secs` (on X11/Wayland the owning
//! process must stay alive to serve the selection anyway), then clears and exits.
use anyhow::Result;
use gitpass_pasteboard::PasteboardMarkers;
use std::time::Duration;
use zeroize::Zeroizing;

pub trait Clipboard {
    fn set(&mut self, text: &str) -> Result<()>;
    fn get(&mut self) -> Result<String>;
    fn clear(&mut self) -> Result<()>;
    /// Copy a secret: macOS declares the nspasteboard.org markers; the default writes plain text.
    fn set_marked(&mut self, text: &str, markers: PasteboardMarkers) -> Result<()> { let _ = markers; self.set(text) }
}

#[cfg(feature = "clipboard")]
pub struct SystemClipboard(arboard::Clipboard);

#[cfg(feature = "clipboard")]
impl SystemClipboard {
    pub fn new() -> Result<Self> { Ok(Self(arboard::Clipboard::new()?)) }
}

#[cfg(feature = "clipboard")]
impl Clipboard for SystemClipboard {
    fn set(&mut self, t: &str) -> Result<()> { Ok(self.0.set_text(t.to_owned())?) }
    fn get(&mut self) -> Result<String> { Ok(self.0.get_text()?) }
    fn clear(&mut self) -> Result<()> { self.0.set_text(String::new())?; let _ = self.0.clear(); Ok(()) }
    #[cfg(target_os = "macos")]
    fn set_marked(&mut self, t: &str, markers: PasteboardMarkers) -> Result<()> { gitpass_pasteboard::write(t, markers) }
}

/// Open the system clipboard (Err if headless or built without the `clipboard` feature).
pub fn system() -> Result<Box<dyn Clipboard>> {
    #[cfg(feature = "clipboard")] { Ok(Box::new(SystemClipboard::new()?)) }
    #[cfg(not(feature = "clipboard"))] { anyhow::bail!("built without clipboard support") }
}

/// Write `text` to the clipboard, with markers when it is a secret.
pub fn set_with_markers(cb: &mut dyn Clipboard, text: &str, markers: Option<PasteboardMarkers>) -> Result<()> {
    match markers { Some(m) => cb.set_marked(text, m), None => cb.set(text) }
}

/// Copy `secret`, sleep via `sleep(dur)`, then clear only if the clipboard still holds our value
/// (don't clobber something the user copied since). Returns whether we cleared.
pub fn copy_then_clear(cb: &mut dyn Clipboard, secret: &str, markers: Option<PasteboardMarkers>, dur: Duration, sleep: impl FnOnce(Duration)) -> Result<bool> {
    set_with_markers(cb, secret, markers)?;
    sleep(dur);
    let cur = Zeroizing::new(cb.get().unwrap_or_default());
    if *cur == secret || cur.is_empty() { cb.clear()?; Ok(true) } else { Ok(false) }
}

/// In-memory clipboard for tests.
#[derive(Default)]
pub struct MemClipboard { pub content: Zeroizing<String>, pub cleared: usize, pub markers: Option<PasteboardMarkers> }
impl Clipboard for MemClipboard {
    fn set(&mut self, t: &str) -> Result<()> { self.content = Zeroizing::new(t.into()); self.markers = None; Ok(()) }
    fn get(&mut self) -> Result<String> { Ok(self.content.to_string()) }
    fn clear(&mut self) -> Result<()> { self.content = Zeroizing::new(String::new()); self.markers = None; self.cleared += 1; Ok(()) }
    fn set_marked(&mut self, t: &str, markers: PasteboardMarkers) -> Result<()> { self.set(t)?; self.markers = Some(markers); Ok(()) }
}
