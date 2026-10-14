use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use cmh_sdk::fixture::{Direction, Fixture, Record};
use cmh_sdk::protocol::InputMessage;
use cmh_sdk::protocol::feature::display_brightness_report;
use cmh_sdk::{
    Deck, DeckEvent, DeckOptions, Error, HidInterface, InputDecoder, LedMode, Manager, Module,
    ModuleKind, Placement, Result,
};

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
    /// Show capabilities, firmware version and device info (read only
    /// unless --software-mode is given).
    Info {
        #[command(flatten)]
        select: Select,
        /// Switch the keys and the dial to software mode first. They report
        /// their firmware version and backlight only in that mode; it
        /// replaces what they show with a black screen.
        #[arg(long)]
        software_mode: bool,
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
    /// Set the backlight of the keys or of the dial display. Switches the
    /// module to software mode, which the backlight needs; a module that
    /// was in standalone mode goes black.
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
        /// PNG or JPEG. A JPEG that already has the exact format the module
        /// needs is sent unchanged; anything else is converted.
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
        Command::Info {
            select,
            software_mode,
        } => info(&select, software_mode),
        Command::Monitor {
            select,
            raw,
            heartbeat,
            seconds,
        } => monitor(select, raw, heartbeat, seconds.map(Duration::from_secs)),
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

fn cells(placement: Option<Placement>) -> String {
    match placement {
        Some(p) => format!(
            "columns {}-{} rows {}-{} turned {}",
            p.column,
            p.last_column(),
            p.row,
            p.last_row(),
            p.rotation.degrees()
        ),
        None => "?".to_owned(),
    }
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
        if interface.kind != ModuleKind::Base {
            println!("    placement: {}", cells(manager.placement(interface)));
        }
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

fn info(select: &Select, software_mode: bool) -> Result<()> {
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
        if caps.display.is_some()
            && software_mode
            && let Err(err) = module.set_software_mode(true)
        {
            println!("    software mode: <{err}>");
        }
        match module.firmware() {
            Ok(fw) => println!(
                "    firmware: {} (build tag {})",
                fw.version,
                hex(&fw.build_tag)
            ),
            Err(Error::NotInSoftwareMode { .. }) if !software_mode => {
                println!("    firmware: <not asked; needs --software-mode>")
            }
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
        if caps.leds > 0 {
            match module.led_brightness() {
                Ok(levels) => println!("    LED brightness: {levels:?}"),
                Err(err) => println!("    LED brightness: <{err}>"),
            }
        }
        if caps.display.is_some() {
            match module.display_brightness() {
                Ok(level) => println!("    display brightness: {level}"),
                Err(Error::NotInSoftwareMode { .. }) if !software_mode => {
                    println!("    display brightness: <not asked; needs --software-mode>")
                }
                Err(err) => println!("    display brightness: <{err}>"),
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

fn monitor(select: Select, raw: bool, heartbeat: bool, limit: Option<Duration>) -> Result<()> {
    // Diagnostics stay passive: nothing is written to a module.
    let options = DeckOptions::default()
        .software_mode(false)
        .raw_reports(raw)
        .filter(move |interface| select.matches(interface));
    let mut deck = Deck::with_options(options)?;
    let start = Instant::now();
    let stamp = |start: Instant| start.elapsed().as_secs_f64();

    while limit.is_none_or(|l| start.elapsed() < l) {
        for event in deck.poll(Duration::from_millis(100))? {
            let t = stamp(start);
            match event {
                DeckEvent::Connected(info) => println!(
                    "# {t:9.3} arrived {} mount_path={} placement: {}",
                    label(&info.interface),
                    dotted(info.mount_path),
                    cells(info.placement)
                ),
                DeckEvent::Disconnected(info) => {
                    println!("# {t:9.3} left    {}", label(&info.interface))
                }
                DeckEvent::Relocated(info) => println!(
                    "# {t:9.3} placed  {} mount_path={} placement: {}",
                    label(&info.interface),
                    dotted(info.mount_path),
                    cells(info.placement)
                ),
                DeckEvent::RawInput { kind, report, .. } => {
                    let is_heartbeat = matches!(
                        InputMessage::parse(kind, &report),
                        Ok(InputMessage::Heartbeat { .. })
                    );
                    if heartbeat || !is_heartbeat {
                        println!("{}", Record::new(t, kind, Direction::In, &report));
                    }
                }
                DeckEvent::Input { kind, event, .. } => {
                    println!("# {t:9.3} {:<12} {event:?}", kind.name())
                }
                DeckEvent::Undecodable { kind, error, .. } => {
                    println!("# {t:9.3} {:<12} undecodable: {error}", kind.name())
                }
                DeckEvent::OpenFailed {
                    interface, error, ..
                } => {
                    eprintln!("cannot open {}: {error} (will retry)", label(&interface))
                }
                _ => {}
            }
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
    // Validate before touching the module; the backlight can only be set
    // in software mode.
    display_brightness_report(module.kind(), level)?;
    module.set_software_mode(true)?;
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
    let data = std::fs::read(file)
        .map_err(|err| Error::InvalidArgument(format!("{}: {err}", file.display())))?;
    let module = select.one(&connected()?)?;
    let Some(display) = module.kind().capabilities().display else {
        return Err(Error::Unsupported {
            module: module.kind().name(),
            capability: "images",
        });
    };
    let jpeg = if module.check_image(key, &data).is_ok() {
        data
    } else {
        cmh_sdk::picture::convert(display, &data)?
    };
    // Validate before touching the module so that a rejected request leaves
    // it exactly as it was.
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
