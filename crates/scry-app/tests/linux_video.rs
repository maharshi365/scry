#![cfg(target_os = "linux")]

use anyhow::{Result, ensure};
use scry_app::platform::{Host, VideoHost, VideoPresentation};
use scry_playback::{PlaybackTarget, Player};
use std::{
    io::Write,
    time::{Duration, Instant},
};

#[test]
fn linux_host_uses_composited_video() -> Result<()> {
    let host = Host::default();
    ensure!(matches!(host.target()?, PlaybackTarget::Software(_)));
    ensure!(matches!(host.presentation(), VideoPresentation::Pixels(_)));
    Ok(())
}

#[test]
#[ignore = "requires an installed libmpv runtime"]
fn linux_host_delivers_resized_frames_and_clears_after_playback() -> Result<()> {
    let mut media = tempfile::Builder::new().suffix(".y4m").tempfile()?;
    media.write_all(b"YUV4MPEG2 W16 H16 F24:1 Ip A1:1 C420jpeg\n")?;
    for _ in 0..48 {
        media.write_all(b"FRAME\n")?;
        media.write_all(&[180; 256])?;
        media.write_all(&[128; 128])?;
    }
    media.flush()?;
    let host = Host::default();
    for _ in 0..2 {
        host.place([0.0, 0.0, 32.0, 16.0], 2.0);
        let player = Player::open(host.target()?)?;
        player.command(&["set", "pause", "yes"])?;
        player.load(
            media
                .path()
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid fixture path"))?,
        )?;
        wait_for_size(&host, (64, 32))?;
        host.place([0.0, 0.0, 48.0, 16.0], 2.0);
        wait_for_size(&host, (96, 32))?;
        drop(player);
        host.hide();
        ensure!(
            pixels_size(&host)? == (0, 0),
            "Closed video was not cleared"
        );
    }
    Ok(())
}

fn pixels_size(host: &Host) -> Result<(u32, u32)> {
    match host.presentation() {
        VideoPresentation::Pixels(pixels) => Ok(pixels.size()),
        VideoPresentation::Native => anyhow::bail!("Expected composited video"),
    }
}

fn wait_for_size(host: &Host, expected: (u32, u32)) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while pixels_size(host)? != expected {
        ensure!(
            Instant::now() < deadline,
            "Video did not resize to {expected:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
