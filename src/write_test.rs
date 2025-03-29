//! Running the "write" portion of the test.

use crate::crypto::GarbageGenerator;
use crate::metadata::TestOptions;
use crate::PROGRESS_STYLE;
use anyhow::Context as _;
use compio::fs::File;
use compio::io::{AsyncWrite, AsyncWriteAt};
use compio::{fs::OpenOptions, io::BufWriter};
use std::io::Cursor;
use std::{
    io::{self},
    path::Path,
};
use tracing::{info_span, Span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

#[tracing::instrument(skip(opts))]
pub(crate) async fn write(dev_path: &Path, opts: &TestOptions) -> anyhow::Result<()> {
    let bar_span = info_span!("writing");
    bar_span.pb_set_style(&PROGRESS_STYLE);
    bar_span.pb_set_length(opts.device_capacity);
    let _current_span = bar_span.enter();

    let mut out = BufWriter::with_capacity(
        opts.buffer_size,
        Cursor::new(
            OpenOptions::new()
                .write(true)
                .open(dev_path)
                .await
                .with_context(|| format!("Opening the device {:?} for writing", dev_path))?,
        ),
    );
    let generator = GarbageGenerator::new(opts.buffer_size, opts.seed, |_| {});
    let mut generator = compio::io::BufReader::new(generator);
    match compio::io::copy(&mut generator, &mut out).await {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(28) => {
            // "disk full", meaning we're done:
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::WriteZero => {
            // "disk full" on macOS, meaning we're done:
            Ok(())
        }
        Err(e) => anyhow::bail!("io Error {:?}: kind {:?}", e, e.kind()),
    }
}
