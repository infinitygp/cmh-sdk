# 0029: what an idle deck costs, and waiting instead of looking

- Date: 2026-10-14
- Host: a Fedora Linux 44 (Cloud Edition) virtual machine, kernel
  7.2.8-200.fc44.x86_64, 16 vCPUs of an AMD Ryzen 9 7945HX under KVM
  (Proxmox). The owner's MasterHUB is passed through to it device by device
  on a QEMU xHCI controller: the Base, the 15 LCD Keys, the 3 Knobs, the
  2 Rollers and the Dial Display (no Faders). The base's hub tree does not
  come with them, so no module has a mount path or a placement. Nobody
  operated, re-plugged or watched the modules during the runs.
  Firmware: Base 02.00.04.00, Keys 02.00.01.00, Dial 01.00.04.00, Knobs
  01.00.05.00, Rollers 01.00.03.01.
- Tools: `cmh monitor` built in release mode from `b426b2b` (before) and
  from this change (after), Rust 1.97.1; a throwaway program outside this
  repository that drives a `Deck` (built twice, against each source tree);
  `/proc` for CPU and context switches.

## Why

A host that embeds the SDK (Dialloom) measured 1.04 % of one core for
`cmh monitor` with five modules open and nothing touched. Two causes were
found there: the once-a-second rescan (`hid_enumerate` of the hidraw backend
walks sysfs with libudev, 6.7 ms of CPU each) and the pass over the modules
every 2 ms in `Deck::poll` (482 wake-ups a second). The base's heartbeat,
two reports a second, is all the traffic there is at rest.

## What was changed

- **Discovery by announcement (Linux).** A thread blocks in `poll(2)` on an
  inotify descriptor watching `/dev` for `IN_CREATE`, `IN_DELETE`,
  `IN_ATTRIB` and the move events of `hidraw*`; it tells the deck, which
  reads the device list 25 ms later (a plugged module is several nodes, each
  changing twice) and then looks at what changed. The list is also read at
  the start and as a safety net every 60 s. `rescan_interval` now sets the
  safety interval where the system announces and the polling interval
  where it does not (default 1 s). The only new dependency is `libc`, which
  `hidapi` already pulls in; no new crate enters the build. `/dev` cannot
  be watched in some containers; the deck then rescans on the 1 s timer.
- **Race with udev.** The kernel creates the node as root's; udev gives the
  user access a moment later, which is an attribute change on the node and
  is announced too. An open that fails while the node is still new is
  therefore tried again every 250 ms for two seconds before `OpenFailed` is
  reported. Without events a failed open is as before: reported at once and
  retried by the timer.
- **Reads without a pass over the modules.** Every open module has a reader
  thread that waits in `hid_read_timeout` on a second handle of the device
  and sends each report, stamped with the time it was read, to the deck's
  channel; `Deck::poll` sleeps on that channel together with the
  announcements and the next rescan. The second handle exists because the
  hidapi handle is `Send` but not `Sync` and a read that waits would hold up
  every write on the same handle; the main thread writes on the first one. On
  Linux the two are two file descriptors of the same hidraw node, each with
  a report queue of its own. A reader ends when its read fails, which is how
  a pulled module is found without a rescan, and reports that; the deck
  then waits one second before it looks at the device list, so that a
  module that connects and fails at once cannot be reopened at full speed.
  Reports of one module stay in the order they were read; `DeckEvent::Input`,
  `RawInput` and `Undecodable` carry the time (`at`) at which the reader
  thread got the report, not the time `poll` handed it over.
- **A reader waits at most 2 s per read (250 ms on Windows)** so that it can
  look at a flag set when its slot is dropped; a dropped deck's threads
  therefore end within that time, or at once when the module goes. This costs
  half a wake-up a second per module.
- **`DeckWaker`** (`Deck::waker`): ends the poll that is waiting, or the next
  one, from any thread. A host that waits for something else too (Dialloom's
  commands) has no other way to make `poll` return early.
- **Windows.** Device interface notifications for the HID class
  (`CM_Register_Notification`, no window needed; the callback runs on a
  system pool thread) are written against `windows-sys`, which is already
  a dependency, and compile for `x86_64-pc-windows-msvc`. They are off by
  default (`DeckOptions::device_events`), so the timer rescan stays the
  Windows default. The reader threads and the second handle are used on
  Windows as well.

