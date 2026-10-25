//! Clipboard copy with timed auto-clear. `copy` blocks for `clear_secs` (on X11/Wayland the owning
//! process must stay alive to serve the selection anyway), then clears and exits.
use anyhow::Result;
use std::time::Duration;
use zeroize::Zeroizing;

pub trait Clipboard {
    fn set(&mut self, text: &str) -> Result<()>;
    fn get(&mut self) -> Result<String>;
    fn clear(&mut self) -> Result<()>;
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
}

/// Open the system clipboard (Err if headless or built without the `clipboard` feature).
pub fn system() -> Result<Box<dyn Clipboard>> {
    #[cfg(feature = "clipboard")] { Ok(Box::new(SystemClipboard::new()?)) }
    #[cfg(not(feature = "clipboard"))] { anyhow::bail!("built without clipboard support") }
}

/// Copy `secret`, sleep via `sleep(dur)`, then clear only if the clipboard still holds our value
/// (don't clobber something the user copied since). Returns whether we cleared.
pub fn copy_then_clear(cb: &mut dyn Clipboard, secret: &str, dur: Duration, sleep: impl FnOnce(Duration)) -> Result<bool> {
    cb.set(secret)?;
    sleep(dur);
    let cur = Zeroizing::new(cb.get().unwrap_or_default());
    if *cur == secret || cur.is_empty() { cb.clear()?; Ok(true) } else { Ok(false) }
}

/// In-memory clipboard for tests.
#[derive(Default)]
pub struct MemClipboard { pub content: Zeroizing<String>, pub cleared: usize }
impl Clipboard for MemClipboard {
    fn set(&mut self, t: &str) -> Result<()> { self.content = Zeroizing::new(t.into()); Ok(()) }
    fn get(&mut self) -> Result<String> { Ok(self.content.to_string()) }
    fn clear(&mut self) -> Result<()> { self.content = Zeroizing::new(String::new()); self.cleared += 1; Ok(()) }
}
