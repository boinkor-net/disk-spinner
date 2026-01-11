use std::fs::OpenOptions;
use std::io;
use std::io::Seek as _;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use garbage::GarbageGeneratorVariant;
use indicatif::ProgressStyle;
use rand::prelude::*;
use rand::rng;
use rayon::iter::Either;
use rayon::prelude::*;
use tracing::error;
use tracing::info;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[macro_use]
extern crate lazy_static;

mod garbage;
mod read_test;
mod write_test;

mod linux;
#[cfg(target_os = "linux")]
use linux::sanity_checks;
#[cfg(target_os = "linux")]
use linux::IOBuffer;
#[cfg(target_os = "linux")]
use linux::ValidDevice;
#[cfg(target_os = "linux")]
use linux::OPEN_FLAGS;

#[cfg(not(target_os = "linux"))]
mod other_os;
#[cfg(not(target_os = "linux"))]
use other_os::sanity_checks;
#[cfg(not(target_os = "linux"))]
use other_os::IOBuffer;
#[cfg(not(target_os = "linux"))]
use other_os::ValidDevice;
#[cfg(not(target_os = "linux"))]
use other_os::OPEN_FLAGS;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Args {
    /// Name of the devices to test.
    ///
    /// Each should be a mechanical disk block device (e.g. /dev/sda,
    /// /dev/disk/by-id/wwn-...).
    #[clap(value_parser = clap::value_parser!(ValidDevice), num_args = 1..)]
    devices: Vec<ValidDevice>,

    /// Number of bytes to buffer from the PRNG for writing or reading.
    #[clap(long, default_value_t = 16*1024*1024)]
    buffer_size: usize,

    #[clap(long, default_value_t, value_parser = clap::value_parser!(GarbageGeneratorVariant))]
    generator: GarbageGeneratorVariant,

    /// Random seed to use for generating random data. By default, this tool generates its own.
    #[clap(long)]
    seed: Option<u64>,

    /// Test the device even if the media type is not a spinning disk.
    #[clap(long)]
    allow_any_media: bool,

    /// Run the test even if the given path is a block device but not
    /// a disk (e.g. a single partition).
    #[clap(long)]
    allow_any_block_device: bool,

    /// Run the test even if any sanity check at all could fail. This is dangerous.
    #[clap(long)]
    i_know_what_im_doing_let_me_skip_sanity_checks: bool,
}

fn main() -> anyhow::Result<()> {
    let indicatif_layer = IndicatifLayer::new().with_max_progress_bars(128, None);
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .init();
    let args = Args::parse();
    let seed = args.seed.unwrap_or_else(|| rng().random());
    let (_, failed) = args.devices.clone().into_par_iter().map(|device| {
        let ValidDevice {
            device,
            partition,
            path,
        } = device;
        let buffer_size = args.buffer_size;
        sanity_checks(&args, partition, &path, &device)?;

        info!(?seed, ?partition, ?device, ?path, "Starting test");
        let write_generator = args.generator.to_generator(buffer_size, seed);
        let written = write_test::write(&path, write_generator, buffer_size).context("During write test")?;
        info!(device=?path, %written, "write test succeeded");
        let read_generator = args.generator.to_generator(buffer_size, seed);
        match read_test::read_back(&path, read_generator, buffer_size, written).context("During read test")? {
            Ok(_) => {
                info!(device=?path, "read-back test succeeded");
                Ok(Either::Left(()))
            }
            Err(n) => {
                error!(device=?path, bad_blocks=?n, "Data on disk is inconsistent/corrupted. THIS IS BAD - RMA THE DRIVE!");
                Ok(Either::Right(path))
            }
        }
    }).collect::<anyhow::Result<(Vec<()>, Vec<PathBuf>)>>()?;
    if !failed.is_empty() {
        error!(devices=?failed, "Devices have failed validation. You should return them.");
        anyhow::bail!("Tests not successful.");
    }
    Ok(())
}

lazy_static! {
    pub(crate) static ref PROGRESS_STYLE: ProgressStyle = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.white/grey} {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta_precise}) {msg}",
    ).expect("Internal error in indicatif progress bar template syntax");
}

/// Open the device at dev_path and determine its size by seeking to the end.
pub(crate) fn determine_size(dev_path: &Path) -> anyhow::Result<u64> {
    let mut out = OpenOptions::new()
        .read(true)
        .open(dev_path)
        .with_context(|| format!("Opening the device {dev_path:?} for determining the size"))?;
    out.seek(io::SeekFrom::End(0)).context("seeking to end")
}
