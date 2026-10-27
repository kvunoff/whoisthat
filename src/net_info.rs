use std::net::IpAddr;
use std::time::Duration;

/// Query a public IP echo service over HTTPS.
///
/// Plain HTTP was used historically, which let on-path observers see (and
/// modify) the response rendered in the status bar. We now require TLS and
/// only accept a response that parses as an IP address of the expected family.
fn fetch_ip(url: &str, want_v6: bool) -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(5))
        .timeout_write(Duration::from_secs(5))
        .build();

    let body = agent.get(url).call().ok()?.into_string().ok()?;
    let ip: IpAddr = body.trim().parse().ok()?;
    if ip.is_ipv6() == want_v6 {
        Some(ip.to_string())
    } else {
        None
    }
}

pub(crate) fn fetch_public_ip() -> Option<String> {
    fetch_ip("https://api.ipify.org", false)
}

pub(crate) fn fetch_public_ipv6() -> Option<String> {
    fetch_ip("https://api6.ipify.org", true)
}

pub(crate) fn check_sudo_env() -> Option<&'static str> {
    if std::env::var("SUDO_UID").is_ok() && std::env::var("HOME").unwrap_or_default() == "/root" {
        Some("Restart with: sudo -E whoisthat")
    } else {
        None
    }
}
