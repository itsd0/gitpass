use gitpass_cli::clip::{copy_then_clear, set_with_markers, Clipboard, MemClipboard};
use gitpass_pasteboard::PasteboardMarkers;
use std::time::Duration;

#[test]
fn clipboard_clear_after_timer() {
    let mut cb = MemClipboard::default();
    let mut slept = None;
    let cleared = copy_then_clear(&mut cb, "s3cret", None, Duration::from_secs(30), |d| slept = Some(d)).unwrap();
    assert!(cleared); assert_eq!(slept, Some(Duration::from_secs(30)));
    assert_eq!(cb.get().unwrap(), ""); assert_eq!(cb.cleared, 1);
}

#[test]
fn secrets_get_pasteboard_markers_plain_fields_do_not() {
    let mut cb = MemClipboard::default();
    set_with_markers(&mut cb, "s3cret", Some(PasteboardMarkers::SECRET)).unwrap();
    assert_eq!(cb.markers, Some(PasteboardMarkers::SECRET));
    set_with_markers(&mut cb, "someone", None).unwrap();
    assert_eq!(cb.markers, None);
}

#[test]
fn copy_then_clear_marks_the_value() {
    let mut cb = Hijacked(MemClipboard::default());
    copy_then_clear(&mut cb, "s3cret", Some(PasteboardMarkers::SECRET), Duration::ZERO, |_| {}).unwrap();
    assert_eq!(cb.0.markers, Some(PasteboardMarkers::SECRET));
}

struct Hijacked(MemClipboard);
impl Clipboard for Hijacked {
    fn set(&mut self, t: &str) -> anyhow::Result<()> { self.0.set(t) }
    fn get(&mut self) -> anyhow::Result<String> { Ok("other".into()) } // another app copied during the sleep
    fn clear(&mut self) -> anyhow::Result<()> { self.0.clear() }
    fn set_marked(&mut self, t: &str, m: PasteboardMarkers) -> anyhow::Result<()> { self.0.set_marked(t, m) }
}

#[test]
fn clipboard_not_clobbered_if_user_copied_else() {
    let mut cb = Hijacked(MemClipboard::default());
    assert!(!copy_then_clear(&mut cb, "s3cret", None, Duration::ZERO, |_| {}).unwrap());
    assert_eq!(cb.0.cleared, 0);
}
