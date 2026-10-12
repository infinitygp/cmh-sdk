use std::time::Duration;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("HID backend error: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("device disconnected")]
    Disconnected,

    #[error("no response within {0:?}")]
    Timeout(Duration),

    #[error(
        "permission denied opening {path}; on Linux install the udev rules from docs/linux-udev.md"
    )]
    PermissionDenied { path: String },

    #[error("malformed report: {0}")]
    MalformedReport(String),

    #[error("{capability} is not supported by {module}")]
    Unsupported {
        module: &'static str,
        capability: &'static str,
    },

    #[error("no MasterHUB module matching {0}")]
    NotFound(String),
}
