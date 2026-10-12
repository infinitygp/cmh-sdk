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

    /// A display module was asked for something it only does in software
    /// mode: its firmware version, its backlight level, or a new backlight
    /// level. Call
    /// [`Module::set_software_mode`](crate::Module::set_software_mode) on
    /// the same handle first. That call returns this error itself when the
    /// module did not confirm the switch.
    #[error("{module} is not in software mode")]
    NotInSoftwareMode { module: &'static str },

    /// A display module kept reporting another backlight level than the one
    /// that was set.
    #[error("{module} reports display brightness {found} after {wanted} was set")]
    BrightnessNotApplied {
        module: &'static str,
        wanted: u8,
        found: u8,
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
