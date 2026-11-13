include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

pub fn is_available() -> bool {
    !EMBEDDED_ASSETS.is_empty()
}

pub fn get(path: &str) -> Option<&'static [u8]> {
    EMBEDDED_ASSETS
        .iter()
        .find_map(|(name, bytes)| (*name == path).then_some(*bytes))
}
