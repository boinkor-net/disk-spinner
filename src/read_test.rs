//! Running the "read back" portion of the test.

use crate::{crypto::GarbageGenerator, metadata::TestOptions, PROGRESS_STYLE};
use anyhow::Context as _;
use compio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt as _},
};
use std::io::{Cursor, Read};
use std::path::Path;
use tracing::{info_span, warn};
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

    let generator = GarbageGenerator::new(opts.buffer_size, opts.seed, |_| {});
    let blockdev = Cursor::new(blockdev);
    let mismatched =
        compare_persisted_bytes(blockdev, generator, opts.buffer_size, opts.device_capacity)
            .await?;
    if mismatched > 0 {
        return Ok(Err(mismatched));
    }
    Ok(Ok(()))
}

async fn compare_persisted_bytes(
    mut blockdev: impl AsyncRead,
    mut generator: impl Read,
    buffer_size: usize,
    device_capacity: u64,
) -> anyhow::Result<usize> {
    let bar_span = info_span!("reading back");
    bar_span.pb_set_style(&PROGRESS_STYLE);
    bar_span.pb_set_length(device_capacity);
    let _bar_span_handle = bar_span.enter();

    let mut mismatches = 0;
    let mut offset = 0;
    let mut should = vec![0; buffer_size];
    let mut have = vec![0; buffer_size];
    loop {
        let res;
        (res, have) = blockdev.read_exact(have).await.into();
        match res {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(mismatches);
            }
            error => error.map_err(|e| anyhow::anyhow!("Reading bytes on disk: {:?}", e))?,
        }
        if have.is_empty() {
            return Ok(mismatches);
        }
        offset += have.len();
        generator
            .read_exact(&mut should)
            .context("Generating pseudorandom data")?;
        if have != should {
            warn!(offset = offset, "Did not read back the exact bytes written");
            mismatches += 1;
        }
        bar_span.pb_inc(have.len() as u64);
    }
}

#[cfg(test)]
mod test {
    use super::compare_persisted_bytes;
    use std::io;
    use tracing_test::traced_test;

    #[traced_test]
    #[compio::test]
    async fn detects_issues() {
        let input: Vec<u8> = vec![1; 1024 * 1024];
        let mut read_back: Vec<u8> = vec![1; 1024 * 1024];
        read_back[1024 * 512] = 255; // corrupt our read-back data
        let mut read_back = io::Cursor::new(read_back);

        let mismatched = compare_persisted_bytes(
            &mut read_back,
            &mut io::Cursor::new(input),
            1024,
            1024 * 1024,
        )
        .await
        .expect("No io errors");
        assert_eq!(mismatched, 1);
    }

    #[traced_test]
    #[compio::test]
    async fn succeeds() {
        let input: Vec<u8> = vec![1; 1024 * 1024];
        let read_back: Vec<u8> = vec![1; 1024 * 1024];
        let mut read_back = io::Cursor::new(read_back);
        let mismatched = compare_persisted_bytes(
            &mut read_back,
            &mut io::Cursor::new(input),
            1024,
            1024 * 1024,
        )
        .await
        .expect("No io errors");
        assert_eq!(mismatched, 0);
    }
}
