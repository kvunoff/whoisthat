use base64::Engine;

use crate::core_client::protocol::{AntiDPIConfig, AntiDPIFragment, AntiDPINoise};

pub const PACKET_PRESETS: [&str; 4] = ["tlshello", "1-1", "1-2", "1-3"];
pub const NOISE_TYPES: [&str; 4] = ["rand", "str", "base64", "hex"];

const MAX_INT32: i64 = 2_147_483_647;
const MAX_FRAGMENT_LENGTH: i64 = 65_535;
const MAX_DELAY_MS: i64 = 60_000;
const MAX_NOISE_PACKET: i64 = 4_096;

impl Default for AntiDPIFragment {
    fn default() -> Self {
        Self {
            enabled: false,
            packets: "tlshello".into(),
            length: "100-200".into(),
            interval: "10-20".into(),
        }
    }
}

impl Default for AntiDPINoise {
    fn default() -> Self {
        Self {
            enabled: false,
            noise_type: "rand".into(),
            packet: "50-100".into(),
            delay: "10-16".into(),
        }
    }
}

pub fn default_packet_for_type(noise_type: &str) -> &'static str {
    match noise_type {
        "str" => "whoisthat",
        "base64" => "d2hvaXN0aGF0",
        "hex" => "77686f697374686174",
        _ => "50-100",
    }
}

pub fn next_packets(current: &str) -> &'static str {
    match PACKET_PRESETS.iter().position(|p| *p == current) {
        Some(i) => PACKET_PRESETS[(i + 1) % PACKET_PRESETS.len()],
        None => PACKET_PRESETS[0],
    }
}

pub fn next_noise_type(current: &str) -> &'static str {
    match NOISE_TYPES.iter().position(|t| *t == current) {
        Some(i) => NOISE_TYPES[(i + 1) % NOISE_TYPES.len()],
        None => NOISE_TYPES[0],
    }
}

pub fn validate(cfg: &AntiDPIConfig) -> Result<(), String> {
    if !valid_packets(&cfg.fragment.packets) {
        return Err("anti-dpi.fragment.packets must be tlshello or N-M (N >= 1)".into());
    }
    if !valid_range(&cfg.fragment.length, 1, MAX_FRAGMENT_LENGTH) {
        return Err(format!(
            "anti-dpi.fragment.length must be N-M with 1 <= N <= M <= {MAX_FRAGMENT_LENGTH}"
        ));
    }
    if !valid_range(&cfg.fragment.interval, 0, MAX_DELAY_MS) {
        return Err(format!(
            "anti-dpi.fragment.interval must be N-M with 0 <= N <= M <= {MAX_DELAY_MS}"
        ));
    }
    if !NOISE_TYPES.contains(&cfg.noise.noise_type.as_str()) {
        return Err("anti-dpi.noise.type must be rand, str, base64, or hex".into());
    }
    if cfg.noise.noise_type == "rand" {
        if !valid_range(&cfg.noise.packet, 1, MAX_NOISE_PACKET) {
            return Err(format!(
                "anti-dpi.noise.packet must be N-M with 1 <= N <= M <= {MAX_NOISE_PACKET} for rand"
            ));
        }
    } else if !valid_noise_payload(&cfg.noise.noise_type, &cfg.noise.packet) {
        return Err(format!(
            "anti-dpi.noise.packet must be a valid non-empty {} payload (max {MAX_NOISE_PACKET} bytes)",
            cfg.noise.noise_type
        ));
    }
    if !valid_range(&cfg.noise.delay, 0, MAX_DELAY_MS) {
        return Err(format!(
            "anti-dpi.noise.delay must be N-M with 0 <= N <= M <= {MAX_DELAY_MS}"
        ));
    }
    Ok(())
}

/// State-free checks used before the CLI opens a core session.
pub fn validate_field_syntax(key: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    match key {
        "anti-dpi.fragment.enabled" | "anti-dpi.noise.enabled" => Ok(()),
        "anti-dpi.fragment.packets" => {
            if valid_packets(value) {
                Ok(())
            } else {
                Err("anti-dpi.fragment.packets must be tlshello or N-M (N >= 1)".into())
            }
        }
        "anti-dpi.fragment.length" => {
            if valid_range(value, 1, MAX_FRAGMENT_LENGTH) {
                Ok(())
            } else {
                Err(format!(
                    "anti-dpi.fragment.length must be N-M with 1 <= N <= M <= {MAX_FRAGMENT_LENGTH}"
                ))
            }
        }
        "anti-dpi.fragment.interval" => {
            if valid_range(value, 0, MAX_DELAY_MS) {
                Ok(())
            } else {
                Err(format!(
                    "anti-dpi.fragment.interval must be N-M with 0 <= N <= M <= {MAX_DELAY_MS}"
                ))
            }
        }
        "anti-dpi.noise.type" => {
            if NOISE_TYPES.contains(&value) {
                Ok(())
            } else {
                Err("anti-dpi.noise.type must be rand, str, base64, or hex".into())
            }
        }
        "anti-dpi.noise.packet" => {
            if value.is_empty() || value.len() > MAX_NOISE_PACKET as usize {
                Err(format!(
                    "anti-dpi.noise.packet must be non-empty and at most {MAX_NOISE_PACKET} characters"
                ))
            } else {
                Ok(())
            }
        }
        "anti-dpi.noise.delay" => {
            if valid_range(value, 0, MAX_DELAY_MS) {
                Ok(())
            } else {
                Err(format!(
                    "anti-dpi.noise.delay must be N-M with 0 <= N <= M <= {MAX_DELAY_MS}"
                ))
            }
        }
        other => Err(format!("unknown anti-DPI setting '{other}'")),
    }
}

