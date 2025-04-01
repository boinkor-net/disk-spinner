//! Meta-information about devices under test.

use std::{
    fs::OpenOptions,
    io::{self, Seek},
    path::Path,
};

use anyhow::Context as _;

pub struct TestOptions {
    pub buffer_size: usize,
    pub seed: u64,
    pub device_capacity: u64,
}

/// Opens the given device, seeks to the end and returns the number of bytes skipped over.
///
/// This is a mostly-reliable way to determine the capacity of a device, but I'm wary it might be
/// off by a bit considering block sizes.
pub fn device_capacity(dev_path: &Path) -> anyhow::Result<u64> {
    let mut out = OpenOptions::new()
        .write(true)
        .open(dev_path)
        .context("Opening the device")?;
    out.seek(io::SeekFrom::End(0)).context("Seeking to end")
}