## Procedure

1. `cmh monitor --seconds 170` (passive: no software mode, nothing written),
   25 s after its start, user plus system time of the process over 120 s
   from `/proc/<pid>/stat`, per thread run time, time slices and voluntary
   context switches from `/proc/<pid>/task/*/{schedstat,status}`; the
   clock tick is 10 ms. Three runs of each binary, in turn.
2. `cmh monitor --raw --heartbeat --seconds 60` with each binary, intervals
   of the base's heartbeat from the printed times.
3. A program that connects through `Deck` and shows 200 pictures on the
   keys (20 ms apart, `poll(20 ms)` in between) and 100 on the dial, then
   returns the displays as the vendor application does at exit (backlight
   30, standalone mode); run alternately against the old and the new source.

## Results

Idle CPU of `cmh monitor`, five modules open, 120 s:

| | CPU time | Share of one core | Wake-ups a second |
|---|---|---|---|
| before (`b426b2b`) | 1230, 1220, 1230 ms | 1.025, 1.017, 1.025 % | 486.8, 486.8, 486.8 |
| after | 30, 40, 40 ms | 0.025, 0.033, 0.033 % | 5.90, 5.90, 5.88 |

After the change nothing in the process runs for more than a few
milliseconds in 120 s. Where the 5.9 a second go: the main thread 1.96 (one
per heartbeat), the base's reader 1.94 (the same report), the four other
readers 0.5 each (their 2 s read timeout), the inotify thread 0. The
process keeps 10 hidraw descriptors open, two per module. An early run of
the monitor that still woke every 100 ms for its own timer measured 11.95
wake-ups a second on the main thread; `cmh monitor` now waits in `poll`
until its `--seconds` are up.

The base's heartbeat, 60 s: 117 reports before and 116 after, in order,
interval 515.6 ms (standard deviation 1.26 ms before, 0.89 ms after, range
513 to 517 ms in both). The first report, which carries the orientation,
produced one `OrientationChanged` in each run.

Output through a `Deck`: every picture was accepted (`set_image` returned),
on the keys (median 1.6 to 2.0 ms before, 1.7 to 1.9 ms after) and on the
dial (mean 4.2 to 5.0 ms before, 4.4 to 4.8 ms after), and the displays were
returned to standalone mode with the backlight at 30 at the end of every
run. What was shown could not be seen. In some runs one call, the tenth, took
98 to 104 ms (once 33 ms), about 320 ms after the first picture. That
happened in the old source as well (4 of 9 runs with a sleep between the
pictures, 0 of 5 with `poll(20 ms)` between them) as in the new (11 of 12
and 4 of 8). It did not happen in any of 11 runs of either source in which
the first picture came 100 ms to 1.5 s after the connect. It is the module
settling after it entered software mode (experiment 0021 found a backlight
level ignored for the first 0.3 s), not the change; why the new source is
hit more often when the pictures start at once is not known.

## Results of tests without hardware

The scripted transport blocks in reads, wakes on pushed reports and ends a
read when the module is unplugged; the deck tests cover discovery on an
announcement and not on a timer, the safety rescan, a reader that ends with
its module and the module coming back, the order and the times of a burst, a
departure that comes after what the module sent last, a waker, open
failures held back and then reported once, and that an idle deck makes at most
two passes in half a second and each reader one read. The watcher itself
is tested against a real inotify watch on a temporary directory (a node
named `hidraw9` created and removed, the thread ending when the deck lets
go). `emits_only_the_verified_command_set` is unchanged: the change sends
nothing new to a module.

## Findings

- A deck with five modules open and nothing touched costs 0.03 % of a core
  on Linux in this virtual machine instead of 1.02 %, and 5.9 instead of 487
  wake-ups a second: CONFIRMED.
