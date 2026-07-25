use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub(crate) fn fetch_public_ip() -> Option<String> {
    let addr = "api.ipify.org:80"
        .to_socket_addrs()
        .ok()?
        .find(|a| a.is_ipv4())?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .write_all(b"GET / HTTP/1.0\r\nHost: api.ipify.org\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    let body = buf.split("\r\n\r\n").nth(1)?;
    let ip = body.trim();
    if ip.is_empty() {
        None
    } else {
        Some(ip.to_string())
    }
}

pub(crate) fn fetch_public_ipv6() -> Option<String> {
    let addr = "api6.ipify.org:80".to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .write_all(b"GET / HTTP/1.0\r\nHost: api6.ipify.org\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    let body = buf.split("\r\n\r\n").nth(1)?;
    let ip = body.trim();
    if ip.is_empty() {
        None
    } else {
        Some(ip.to_string())
    }
}

pub(crate) fn check_sudo_env() -> Option<&'static str> {
    if std::env::var("SUDO_UID").is_ok() && std::env::var("HOME").unwrap_or_default() == "/root" {
        Some("Restart with: sudo -E whoisthat")
    } else {
        None
    }
}
