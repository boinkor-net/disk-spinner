//! Running the "write" portion of the test.

use crate::crypto::GarbageGenerator;
use crate::metadata::TestOptions;
use crate::PROGRESS_STYLE;
use anyhow::Context as _;
use compio::fs::OpenOptions;
use compio::io::AsyncWriteExt;
use compio::runtime::spawn;
use std::io::Cursor;
use std::{
    io::{self},
    path::Path,
};
use tracing::{info_span, warn};
use tracing_indicatif::span_ext::IndicatifSpanExt;

#[tracing::instrument(skip(opts))]
pub(crate) async fn write(dev_path: &Path, opts: &TestOptions) -> anyhow::Result<()> {
    let bar_span = info_span!("writing");
    bar_span.pb_set_style(&PROGRESS_STYLE);
    bar_span.pb_set_length(opts.device_capacity);
    let _current_span = bar_span.enter();

    let mut out = Cursor::new(
        OpenOptions::new()
            .write(true)
            .open(dev_path)
            .await
            .with_context(|| format!("Opening the device {:?} for writing", dev_path))?,
    );
    let (bytes_send, bytes_recv) = async_channel::bounded(1024);
    let _gen_task = spawn({
        let seed = opts.seed;
        let buffer_size = opts.buffer_size;
        async move {
            let mut generator = GarbageGenerator::new(buffer_size, seed, |_| {});
            loop {
                let mut buf = vec![0; buffer_size];
                if let Err(error) = generator.fill_buffer(&mut buf) {
                    warn!(%error, "Could not fill buffer with random-ish bytes");
                    return;
                }
                if let Err(error) = bytes_send.send(buf).await {
                    warn!(%error, "Could not send random bytes across to consumer");
                    return;
                };
            }
        }
    });
    loop {
        let mut buf = bytes_recv
            .recv()
            .await
            .context("Receiving random-ish bytes")?;
        let res;
        (res, buf) = out.write_all(buf).await.into();
        match res {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(28) => {
                // "disk full", meaning we're done:
                return Ok(());
            }
            Err(e) if e.kind() == io::ErrorKind::WriteZero => {
                return Ok(());
            }
            Err(e) => anyhow::bail!("io Error {:?}: kind {:?}", e, e.kind()),
        }
        bar_span.pb_inc(buf.len() as u64);
    }
}