- Input reports still arrive in order and with the time they were read
  (the base's heartbeat): CONFIRMED. Presses, turns and fader moves were
  not made; their reports travel the same way, and the decoding is unchanged:
  LIKELY.
- Output through a `Deck` still works with the second handle open and the
  reader waiting: CONFIRMED for pictures on the keys and on the dial
  (accepted by the module; not seen).
- The inotify watch of `/dev` works as a mechanism (test on a temporary
  directory): CONFIRMED. That the kernel reports `hidraw` nodes arriving and
  leaving, and that udev's change of access is an `IN_ATTRIB` there, is
  how inotify and udev are documented to work and was not seen on a real
  arrival: LIKELY. If the access change were not announced, the open
  retries (two seconds) and the safety rescan (a minute) are the net.
- A re-plug of a module, a module pulled while the deck runs, and the speed
  at which an arrival is then noticed (expected: 25 ms plus one
  enumeration): UNKNOWN. Nobody could re-plug a module for this note.
- Several units of one model, and the Faders on the new path: UNKNOWN (none
  attached).
- Windows: UNKNOWN for all of it: the device notifications (they compile
  and are off by default), a second handle on a module opened by the same
  process, reader threads with a 250 ms read, and the end of a read when a
  module is pulled. That two handles of one module can be open at once is
  LIKELY, because the vendor application and the SDK read the same module
  side by side (README). To check on Windows: `cmh monitor --raw` with
  the base's heartbeat and a press, `DeckOptions::device_events(true)` with
  a module pulled and re-seated, and idle CPU with the Task Manager's
  per-thread view.

## Follow-up after review (same day)

A review of the change found six small defects, three of them in the SDK.
They are fixed on top of the commit this note belongs to; what the text
above says about discovery and readers holds with these changes.

- **A module that does not open is retried every second, not every minute.**
  A module that is listed and failed to open (after the two seconds of
  hold-back), or that was opened and then missing from the list taken
  straight after (`relisted`), used to wait for an announcement or the
  60 s safety rescan. While any such module exists the deck now reads the
  device list at the polling interval (1 s, or a shorter `rescan_interval`)
  until the module is open or no longer listed, then goes back to sleeping.
  Without device events nothing changes: the timer was the retry already. The
  price is what a rescan costs (6.7 ms of CPU, about 0.7 % of a core at one a
  second) for as long as a module stays listed and unopenable, for example
  without the udev rule; this is what the SDK cost before this note.
- **A failed read does not end the reader at once.** hidapi 2.6.7 reads the
  hidraw node in `hid_read_timeout` with `poll(2)`, and when that is
  interrupted by a signal (EINTR) it returns -1, which the SDK took for a
  pulled module. The error text is `strerror(errno)`, so it is exposed
  ("Interrupted system call" through `HidError`'s display), but it depends
  on the locale and older hidapi versions return -1 without a text, so the
  reader does not read it: after a failed read it reads once more without
  waiting and ends only if that fails too. For a module that is gone that
  read fails as well (`poll` reports `POLLERR` or `POLLHUP`, which hidapi
  turns into -1). Both facts are from the source of hidapi 2.6.7
  (`etc/hidapi/linux/hid.c`): CONFIRMED as to what the code does; that a real
  signal interrupts a real read was not provoked: LIKELY. The testing
  transport fails one read of a reader (`Mock::glitch`).
- **A dead watch is noticed.** The inotify thread now counts `IN_IGNORED` and
  `IN_UNMOUNT` (the kernel removed the watch: `/dev` was unmounted or
  removed) as a change, ends on them, and when it ends for any reason (that,
  a failed `read` or `poll`, or the deck letting go) calls a second callback
  that the deck turns into a message. The deck then drops the watch, reads
  the device list at once for what went unseen and rescans at the polling
  interval from then on (an explicit `rescan_interval` still wins). Tested
  against a real inotify watch on a temporary directory that is removed
  (`IN_IGNORED`): CONFIRMED. An unmount of `/dev` and the loss of events
  by an inotify queue overflow (counted as a change already) were not
  provoked: UNKNOWN. On Windows the notification has no thread of its own and
  never ends by itself.
- **Tests without hardware.** The scripted deck covers a failed open that
  becomes possible with no announcement, a module dropped on a missing relist
  that then connects, the return to sleeping once nothing is pending, a watch
  that ends, and a read that fails once. The idle test
  (`an_idle_deck_sleeps_instead_of_polling`) is unchanged and passes. The test that
  pinned "only an announcement or the safety rescan tries again" now pins the
  one-second retry.

