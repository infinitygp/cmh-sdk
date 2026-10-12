use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use cmh_sdk::transport::{self, HidInterface, RawDevice};

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
    /// Passively print raw input reports (never writes to the device).
    Sniff {
        /// Only listen to modules whose serial contains this text.
        #[arg(long)]
        serial: Option<String>,
        /// Stop after this many seconds.
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::List { descriptors } => list(descriptors),
        Command::Sniff { serial, seconds } => {
            sniff(serial.as_deref(), Duration::from_secs(seconds))
        }
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

fn list(descriptors: bool) -> cmh_sdk::Result<()> {
    let interfaces = transport::enumerate()?;
    if interfaces.is_empty() {
        println!("no MasterHUB modules found");
        return Ok(());
    }
    for interface in &interfaces {
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
        if descriptors {
            match RawDevice::open(interface).and_then(|d| d.report_descriptor()) {
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

fn sniff(serial: Option<&str>, duration: Duration) -> cmh_sdk::Result<()> {
    let interfaces: Vec<HidInterface> = transport::enumerate()?
        .into_iter()
        .filter(|i| match serial {
            Some(wanted) => i.serial.as_deref().is_some_and(|s| s.contains(wanted)),
            None => true,
        })
        .collect();
    let mut devices = Vec::new();
    for interface in &interfaces {
        match RawDevice::open(interface) {
            Ok(device) => devices.push((label(interface), device)),
            Err(err) => eprintln!("cannot open {}: {err}", label(interface)),
        }
    }
    if devices.is_empty() {
        return Err(cmh_sdk::Error::NotFound(
            serial.unwrap_or("any module").to_owned(),
        ));
    }
    eprintln!(
        "listening on {} interface(s) for {duration:?}",
        devices.len()
    );

    let start = Instant::now();
    let mut buf = [0u8; 1024];
    while start.elapsed() < duration {
        for (name, device) in &devices {
            let len = device.read_input(&mut buf, Duration::from_millis(5))?;
            if len > 0 {
                println!(
                    "{:9.3} {name} [{len}] {}",
                    start.elapsed().as_secs_f64(),
                    hex(&buf[..len])
                );
            }
        }
    }
    Ok(())
}
