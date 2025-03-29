//! Running the "read back" portion of the test.

use crate::{crypto::GarbageGenerator, metadata::TestOptions, PROGRESS_STYLE};
use anyhow::Context as _;
use compio::{
    fs::File,
    io::{self, AsyncReadExt as _, BufReader},
    BufResult,
};
use std::io::Cursor;
use std::path::Path;
use tracing::{info_span, warn, Span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

type FailedReads = usize;

#[tracing::instrument(skip(opts))]
pub(crate) async fn read_back(
    dev_path: &Path,
    opts: &TestOptions,
) -> anyhow::Result<Result<(), FailedReads>> {
    let blockdev = File::open(dev_path)
        .await
        .with_context(|| format!("Opening the device {:?} for reading", dev_path))?;

    let bar_span = info_span!("reading back");
    bar_span.pb_set_style(&PROGRESS_STYLE);
    bar_span.pb_set_length(opts.device_capacity);
    let _bar_span_handle = bar_span.enter();

    let generator = GarbageGenerator::new(opts.buffer_size, opts.seed, |_| {});
    let generator = BufReader::new(generator);
    let mut compare = CompareWriter::new(generator);
    let mut blockdev = Cursor::new(blockdev);
    io::copy(&mut blockdev, &mut compare).await?;
    if compare.mismatched > 0 {
        return Ok(Err(compare.mismatched));
    }
    Ok(Ok(()))
}

/// A struct that pretends to be [io::Write] by doing block-by-block comparisons against another reader.
#[derive(Debug)]
struct CompareWriter<R> {
    compare: R,
    mismatched: usize,
    current_offset: usize,
}

impl<R> CompareWriter<R> {
    fn new(compare: R) -> Self {
        Self {
            compare,
            mismatched: 0,
            current_offset: 0,
        }
    }
}

impl<R: io::AsyncRead> io::AsyncWrite for CompareWriter<R> {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        let input = buf.as_slice();
        let mut read = Vec::with_capacity(input.len());
        read.resize(input.len(), 0);
        let BufResult(_read_result, read) = self.compare.read_exact(read).await;
        self.current_offset += input.len();
        if read != input {
            warn!(
                offset = self.current_offset,
                "Did not read back the exact bytes written"
            );
            self.mismatched += 1;
        }
        Span::current().pb_inc(read.len() as u64);
        compio::BufResult(Ok(read.len()), buf)
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<R: std::io::Read> std::io::Write for CompareWriter<R> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut read = Vec::with_capacity(buf.len());
        read.resize(buf.len(), 0);
        self.compare.read_exact(&mut read)?;
        self.current_offset += buf.len();
        if &read != buf {
            warn!(
                offset = self.current_offset,
                "Did not read back the exact bytes written"
            );
            self.mismatched += 1;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::CompareWriter;
    use std::io;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn detects_issues() {
        let input: Vec<u8> = vec![1; 1024 * 1024];
        let mut read_back: Vec<u8> = vec![1; 1024 * 1024];
        read_back[1024 * 512] = 255; // corrupt our read-back data
        let mut read_back = io::Cursor::new(read_back);

        let mut compare = CompareWriter::new(io::Cursor::new(input));
        io::copy(&mut read_back, &mut compare).expect("No io errors");
        assert_eq!(compare.mismatched, 1);
    }

    #[traced_test]
    #[test]
    fn succeeds() {
        let input: Vec<u8> = vec![1; 1024 * 1024];
        let read_back: Vec<u8> = vec![1; 1024 * 1024];
        let mut read_back = io::Cursor::new(read_back);
        let mut compare = CompareWriter::new(io::Cursor::new(input));
        io::copy(&mut read_back, &mut compare).expect("No io errors");
        assert_eq!(compare.mismatched, 0);
    }
}
