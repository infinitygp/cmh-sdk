pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The HID backend failed while initialising, enumerating or opening.
    #[error("HID backend error: {0}")]
    Backend(String),

    /// I/O on an open module failed, which in practice means it was
    /// unplugged. The handle is dead: drop it and open the module again once
    /// it is listed by [`Manager::refresh`](crate::Manager::refresh).
    #[error("module disconnected: {0}")]
    Disconnected(String),

    #[error(
        "permission denied opening {path}; on Linux install the udev rule from docs/linux-udev.md"
    )]
    PermissionDenied { path: String },

    /// The device at a known path is no longer the expected module.
    #[error("{path} is not the expected module (found {found})")]
    WrongDevice { path: String, found: String },

    /// A report did not have the documented shape. Not fatal: the module
    /// stays usable and later reports are decoded normally.
    #[error("malformed report: {0}")]
    MalformedReport(String),

    #[error("{capability} is not supported by {module}")]
    Unsupported {
        module: &'static str,
        capability: &'static str,
    },

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("no MasterHUB module matching {0}")]
    NotFound(String),
}

impl Error {
    pub(crate) fn backend(err: hidapi::HidError) -> Self {
        Error::Backend(err.to_string())
    }

    pub(crate) fn disconnected(err: hidapi::HidError) -> Self {
        Error::Disconnected(err.to_string())
    }
}
