use std::ffi::CString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use cmh_sdk::fixture::{Direction, Fixture, Record};
use cmh_sdk::protocol::InputMessage;
use cmh_sdk::{Error, HidInterface, InputDecoder, LedMode, Manager, Module, ModuleKind, Result};

#[derive(Parser)]
#[command(name = "cmh", about = "Cooler Master MasterHUB diagnostics", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected modules and their HID interfaces.
    List {
        /// Also dump the HID report descriptor of every interface.
        #[arg(long)]
        descriptors: bool,
    },
    /// Show capabilities, firmware version and device info (read only).
    Info {
        #[command(flatten)]
        select: Select,
    },
    /// Print decoded input events live; follows hot-plug.
    Monitor {
        #[command(flatten)]
        select: Select,
        /// Also print every raw report in fixture format.
        #[arg(long)]
        raw: bool,
        /// Include the periodic base heartbeat in --raw output.
        #[arg(long)]
        heartbeat: bool,
        /// Stop after this many seconds (default: run until interrupted).
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Decode a recorded fixture file without any hardware.
    Replay { file: PathBuf },
    /// Switch a display module to application control or back.
    SoftwareMode {
        #[command(flatten)]
        select: Select,
        state: Switch,
    },
    /// Set the backlight of the keys or of the dial display.
    Brightness {
        #[command(flatten)]
        select: Select,
        /// 0..100.
        level: u8,
    },
    /// Set LED effect, colour and brightness (not stored by the module).
    Led {
        #[command(flatten)]
        select: Select,
        /// LED zone; all zones of the module when omitted.
        #[arg(long)]
        index: Option<u8>,
        #[arg(long, value_enum)]
        mode: Option<Effect>,
        /// Colour as RRGGBB.
        #[arg(long, value_parser = parse_rgb)]
        color: Option<(u8, u8, u8)>,
        /// 0..100.
        #[arg(long)]
        brightness: Option<u8>,
    },
    /// Show a JPEG on a key or on the dial (not stored by the module).
    Image {
        #[command(flatten)]
        select: Select,
        /// Key index 0..14 on the 15 LCD Keys.
        #[arg(long, default_value_t = 0)]
        key: u8,
        /// Baseline 4:4:4 JPEG of the exact display size (72x72 or 240x240).
        file: PathBuf,
    },
}

#[derive(Args)]
struct Select {
    /// Only modules of this model.
    #[arg(long, value_enum)]
    module: Option<Model>,
    /// Only modules whose serial number contains this text.
    #[arg(long)]
    serial: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Model {
    Base,
    Keys,
    Faders,
    Knobs,
    Rollers,
    Dial,
}

#[derive(Clone, Copy, ValueEnum)]
enum Switch {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum Effect {
    Off,
    Static,
    Breathing,
    Strobe,
    Cycling,
    Random,
    Wave,
    Water,
    Rainbow,
}

fn parse_rgb(text: &str) -> std::result::Result<(u8, u8, u8), String> {
    let text = text.trim_start_matches('#');
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected RRGGBB".to_owned());
    }
    let channel = |i: usize| u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| e.to_string());
    Ok((channel(0)?, channel(2)?, channel(4)?))
}

impl Select {
    fn matches(&self, interface: &HidInterface) -> bool {
        let kind = self.module.map(|m| match m {
            Model::Base => ModuleKind::Base,
            Model::Keys => ModuleKind::Keys15,
            Model::Faders => ModuleKind::Faders5,
            Model::Knobs => ModuleKind::Knobs3,
            Model::Rollers => ModuleKind::Rollers2,
            Model::Dial => ModuleKind::DialDisplay,
        });
        kind.is_none_or(|k| k == interface.kind)
            && self.serial.as_deref().is_none_or(|wanted| {
                interface
                    .serial
                    .as_deref()
                    .is_some_and(|s| s.contains(wanted))
            })
    }

    fn all(&self, manager: &Manager) -> Vec<HidInterface> {
        manager
            .modules()
            .iter()
            .filter(|i| self.matches(i))
            .cloned()
            .collect()
    }

