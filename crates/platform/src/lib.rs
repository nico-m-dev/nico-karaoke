//! Platform boundary for Nico Karaoke.
//!
//! Linux MPRIS and Windows SMTC implementations will live here behind a shared provider
//! trait. The foundation only exposes target information so the server can report what it is
//! running on without pulling platform APIs into the domain crate.

pub const SUPPORTED_OPERATING_SYSTEMS: [&str; 2] = ["linux", "windows"];

pub fn operating_system() -> &'static str {
    std::env::consts::OS
}

pub fn architecture() -> &'static str {
    std::env::consts::ARCH
}

pub fn is_supported() -> bool {
    SUPPORTED_OPERATING_SYSTEMS.contains(&operating_system())
}

pub fn target_label() -> String {
    format!("{}/{}", operating_system(), architecture())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_non_empty_target() {
        assert!(!target_label().is_empty());
        assert!(!operating_system().is_empty());
        assert!(!architecture().is_empty());
    }

    #[test]
    fn supported_list_contains_only_known_targets() {
        for target in SUPPORTED_OPERATING_SYSTEMS {
            assert!(matches!(target, "linux" | "windows"));
        }
    }
}
