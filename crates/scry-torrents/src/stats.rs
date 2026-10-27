/// Small, address-free snapshot for the selected video, not the entire torrent pack.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: u64,
    pub download_bytes_per_second: Option<f64>,
    pub upload_bytes_per_second: Option<f64>,
    pub peers: Option<usize>,
}

impl Stats {
    fn selected_file(file_progress: &[u64], index: usize, total_bytes: u64) -> Self {
        Self {
            downloaded_bytes: file_progress
                .get(index)
                .map(|bytes| (*bytes).min(total_bytes)),
            total_bytes,
            ..Self::default()
        }
    }
}

impl super::Torrents {
    #[must_use]
    pub fn stats(&self) -> Option<Stats> {
        let handle = self
            .session
            .with_torrents(|torrents| torrents.next().map(|(_, handle)| handle.clone()))?;
        let files = handle.only_files()?;
        let index = *files.first()?;
        let total = handle
            .with_metadata(|meta| meta.file_infos.get(index).map(|file| file.len))
            .ok()??;
        let snapshot = handle.stats();
        let mut stats = Stats::selected_file(&snapshot.file_progress, index, total);
        if let Some(live) = snapshot.live {
            // librqbit reports MiB/s, despite naming the field `mbps`.
            stats.download_bytes_per_second = Some(live.download_speed.mbps * 1_048_576.0);
            stats.upload_bytes_per_second = Some(live.upload_speed.mbps * 1_048_576.0);
            stats.peers = Some(live.snapshot.peer_stats.live);
        }
        Some(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_video_progress_does_not_include_other_pack_files() {
        let stats = Stats::selected_file(&[1000, 250, 8000], 1, 1000);
        assert_eq!(stats.downloaded_bytes, Some(250));
        assert_eq!(stats.total_bytes, 1000);
        assert_eq!(Stats::selected_file(&[], 1, 1000).downloaded_bytes, None);
        assert_eq!(
            Stats::selected_file(&[1100], 0, 1000).downloaded_bytes,
            Some(1000)
        );
    }
}