    /// Resolves the selection to exactly one module.
    fn one(&self, manager: &Manager) -> Result<Module> {
        match self.all(manager).as_slice() {
            [interface] => manager.open_interface(interface),
            [] => Err(Error::NotFound("the selection".to_owned())),
            many => Err(Error::InvalidArgument(format!(
                "selection matches {} modules; narrow it with --module or --serial",
                many.len()
            ))),
        }
    }
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::List { descriptors } => list(descriptors),
        Command::Info { select } => info(&select),
        Command::Monitor {
            select,
            raw,
            heartbeat,
            seconds,
        } => monitor(&select, raw, heartbeat, seconds.map(Duration::from_secs)),
        Command::Replay { file } => replay(&file),
        Command::SoftwareMode { select, state } => software_mode(&select, state),
        Command::Image { select, key, file } => image(&select, key, &file),
        Command::Brightness { select, level } => brightness(&select, level),
        Command::Led {
            select,
            index,
            mode,
            color,
            brightness,
        } => led(&select, index, mode, color, brightness),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn label(interface: &HidInterface) -> String {
    format!(
        "{:<12} {:04x}:{:04x} serial={}",
        interface.kind.name(),
        cmh_sdk::VENDOR_ID,
        interface.product_id,
        interface.serial.as_deref().unwrap_or("?"),
    )
}

fn dotted(path: Option<Vec<u8>>) -> String {
    match path {
        Some(path) => path.iter().map(u8::to_string).collect::<Vec<_>>().join("."),
        None => "?".to_owned(),
    }
}

fn connected() -> Result<Manager> {
    let mut manager = Manager::new()?;
    manager.refresh()?;
    Ok(manager)
}

fn list(descriptors: bool) -> Result<()> {
    let manager = connected()?;
    if manager.modules().is_empty() {
        println!("no MasterHUB modules found");
        return Ok(());
    }
    for interface in manager.modules() {
        println!("{}", label(interface));
        println!(
            "    product={:?} manufacturer={:?} bcdDevice={:04x} interface={} usage_page={:04x} usage={:04x}",
            interface.product.as_deref().unwrap_or(""),
            interface.manufacturer.as_deref().unwrap_or(""),
            interface.release_number,
            interface.interface_number,
            interface.usage_page,
            interface.usage,
        );
        println!("    path={}", interface.path.to_string_lossy());
        println!(
            "    usb_port_chain={} mount_path={}",
            dotted(interface.port_chain.clone()),
            dotted(manager.mount_path(interface))
        );
        if descriptors {
            let descriptor = manager
                .open_interface(interface)
                .and_then(|m| m.report_descriptor());
            match descriptor {
                Ok(descriptor) => println!(
                    "    report_descriptor[{}]={}",
                    descriptor.len(),
                    hex(&descriptor)
                ),
                Err(err) => println!("    report_descriptor: <{err}>"),
            }
        }
    }
    Ok(())
}

fn info(select: &Select) -> Result<()> {
    let manager = connected()?;
    let interfaces = select.all(&manager);
    if interfaces.is_empty() {
        return Err(Error::NotFound("the selection".to_owned()));
    }
    for interface in &interfaces {
        println!("{}", label(interface));
        let caps = interface.kind.capabilities();
        print!(
            "    capabilities: {} button(s), {} encoder(s), {} fader(s), {} LED zone(s)",
            caps.buttons, caps.encoders, caps.faders, caps.leds
        );
        match caps.display {
            Some(d) => println!(", {} image(s) of {}x{}", d.images, d.width, d.height),
            None => println!(", no display"),
        }
        let mut module = match manager.open_interface(interface) {
            Ok(module) => module,
            Err(err) => {
                println!("    <cannot open: {err}>");
                continue;
            }
        };
        match module.firmware() {
            Ok(fw) => println!(
                "    firmware: {} (build tag {})",
                fw.version,
                hex(&fw.build_tag)
            ),
            Err(err) => println!("    firmware: <{err}>"),
        }
        match module.device_info() {
            Ok(raw) => println!("    device info: {}", hex(&raw)),
            Err(err) => println!("    device info: <{err}>"),
        }
        if caps.display.is_some() {
            match module.display_info() {
                Ok(d) => println!(
                    "    display: images {}x{}, panel {}x{}, grid {}x{} ({} keys)",
                    d.image_width,
                    d.image_height,
                    d.panel_width,
                    d.panel_height,
                    d.columns,
                    d.rows,
                    d.keys
                ),
                Err(err) => println!("    display: <{err}>"),
            }
        }
        if caps.faders > 0 {
            match module.fader_positions() {
                Ok(positions) => println!("    fader positions: {positions:?}"),
                Err(err) => println!("    fader positions: <{err}>"),
            }
        }
    }
    Ok(())
}

/// Identity of a module for as long as it stays plugged in.
type Key = (Option<String>, CString);

fn key(interface: &HidInterface) -> Key {
    (interface.serial.clone(), interface.path.clone())
}

/// Decides which open handles to close and which listed modules to open so
/// that the open set matches what is currently connected. Modules that
/// failed to open or were dropped after an I/O error are simply absent from
/// `open` and therefore retried on every pass.
fn reconcile(open: &[Key], listed: &[Key]) -> (Vec<Key>, Vec<Key>) {
    let close = open
        .iter()
        .filter(|k| !listed.contains(k))
        .cloned()
        .collect();
    let to_open = listed
        .iter()
        .filter(|k| !open.contains(k))
        .cloned()
        .collect();
    (close, to_open)
}

fn monitor(select: &Select, raw: bool, heartbeat: bool, limit: Option<Duration>) -> Result<()> {
    const RESCAN: Duration = Duration::from_secs(1);

    let mut manager = Manager::new()?;
    let mut open: Vec<(HidInterface, Module)> = Vec::new();
    // Modules whose open error was already reported, to avoid repeating it
    // on every rescan.
    let mut complained: Vec<Key> = Vec::new();
    let start = Instant::now();
    let mut last_scan: Option<Instant> = None;
    let stamp = |start: Instant| start.elapsed().as_secs_f64();

    while limit.is_none_or(|l| start.elapsed() < l) {
        if last_scan.is_none_or(|t| t.elapsed() >= RESCAN) {
            last_scan = Some(Instant::now());
            manager.refresh()?;
            let listed = select.all(&manager);
            let listed_keys: Vec<Key> = listed.iter().map(key).collect();
            let open_keys: Vec<Key> = open.iter().map(|(i, _)| key(i)).collect();
            let (close, to_open) = reconcile(&open_keys, &listed_keys);

            open.retain(|(interface, _)| {
                let gone = close.contains(&key(interface));
                if gone {
                    println!("# {:9.3} left    {}", stamp(start), label(interface));
                }
                !gone
            });
            complained.retain(|k| listed_keys.contains(k));
            for interface in listed.iter().filter(|i| to_open.contains(&key(i))) {
                match manager.open_interface(interface) {
                    Ok(mut module) => {
                        println!(
                            "# {:9.3} arrived {} mount_path={}",
                            stamp(start),
                            label(interface),
                            dotted(manager.mount_path(interface))
                        );
                        if interface.kind.capabilities().faders > 0 {
                            match module.fader_positions() {
                                Ok(p) => println!("# {:9.3} fader positions {p:?}", stamp(start)),
                                Err(err) => {
                                    println!("# {:9.3} fader positions: {err}", stamp(start))
                                }
                            }
                        }
                        complained.retain(|k| *k != key(interface));
                        open.push((interface.clone(), module));
                    }
                    Err(err) => {
                        if !complained.contains(&key(interface)) {
                            eprintln!("cannot open {}: {err} (will retry)", label(interface));
                            complained.push(key(interface));
                        }
                    }
                }
            }
        }
        if open.is_empty() {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let mut failed = None;
        for (i, (interface, module)) in open.iter_mut().enumerate() {
            let report = match module.read_raw(Duration::from_millis(5)) {
                Ok(Some(report)) => report,
                Ok(None) => continue,
                Err(err) => {
                    println!(
                        "# {:9.3} lost    {} ({err})",
                        stamp(start),
                        label(interface)
                    );
                    failed = Some(i);
                    break;
                }
            };
            let is_heartbeat = matches!(
                InputMessage::parse(interface.kind, &report),
                Ok(InputMessage::Heartbeat { .. })
            );
            if raw && (heartbeat || !is_heartbeat) {
                let record = Record::new(stamp(start), interface.kind, Direction::In, &report);
                println!("{record}");
            }
            match module.decode(&report) {
                Ok(events) => {
                    for event in events {
                        println!(
                            "# {:9.3} {:<12} {event:?}",
                            stamp(start),
                            interface.kind.name()
                        );
                    }
                }
                Err(err) => println!(
                    "# {:9.3} {:<12} undecodable: {err}",
                    stamp(start),
                    interface.kind.name()
                ),
            }
        }
        if let Some(i) = failed {
            // The next rescan reopens it if it is still (or again) listed.
            open.remove(i);
        }
    }
    Ok(())
}

fn replay(file: &PathBuf) -> Result<()> {
    let text = std::fs::read_to_string(file)
        .map_err(|err| Error::InvalidArgument(format!("{}: {err}", file.display())))?;
    let fixture = Fixture::parse(&text)?;
    let mut decoders: Vec<InputDecoder> = Vec::new();
    for record in fixture.records() {
        println!("{record}");
        if record.direction != Direction::In {
            continue;
        }
        let index = match decoders.iter().position(|d| d.kind() == record.kind) {
            Some(index) => index,
            None => {
                decoders.push(InputDecoder::new(record.kind));
                decoders.len() - 1
            }
        };
        match decoders[index].decode(&record.report()) {
            Ok(events) => {
                for event in events {
                    println!(
                        "# {:9.3} {:<12} {event:?}",
                        record.seconds,
                        record.kind.name()
                    );
                }
            }
            Err(err) => println!("# {:9.3} undecodable: {err}", record.seconds),
        }
    }
    Ok(())
}

fn software_mode(select: &Select, state: Switch) -> Result<()> {
    let module = select.one(&connected()?)?;
    let enabled = matches!(state, Switch::On);
    module.set_software_mode(enabled)?;
    println!(
        "{}: software mode {}",
        module.kind(),
        if enabled { "on" } else { "off" }
    );
    Ok(())
}

fn brightness(select: &Select, level: u8) -> Result<()> {
    let module = select.one(&connected()?)?;
    module.set_display_brightness(level)?;
    println!("{}: display brightness {level}", module.kind());
    Ok(())
}

fn led(
    select: &Select,
    index: Option<u8>,
    mode: Option<Effect>,
    color: Option<(u8, u8, u8)>,
    brightness: Option<u8>,
) -> Result<()> {
    let module = select.one(&connected()?)?;
    let zones = match index {
        Some(index) => index..=index,
        None => 0..=module.kind().capabilities().leds.saturating_sub(1),
    };
    for zone in zones {
        if let Some((r, g, b)) = color {
            module.set_led_color(zone, r, g, b)?;
        }
        if let Some(mode) = mode {
            module.set_led_mode(
                zone,
                match mode {
                    Effect::Off => LedMode::Off,
                    Effect::Static => LedMode::Static,
                    Effect::Breathing => LedMode::Breathing,
                    Effect::Strobe => LedMode::Strobe,
                    Effect::Cycling => LedMode::Cycling,
                    Effect::Random => LedMode::Random,
                    Effect::Wave => LedMode::Wave,
                    Effect::Water => LedMode::Water,
                    Effect::Rainbow => LedMode::Rainbow,
                },
            )?;
        }
        if let Some(level) = brightness {
            module.set_led_brightness(zone, level)?;
        }
    }
    println!("{}: LED settings sent", module.kind());
    Ok(())
}

fn image(select: &Select, key: u8, file: &PathBuf) -> Result<()> {
    let jpeg = std::fs::read(file)
        .map_err(|err| Error::InvalidArgument(format!("{}: {err}", file.display())))?;
    let module = select.one(&connected()?)?;
    // Validate before touching the module so that a rejected file leaves it
    // exactly as it was.
    module.check_image(key, &jpeg)?;
    module.set_software_mode(true)?;
    module.set_image(key, &jpeg)?;
    println!(
        "{}: sent {} bytes to image {key}",
        module.kind(),
        jpeg.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(serial: &str, path: &str) -> Key {
        (Some(serial.to_owned()), CString::new(path).unwrap())
    }

    #[test]
    fn opens_new_and_closes_departed_modules() {
        let open = [k("A", "p1"), k("B", "p2")];
        let listed = [k("B", "p2"), k("C", "p3")];
        assert_eq!(
            reconcile(&open, &listed),
            (vec![k("A", "p1")], vec![k("C", "p3")])
        );
    }

    #[test]
    fn retries_modules_that_are_listed_but_not_open() {
        // A failed open or an I/O error leaves the module listed but absent
        // from the open set; every pass must ask for it again.
        let listed = [k("A", "p1")];
        assert_eq!(reconcile(&[], &listed), (vec![], vec![k("A", "p1")]));
        assert_eq!(reconcile(&listed, &listed), (vec![], vec![]));
    }

    #[test]
    fn a_module_under_a_new_path_is_reopened() {
        assert_eq!(
            reconcile(&[k("A", "p1")], &[k("A", "p9")]),
            (vec![k("A", "p1")], vec![k("A", "p9")])
        );
    }
}
