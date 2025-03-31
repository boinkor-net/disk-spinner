use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use compio::runtime::spawn;
use futures::stream::FuturesOrdered;
use futures::TryStreamExt;
use indicatif::ProgressStyle;
use metadata::device_capacity;
use metadata::TestOptions;
use rand::prelude::*;
use tracing::error;
use tracing::info;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[macro_use]
extern crate lazy_static;

mod crypto;
mod metadata;
mod read_test;
mod write_test;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::sanity_checks;
#[cfg(target_os = "linux")]
use linux::ValidDevice;

#[cfg(not(target_os = "linux"))]
mod other_os;
#[cfg(not(target_os = "linux"))]
use other_os::sanity_checks;
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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Outcome {
    Good(PathBuf),
    Bad(PathBuf),
    Uncertain(PathBuf),
}

#[compio::main]
async fn main() -> anyhow::Result<()> {
    let indicatif_layer = IndicatifLayer::new();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .init();
    let args = Args::parse();
    let seed = args.seed.unwrap_or_else(|| thread_rng().gen());

    let mut tasks = FuturesOrdered::new();
    for device in args.devices.clone() {
        let ValidDevice {
            device,
            partition,
            path,
        } = device;
        let buffer_size = args.buffer_size.unwrap_or_else(|| {
            device
                .physical_block_size
                .unwrap_or(8192)
                .try_into()
                .unwrap()
        });
        sanity_checks(&args, partition, &path, &device)?;

        info!(?partition, ?device, ?path, "Determining capacity");
        let device_capacity = device_capacity(&path).with_context(|| format!("Determining device capacity of {:?}", &path))?;

        tasks.push_back(spawn(async move { 
            info!(?seed, ?partition, ?device, ?path, "Starting test");

            let opts = TestOptions{buffer_size, seed, device_capacity};
            match write_test::write(&path, &opts).await.context("During write test"){
                Ok(_) => {
                    info!(device=?path, "write test succeeded");
                    match read_test::read_back(&path, &opts).await.context("During read test") {
                        Ok(Ok(_)) => {
                            info!(device=?path, "read-back test succeeded");
                            Outcome::Good(path)
                        }
                        Ok(Err(n)) => {
                            error!(device=?path, bad_blocks=?n, "Data on disk is inconsistent/corrupted. THIS IS BAD - RMA THE DRIVE!");
                            Outcome::Bad(path)
                        }
                        Err(error) => {
                            error!(device=?path, %error, "read-back test resulted in an error. Uncertain if the device works.");
                            Outcome::Uncertain(path)
                        }
                    }
                }
                Err(error) => {
                    error!(device=?path, %error, "write test failed, skipping read-back test. Uncertain if the device works.");
                    Outcome::Uncertain(path)
                }
            }
        }));
    }
    let outcomes = tasks.try_collect::<Vec<_>>().await.map_err(|err| anyhow::anyhow!("Panic in one of the data-integrity test threads: {:?}", err))?;
    let (successful, failed) = outcomes.into_iter().partition::<Vec<_>, _>(|outcome| match outcome { Outcome::Good(_) => true, _ => false});

    if successful.len() > 0 {
        info!(devices=?successful, "Devices have succeeded validation!");
    }
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