/// Checks that need the current config (noise.packet depends on noise.type).
pub fn validate_field_against_config(
    key: &str,
    value: &str,
    cfg: &AntiDPIConfig,
) -> Result<(), String> {
    validate_field_syntax(key, value)?;
    if key != "anti-dpi.noise.packet" {
        return Ok(());
    }
    let value = value.trim();
    if cfg.noise.noise_type == "rand" {
        if valid_range(value, 1, MAX_NOISE_PACKET) {
            Ok(())
        } else {
            Err(format!(
                "anti-dpi.noise.packet must be N-M with 1 <= N <= M <= {MAX_NOISE_PACKET} for rand"
            ))
        }
    } else if valid_noise_payload(&cfg.noise.noise_type, value) {
        Ok(())
    } else {
        Err(format!(
            "anti-dpi.noise.packet must be a valid non-empty {} payload (max {MAX_NOISE_PACKET} bytes)",
            cfg.noise.noise_type
        ))
    }
}

fn valid_packets(value: &str) -> bool {
    value == "tlshello" || parse_range(value, 1, MAX_INT32).is_some()
}

fn valid_range(value: &str, min: i64, max: i64) -> bool {
    parse_range(value, min, max).is_some()
}

fn parse_range(value: &str, min: i64, max: i64) -> Option<(i64, i64)> {
    let (from, to) = value.split_once('-')?;
    if to.contains('-') {
        return None;
    }
    let from: i64 = from.trim().parse().ok()?;
    let to: i64 = to.trim().parse().ok()?;
    if from < min || to > max || from > to {
        return None;
    }
    Some((from, to))
}

fn valid_noise_payload(noise_type: &str, payload: &str) -> bool {
    if payload.is_empty() || payload.len() > MAX_NOISE_PACKET as usize {
        return false;
    }
    match noise_type {
        "str" => true,
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map(|decoded| !decoded.is_empty() && decoded.len() <= MAX_NOISE_PACKET as usize)
            .unwrap_or(false),
        "hex" => {
            payload.len().is_multiple_of(2)
                && payload.bytes().all(|b| b.is_ascii_hexdigit())
                && payload.len() / 2 <= MAX_NOISE_PACKET as usize
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> AntiDPIConfig {
        AntiDPIConfig {
            fragment: AntiDPIFragment {
                enabled: true,
                ..Default::default()
            },
            noise: AntiDPINoise {
                enabled: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn defaults_are_not_empty() {
        let cfg = AntiDPIConfig::default();
        assert_eq!(cfg.fragment.packets, "tlshello");
        assert_eq!(cfg.fragment.length, "100-200");
        assert_eq!(cfg.fragment.interval, "10-20");
        assert_eq!(cfg.noise.noise_type, "rand");
        assert_eq!(cfg.noise.packet, "50-100");
        assert_eq!(cfg.noise.delay, "10-16");
        assert!(validate(&cfg).is_ok());
    }

    #[test]
    fn packets_accept_any_ordered_range() {
        for value in ["tlshello", "1-1", "1-3", "5-5", "1-2147483647"] {
            let mut cfg = valid();
            cfg.fragment.packets = value.into();
            assert!(validate(&cfg).is_ok(), "{value} should be valid");
        }
        for value in ["", "0-1", "3-1", "1-2147483648", "5"] {
            let mut cfg = valid();
            cfg.fragment.packets = value.into();
            assert!(validate(&cfg).is_err(), "{value} should be invalid");
        }
    }

    #[test]
    fn noise_packet_depends_on_type() {
        let mut cfg = valid();
        cfg.noise.noise_type = "hex".into();
        cfg.noise.packet = "77686f697374686174".into();
        assert!(validate(&cfg).is_ok());

        cfg.noise.packet = "50-100".into();
        assert!(validate(&cfg).is_err());

        cfg.noise.noise_type = "rand".into();
        cfg.noise.packet = "50-100".into();
        assert!(validate(&cfg).is_ok());

        cfg.noise.packet = "1-4097".into();
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn field_validation_uses_current_noise_type() {
        let mut cfg = valid();
        cfg.noise.noise_type = "base64".into();
        assert!(
            validate_field_against_config("anti-dpi.noise.packet", "d2hvaXN0aGF0", &cfg).is_ok()
        );
        assert!(validate_field_against_config("anti-dpi.noise.packet", "??", &cfg).is_err());

        cfg.noise.noise_type = "rand".into();
        assert!(validate_field_against_config("anti-dpi.noise.packet", "10-20", &cfg).is_ok());
        assert!(
            validate_field_against_config("anti-dpi.noise.packet", "d2hvaXN0aGF0", &cfg).is_err()
        );
    }

    #[test]
    fn cycling_is_stable() {
        assert_eq!(next_packets("tlshello"), "1-1");
        assert_eq!(next_packets("1-3"), "tlshello");
        assert_eq!(next_packets("9-9"), "tlshello");
        assert_eq!(next_noise_type("rand"), "str");
        assert_eq!(next_noise_type("hex"), "rand");
        assert_eq!(default_packet_for_type("hex"), "77686f697374686174");
    }
}
