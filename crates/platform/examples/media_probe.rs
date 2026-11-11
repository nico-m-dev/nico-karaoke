//! Manual media-provider probe.
//!
//! Run this while a player is active:
//!
//! ```text
//! cargo run -p nico-karaoke-platform --example media_probe
//! ```
//!
//! It prints every player the current platform backend can see. It is intentionally a
//! developer tool, not part of the app UI.

#[tokio::main]
async fn main() {
    let provider = nico_karaoke_platform::create_provider().await;
    match provider.snapshots().await {
        Ok(snapshots) => {
            println!("{} player(s)", snapshots.len());
            for snapshot in snapshots {
                println!(
                    "{} | {} - {} | playing={} position={:?} length={:?} seek={}",
                    snapshot.label,
                    snapshot.artist,
                    snapshot.title,
                    snapshot.playing,
                    snapshot.position,
                    snapshot.length,
                    snapshot.can_seek
                );
            }
        }
        Err(error) => eprintln!("media provider unavailable: {error}"),
    }
}
