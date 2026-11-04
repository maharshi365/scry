use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    time::Instant,
};

// Log only numeric range positions and timings, never the local URL, token,
// torrent identity, filename, or peer address.
pub(super) struct ReadTrace<R> {
    inner: R,
    id: usize,
    file: usize,
    start: u64,
    end: u64,
    started: Instant,
    waiting: Option<Instant>,
    wait: Duration,
    longest_wait: Duration,
    bytes: u64,
}

impl<R> ReadTrace<R> {
    pub(super) fn new(inner: R, id: usize, file: usize, start: u64, end: u64) -> Self {
        Self {
            inner,
            id,
            file,
            start,
            end,
            started: Instant::now(),
            waiting: None,
            wait: Duration::ZERO,
            longest_wait: Duration::ZERO,
            bytes: 0,
        }
    }

    fn finish_wait(&mut self) {
        if let Some(started) = self.waiting.take() {
            let elapsed = started.elapsed();
            self.wait += elapsed;
            self.longest_wait = self.longest_wait.max(elapsed);
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ReadTrace<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Pending => {
                this.waiting.get_or_insert_with(Instant::now);
                Poll::Pending
            }
            Poll::Ready(result) => {
                this.finish_wait();
                let read = (buf.filled().len() - before) as u64;
                if read > 0 && this.bytes == 0 {
                    tracing::info!(
                        torrent_id = this.id,
                        file_index = this.file,
                        range_start = this.start,
                        range_end = this.end,
                        elapsed_s = this.started.elapsed().as_secs_f64(),
                        "Torrent HTTP range first bytes available"
                    );
                }
                this.bytes += read;
                Poll::Ready(result)
            }
        }
    }
}

impl<R> Drop for ReadTrace<R> {
    fn drop(&mut self) {
        self.finish_wait();
        tracing::info!(
            torrent_id = self.id,
            file_index = self.file,
            range_start = self.start,
            range_end = self.end,
            bytes = self.bytes,
            elapsed_s = self.started.elapsed().as_secs_f64(),
            read_wait_s = self.wait.as_secs_f64(),
            longest_read_wait_s = self.longest_wait.as_secs_f64(),
            "Torrent HTTP range closed"
        );
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Local stream fixture errors must fail the test"
)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test(start_paused = true)]
    async fn tracks_blocked_reads_and_only_counts_returned_bytes() {
        let (reader, mut writer) = tokio::io::duplex(16);
        let mut trace = ReadTrace::new(reader, 0, 0, 10, 13);
        let mut buf = [0; 4];
        assert!(
            tokio::time::timeout(Duration::from_millis(100), trace.read(&mut buf))
                .await
                .is_err()
        );
        assert_eq!(trace.bytes, 0);
        writer.write_all(b"data").await.unwrap();
        trace.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"data");
        assert_eq!(trace.bytes, 4);
        assert_eq!(trace.wait, Duration::from_millis(100));
        assert_eq!(trace.longest_wait, Duration::from_millis(100));
        assert!(
            trace.waiting.is_none(),
            "A completed read must close its waiting interval"
        );
    }
}
