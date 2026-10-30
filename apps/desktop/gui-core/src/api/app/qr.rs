use super::App;
use anyhow::{Result};

impl App {
    pub fn scan_totp_screen(&self) -> Result<crate::qr::ScannedTotp> {
        crate::qr::scan_totp_screen()
    }
    pub fn scan_totp_clipboard(&self) -> Result<crate::qr::ScannedTotp> {
        crate::qr::scan_totp_clipboard(&*self.clipboard)
    }
    pub fn scan_totp_image(&self, data_base64: &str) -> Result<crate::qr::ScannedTotp> {
        crate::qr::decode_qr_base64(data_base64)
    }

}
