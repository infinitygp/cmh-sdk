//! Learns that the list of HID devices may have changed, so that nothing has
//! to enumerate on a timer.
//!
//! The source only says "look again": which module came or went is found
//! out by the enumeration that follows. A spurious call costs one
//! enumeration, a missing one is caught by the deck's safety rescan.

/// Keeps an event source running. Dropping it stops the source.
pub(crate) struct Watch(#[allow(dead_code)] platform::Guard);

/// Starts an event source that calls `changed` whenever a HID device may
/// have appeared, disappeared or been changed. The call comes from another
/// thread. `None` where the platform has no source or it cannot be set up.
pub(crate) fn start(changed: Box<dyn Fn() + Send + Sync>) -> Option<Watch> {
    platform::start(changed).map(Watch)
}

#[cfg(target_os = "linux")]
mod platform {
    //! An inotify watch of `/dev` for the `hidraw*` nodes.
    //!
    //! A node is created by the kernel as root's, and udev gives the user
    //! access a moment later (owner, group or ACL), which is an attribute
    //! change on the same node. Both are reported, so the enumeration that
    //! follows the first one may find a node that cannot be opened yet and
    //! the one after the second finds it open. The deck retries a failed
    //! open for the same reason.

    use std::fs::File;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;
    use std::path::Path;

    /// Dropping the stream closes the end the thread polls, which stops it.
    pub(super) struct Guard(#[allow(dead_code)] UnixStream);

    pub(super) fn start(changed: Box<dyn Fn() + Send + Sync>) -> Option<Guard> {
        watch(Path::new("/dev"), changed)
    }

    fn watch(dir: &Path, changed: Box<dyn Fn() + Send + Sync>) -> Option<Guard> {
        use std::os::unix::ffi::OsStrExt;

        let dir = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        // SAFETY: a plain system call; the descriptor it returns is owned
        // from here on.
        let inotify = unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC);
            if fd < 0 {
                return None;
            }
            OwnedFd::from_raw_fd(fd)
        };
        let mask = libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_ATTRIB
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO;
        // SAFETY: `dir` is NUL terminated and outlives the call.
        if unsafe { libc::inotify_add_watch(inotify.as_raw_fd(), dir.as_ptr(), mask) } < 0 {
            return None;
        }
        let (guard, parked) = UnixStream::pair().ok()?;
        std::thread::Builder::new()
            .name("cmh-hotplug".to_owned())
            .spawn(move || serve(File::from(inotify), parked, changed))
            .ok()?;
        Some(Guard(guard))
    }

    fn serve(mut inotify: File, parked: UnixStream, changed: Box<dyn Fn() + Send + Sync>) {
        let mut buf = [0u8; 4096];
        loop {
            let mut fds = [
                libc::pollfd {
                    fd: inotify.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: parked.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: `fds` is an array of two valid `pollfd`.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if ready < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            // The deck let go of its end.
            if fds[1].revents != 0 {
                return;
            }
            if fds[0].revents != 0 {
                match inotify.read(&mut buf) {
                    Ok(len) if concerns_hidraw(&buf[..len]) => changed(),
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return,
                }
            }
        }
    }

    /// Whether the `inotify_event` records in `buf` name a `hidraw` node, or
    /// say that events were lost. A record that does not parse counts too.
    fn concerns_hidraw(mut buf: &[u8]) -> bool {
        const HEADER: usize = 16;
        while !buf.is_empty() {
            if buf.len() < HEADER {
                return true;
            }
            let mask = u32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]);
            let len = u32::from_ne_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
            let Some(name) = buf.get(HEADER..HEADER + len) else {
                return true;
            };
            if mask & libc::IN_Q_OVERFLOW != 0 || name.starts_with(b"hidraw") {
                return true;
            }
            buf = &buf[HEADER + len..];
        }
        false
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};

        use super::*;

        fn record(mask: u32, name: &[u8]) -> Vec<u8> {
            // Names are padded with NULs to a multiple of 16 bytes.
            let len = name.len().div_ceil(16) * 16;
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&1i32.to_ne_bytes());
            bytes.extend_from_slice(&mask.to_ne_bytes());
            bytes.extend_from_slice(&0u32.to_ne_bytes());
            bytes.extend_from_slice(&(len as u32).to_ne_bytes());
            bytes.extend_from_slice(name);
            bytes.resize(16 + len, 0);
            bytes
        }

        #[test]
        fn only_hidraw_nodes_and_lost_events_count() {
            assert!(concerns_hidraw(&record(libc::IN_CREATE, b"hidraw7")));
            assert!(concerns_hidraw(&record(libc::IN_ATTRIB, b"hidraw12")));
            assert!(!concerns_hidraw(&record(libc::IN_ATTRIB, b"tty1")));
            assert!(!concerns_hidraw(&[]));
            assert!(concerns_hidraw(&record(libc::IN_Q_OVERFLOW, b"")));
            // A burst: the second record is the one that matters.
            let mut burst = record(libc::IN_CREATE, b"input0");
            burst.extend(record(libc::IN_DELETE, b"hidraw3"));
            assert!(concerns_hidraw(&burst));
            // Cut off in the middle of a record.
            assert!(concerns_hidraw(&record(libc::IN_CREATE, b"tty1")[..10]));
        }

        fn wait_for(what: &str, done: impl Fn() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !done() {
                assert!(Instant::now() < deadline, "timed out: {what}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        #[test]
        fn a_directory_is_watched_until_the_guard_is_dropped() {
            let dir = std::env::temp_dir().join(format!("cmh-watch-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            let counted = calls.clone();
            let guard = watch(
                &dir,
                Box::new(move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                }),
            )
            .expect("inotify is available");

            std::fs::write(dir.join("other"), b"").unwrap();
            std::fs::write(dir.join("hidraw9"), b"").unwrap();
            wait_for("a call for the node", || calls.load(Ordering::SeqCst) >= 1);
            let seen = calls.load(Ordering::SeqCst);

            std::fs::remove_file(dir.join("hidraw9")).unwrap();
            wait_for("a call for its removal", || {
                calls.load(Ordering::SeqCst) > seen
            });

            // The thread ends and lets go of the callback.
            drop(guard);
            wait_for("the thread to end", || Arc::strong_count(&calls) == 1);
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}

#[cfg(windows)]
mod platform {
    //! Device interface notifications of the configuration manager for the
    //! HID class. The callback runs on a thread of the system's pool.
    //!
    //! Not verified on hardware (experiment 0029): `Deck` leaves this off
    //! unless asked, see `DeckOptions::device_events`.

    use std::ffi::c_void;

    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_NOTIFY_ACTION, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_0_0,
        CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CM_Register_Notification,
        CM_Unregister_Notification, CR_SUCCESS, HCMNOTIFICATION,
    };
    use windows_sys::core::GUID;

    type Changed = Box<dyn Fn() + Send + Sync>;

    /// GUID_DEVINTERFACE_HID.
    const HID_CLASS: GUID = GUID {
        data1: 0x4d1e55b2,
        data2: 0xf16f,
        data3: 0x11cf,
        data4: [0x88, 0xcb, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30],
    };

    pub(super) struct Guard {
        handle: HCMNOTIFICATION,
        /// What the callback's context points to; freed after the
        /// notification is unregistered.
        context: *mut Changed,
    }

    // SAFETY: the handle and the context are only used by `drop`, and the
    // callback behind the context is `Send + Sync`.
    unsafe impl Send for Guard {}

    unsafe extern "system" fn callback(
        _handle: HCMNOTIFICATION,
        context: *const c_void,
        _action: CM_NOTIFY_ACTION,
        _data: *const CM_NOTIFY_EVENT_DATA,
        _size: u32,
    ) -> u32 {
        // SAFETY: `context` is the pointer given to `CM_Register_Notification`;
        // it stays valid until `CM_Unregister_Notification` has returned,
        // which waits for running callbacks.
        let changed = unsafe { &*context.cast::<Changed>() };
        changed();
        0
    }

    pub(super) fn start(changed: Changed) -> Option<Guard> {
        let context = Box::into_raw(Box::new(changed));
        let mut filter = CM_NOTIFY_FILTER {
            cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            ..Default::default()
        };
        filter.u.DeviceInterface = CM_NOTIFY_FILTER_0_0 {
            ClassGuid: HID_CLASS,
        };
        let mut handle: HCMNOTIFICATION = std::ptr::null_mut();
        // SAFETY: `filter` and `handle` are valid for the call; the context
        // outlives the registration (see `Guard`).
        let result = unsafe {
            CM_Register_Notification(&filter, context.cast(), Some(callback), &mut handle)
        };
        if result != CR_SUCCESS {
            // SAFETY: the registration failed, so nothing else has the pointer.
            drop(unsafe { Box::from_raw(context) });
            return None;
        }
        Some(Guard { handle, context })
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: `handle` was registered by `start`; the call waits for
            // a callback in progress, so the context can be freed after it.
            unsafe {
                CM_Unregister_Notification(self.handle);
                drop(Box::from_raw(self.context));
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    pub(super) struct Guard;

    pub(super) fn start(_changed: Box<dyn Fn() + Send + Sync>) -> Option<Guard> {
        None
    }
}
