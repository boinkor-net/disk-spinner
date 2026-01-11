use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use aligned_buffer::UniqueAlignedBuffer;

use crate::Args;

pub const OPEN_FLAGS: i32 = libc::O_EXCL;

pub type IOBuffer = UniqueAlignedBuffer<1>;

#[derive(Debug, Clone, Default)]
pub(crate) struct DeviceMetadata {}

#[derive(Debug, Clone)]
pub(crate) struct ValidDevice {
    pub path: PathBuf,
    pub partition: Option<u64>,
    pub device: DeviceMetadata,
}

impl FromStr for ValidDevice {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            path: PathBuf::from(s),
            partition: None,
            device: DeviceMetadata::default(),
        })
    }
}

pub(crate) fn sanity_checks(
    args: &Args,
    _partition: Option<u64>,
    device_path: &Path,
    _device: &DeviceMetadata,
) -> anyhow::Result<()> {
    if args.i_know_what_im_doing_let_me_skip_sanity_checks {
        Ok(())
    } else {
        anyhow::bail!("I have no way to run sanity checks on this platform. Run with --i-know-what-im-doing-let-me-skip-sanity-checks if you want to destroy {:?} anyway.", device_path);
    }
}
