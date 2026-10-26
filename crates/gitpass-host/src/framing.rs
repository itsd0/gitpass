//! Native messaging framing: 4-byte little-endian (native byte order on all supported targets,
//! which are little-endian) u32 length prefix followed by UTF-8 JSON.
use anyhow::{bail, Result};
use std::io::{ErrorKind, Read, Write};

/// Max inbound message we accept (browsers allow up to 4 GiB; we need far less).
pub const MAX_IN: usize = 1024 * 1024;
/// Chrome rejects host->browser messages larger than 1 MiB.
pub const MAX_OUT: usize = 1024 * 1024;

/// Read one message. Ok(None) on clean EOF before a header (browser closed the port).
pub fn read_message<R: Read>(r: &mut R) -> Result<Option<serde_json::Value>> { read_message_max(r, MAX_IN) }

/// Max frame on the desktop-app agent socket (contract: 4 MiB).
pub const MAX_SOCKET: usize = 4 * 1024 * 1024;

/// Like [`read_message`] with an explicit size limit.
pub fn read_message_max<R: Read>(r: &mut R, max: usize) -> Result<Option<serde_json::Value>> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => bail!("truncated length header"),
            Ok(n) => got += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let n = u32::from_le_bytes(len) as usize;
    if n > max { bail!("message too large: {n} bytes (max {max})"); }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).map_err(|e| anyhow::anyhow!("truncated message body: {e}"))?;
    Ok(Some(serde_json::from_slice(&buf)?))
}

/// Serialize + frame one message. Errors if it exceeds [`MAX_OUT`].
pub fn encode_message(v: &serde_json::Value) -> Result<Vec<u8>> { encode_message_max(v, MAX_OUT) }

/// Like [`encode_message`] with an explicit size limit.
pub fn encode_message_max(v: &serde_json::Value, max: usize) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(v)?;
    if body.len() > max { bail!("outgoing message too large: {} bytes", body.len()); }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn write_message<W: Write>(w: &mut W, v: &serde_json::Value) -> Result<()> {
    w.write_all(&encode_message(v)?)?;
    w.flush()?;
    Ok(())
}
