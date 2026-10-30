#![cfg(target_os = "macos")]
//! Opt-in smoke test requiring the macOS libmpv runtime, but no display or network.

use anyhow::Result;
use scry_playback::{PlaybackTarget, Player, macos::Surface};
use std::{
    io::Write,
    sync::mpsc,
    time::{Duration, Instant},
};

fn colored_frame(rx: &mpsc::Receiver<(u32, u32, Vec<u8>)>, size: (u32, u32)) -> Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (w, h, rgba) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
        if (w, h) == size && rgba.iter().any(|&value| value > 100 && value < 250) {
            return Ok(rgba);
        }
    }
}

#[test]
#[ignore = "requires Homebrew mpv; run explicitly on macOS"]
fn macos_renders_and_reopens_without_a_native_window() -> Result<()> {
    let mut media = tempfile::Builder::new().suffix(".y4m").tempfile()?;
    media.write_all(b"YUV4MPEG2 W16 H16 F24:1 Ip A1:1 C420jpeg\n")?;
    for _ in 0..48 {
        media.write_all(b"FRAME\n")?;
        media.write_all(&[180; 256])?;
        media.write_all(&[128; 128])?;
    }
    media.flush()?;
    let (tx, rx) = mpsc::sync_channel(1);
    let surface = Surface::new(move |w, h, rgba| {
        let _ = tx.try_send((w, h, rgba));
    });
    // Closing before load must wake an otherwise idle render thread too.
    drop(Player::open(PlaybackTarget::Macos(surface.clone()))?);
    for _ in 0..2 {
        surface.resize(32, 32);
        let player = Player::open(PlaybackTarget::Macos(surface.clone()))?;
        player.load(
            media
                .path()
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Fixture path is not UTF-8"))?,
        )?;
        let rgba = colored_frame(&rx, (32, 32))?;
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!(rgba.chunks_exact(4).all(|pixel| pixel.get(3) == Some(&255)));
        assert!(rgba.iter().any(|&value| value > 100 && value < 250));
        player.command(&["set", "pause", "yes"])?;
        surface.resize(64, 32);
        assert_eq!(colored_frame(&rx, (64, 32))?.len(), 64 * 32 * 4);
        player.command(&["seek", "0", "absolute"])?;
        player.command(&["stop"])?;
        drop(player);
    }
    Ok(())
}
