//! Wire protocol shared by `whoisthat run` and `whoisthat-runner`.
//!
//! Frame: `u32 big-endian len` followed by `len` bytes, where the first byte is
//! the kind and the rest is the payload. `len` includes the kind byte.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const PROTOCOL: u32 = 2;
pub const MAX_DATA_FRAME: usize = 1 << 20;
pub const MAX_CONTROL_FRAME: usize = 3 << 19; // 1.5 MiB, Spawn carries env
pub const MAX_ENV_BYTES: usize = 1 << 20;
pub const MAX_ARGV_BYTES: usize = 128 << 10;

pub mod kind {
    pub const CONTROL: u8 = 1;
    pub const STDIN: u8 = 2;
    pub const STDOUT: u8 = 3;
    pub const STDERR: u8 = 4;
    pub const EXIT: u8 = 5;
    pub const RESIZE: u8 = 6;
    pub const SIGNAL: u8 = 7;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spawn {
    pub protocol: u32,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub umask: u32,
    #[serde(default)]
    pub rows: u16,
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub tty: bool,
    #[serde(default)]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Control {
    Started { pid: i32, protocol: u32 },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ExitStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Resize {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Signal {
    pub signal: i32,
}

impl Spawn {
    /// Reject anything the runner must not exec before touching the process
    /// table. The runner repeats every check: the client is untrusted.
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != PROTOCOL {
            return Err(format!(
                "runner protocol {} does not match client protocol {PROTOCOL}",
                self.protocol
            ));
        }
        if self.argv.is_empty() || self.argv[0].is_empty() {
            return Err("argv is empty".into());
        }
        let mut argv_bytes = 0usize;
        for arg in &self.argv {
            if arg.as_bytes().contains(&0) {
                return Err("argv contains NUL".into());
            }
            argv_bytes += arg.len() + 1;
        }
        if argv_bytes > MAX_ARGV_BYTES {
            return Err("argv is too large".into());
        }
        if !std::path::Path::new(&self.cwd).is_absolute() {
            return Err("cwd must be an absolute path".into());
        }
        let mut env_bytes = 0usize;
        for (key, value) in &self.env {
            if key.is_empty() || key.as_bytes().contains(&0) || value.as_bytes().contains(&0) {
                return Err("env contains an empty key or NUL".into());
            }
            env_bytes += key.len() + value.len() + 2;
        }
        if env_bytes > MAX_ENV_BYTES {
            return Err("env is too large".into());
        }
        if self.tty && self.detach {
            return Err("tty and detach are mutually exclusive".into());
        }
        if self.tty && (self.rows == 0 || self.cols == 0) {
            return Err("tty size must be provided".into());
        }
        if self.umask > 0o777 {
            return Err("invalid umask".into());
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub fn write_frame(writer: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let len = payload.len() + 1;
    let max = if kind == kind::CONTROL {
        MAX_CONTROL_FRAME
    } else {
        MAX_DATA_FRAME
    };
    if len > max {
        return Err(invalid("frame exceeds the protocol limit"));
    }
    writer.write_all(&(len as u32).to_be_bytes())?;
    writer.write_all(&[kind])?;
    writer.write_all(payload)?;
    writer.flush()
}

pub fn read_frame(reader: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_CONTROL_FRAME {
        return Err(invalid(format!("invalid frame length {len}")));
    }
    let mut buffer = vec![0u8; len];
    reader.read_exact(&mut buffer)?;
    let kind = buffer[0];
    buffer.remove(0);
    if kind != kind::CONTROL && buffer.len() > MAX_DATA_FRAME {
        return Err(invalid("data frame exceeds the protocol limit"));
    }
    Ok((kind, buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn roundtrip(kind: u8, payload: &[u8]) {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, kind, payload).expect("write");
        let mut cursor = Cursor::new(buffer);
        let (got_kind, got_payload) = read_frame(&mut cursor).expect("read");
        assert_eq!(got_kind, kind);
        assert_eq!(got_payload, payload);
    }

    #[test]
    fn frames_roundtrip() {
        roundtrip(kind::CONTROL, br#"{"type":"started"}"#);
        roundtrip(kind::STDOUT, b"hello");
        roundtrip(kind::STDIN, &[]);
    }

    #[test]
    fn rejects_empty_and_oversized_frames() {
        let mut empty = Cursor::new(vec![0, 0, 0, 0]);
        assert!(read_frame(&mut empty).is_err());

        let mut header = (MAX_CONTROL_FRAME + 1).to_be_bytes().to_vec();
        header.extend_from_slice(&[kind::CONTROL]);
        assert!(read_frame(&mut Cursor::new(header)).is_err());

        let mut too_big = Vec::new();
        let payload = vec![0u8; MAX_DATA_FRAME + 1];
        assert!(write_frame(&mut too_big, kind::STDOUT, &payload).is_err());
    }

    #[test]
    fn rejects_truncated_frames() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, kind::STDOUT, b"abc").expect("write");
        buffer.truncate(buffer.len() - 1);
        assert!(read_frame(&mut Cursor::new(buffer)).is_err());
    }

    fn spawn() -> Spawn {
        Spawn {
            protocol: PROTOCOL,
            argv: vec!["sh".into(), "-c".into(), "true".into()],
            env: vec![("PATH".into(), "/usr/bin".into())],
            cwd: "/tmp".into(),
            umask: 0o022,
            rows: 0,
            cols: 0,
            tty: false,
            detach: false,
        }
    }

    #[test]
    fn validates_spawn() {
        assert!(spawn().validate().is_ok());

        let mut bad = spawn();
        bad.argv.clear();
        assert!(bad.validate().is_err());

        let mut bad = spawn();
        bad.cwd = "relative".into();
        assert!(bad.validate().is_err());

        let mut bad = spawn();
        bad.protocol = PROTOCOL + 1;
        assert!(bad.validate().is_err());

        let mut bad = spawn();
        bad.env = vec![("A\0B".into(), "x".into())];
        assert!(bad.validate().is_err());

        let mut bad = spawn();
        bad.tty = true;
        assert!(bad.validate().is_err(), "tty without a size");
        bad.rows = 24;
        bad.cols = 80;
        assert!(bad.validate().is_ok());
        bad.detach = true;
        assert!(bad.validate().is_err());
    }
}
