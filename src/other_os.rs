use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::Args;

#[derive(Debug, Clone, Default)]
pub(crate) struct DeviceMetadata {
    pub physical_block_size: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidDevice {
    path: PathBuf,
    partition: Option<u64>,
    device: DeviceMetadata,
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

impl ValidDevice {
    pub(crate) fn safe_path(&self, args: &Args) -> anyhow::Result<&Path> {
        if args.i_know_what_im_doing_let_me_skip_sanity_checks {
            Ok(self.path)
        } else {
            anyhow::bail!("I have no way to run sanity checks on this platform. Run with --i-know-what-im-doing-let-me-skip-sanity-checks if you want to destroy {:?} anyway.", device_path);
        }
    }

    pub(crate) fn physical_block_size(&self) -> Option<u64> {
        None
    }
}
