use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use indicatif::ProgressStyle;
use rand::prelude::*;
use rayon::iter::Either;
use rayon::prelude::*;
use tracing::error;
use tracing::info;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[macro_use]
extern crate lazy_static;

mod crypto;
mod read_test;
mod write_test;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::ValidDevice;

#[cfg(not(target_os = "linux"))]
mod other_os;
#[cfg(not(target_os = "linux"))]
use other_os::ValidDevice;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Args {
    /// Name of the devices to test.
    ///
    /// Each should be a mechanical disk block device (e.g. /dev/sda,
    /// /dev/disk/by-id/wwn-...).
    #[clap(value_parser = clap::value_parser!(ValidDevice), num_args = 1..)]
    devices: Vec<ValidDevice>,

    #[clap(long, short='j', default_value_t = num_cpus::get())]
    num_threads: usize,

    /// Number of bytes to buffer for writing.
    ///
    /// Defaults to the physical block size of the device (or 8192 if that is unset).
    #[clap(long)]
    buffer_size: Option<usize>,

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
    let indicatif_layer = IndicatifLayer::new();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .init();
    let args = Args::parse();
    let seed = args.seed.unwrap_or_else(|| thread_rng().gen());
    let (_, failed) = args.devices.clone().into_par_iter().map(|device| {
        let path = device.safe_path(&args)?.to_owned();
         let buffer_size = args.buffer_size.unwrap_or_else(|| {
           device
              .physical_block_size()
               .unwrap_or(8192)
               .try_into()
              .unwrap()
        });
        rayon::ThreadPoolBuilder::new().num_threads(args.num_threads).build_global().context("Initializing rayon")?;
        info!(?seed, ?device, ?path, "Starting test");

        write_test::write(&path, buffer_size, seed).context("During write test")?;
        info!(device=?path, "write test succeeded");
        match read_test::read_back(&path, buffer_size, seed).context("During read test")? {
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
    if failed.len() > 0 {
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
