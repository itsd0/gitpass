use gitpass_cli::clip::{copy_then_clear, Clipboard, MemClipboard};
use std::time::Duration;

#[test]
fn clipboard_clear_after_timer() {
    let mut cb = MemClipboard::default();
    let mut slept = None;
    let cleared = copy_then_clear(&mut cb, "s3cret", Duration::from_secs(30), |d| slept = Some(d)).unwrap();
    assert!(cleared); assert_eq!(slept, Some(Duration::from_secs(30)));
    assert_eq!(cb.get().unwrap(), ""); assert_eq!(cb.cleared, 1);
}

struct Hijacked(MemClipboard);
impl Clipboard for Hijacked {
    fn set(&mut self, t: &str) -> anyhow::Result<()> { self.0.set(t) }
    fn get(&mut self) -> anyhow::Result<String> { Ok("other".into()) } // another app copied during the sleep
    fn clear(&mut self) -> anyhow::Result<()> { self.0.clear() }
}

#[test]
fn clipboard_not_clobbered_if_user_copied_else() {
    let mut cb = Hijacked(MemClipboard::default());
    assert!(!copy_then_clear(&mut cb, "s3cret", Duration::ZERO, |_| {}).unwrap());
    assert_eq!(cb.0.cleared, 0);
}
