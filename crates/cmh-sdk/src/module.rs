//! An open MasterHUB module.

use std::cell::Cell;
use std::time::Duration;

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::input::{InputDecoder, InputEvent};
use crate::protocol::feature::{
    DISPLAY_BRIGHTNESS_MAX, DisplayInfo, FADER_POSITIONS_REPORT_ID, FEATURE_REPORT_LEN,
    FirmwareInfo, LED_BRIGHTNESS_REPORT_ID, device_info_report_id, display_brightness_report,
    display_brightness_report_id, feature_payload, firmware_report_id, parse_fader_positions,
    parse_zone_values, software_mode_report,
};
use crate::protocol::frame::INPUT_REPORT_LEN;
use crate::protocol::image::image_reports;
use crate::protocol::led::{LED_BRIGHTNESS_MAX, LedCommand, LedMode, led_report};
use crate::transport::Transport;

/// Typed access to one open module.
///
/// Obtained from [`Manager::open`](crate::Manager::open). A module offers a
/// closed set of verified operations; there is intentionally no way to send
/// an arbitrary report:
///
/// ```compile_fail
/// fn raw(module: &cmh_sdk::Module) {
///     module.transport().write_output(&[0x02, 0x2a]);
/// }
/// ```
///
/// A `Module` is `Send` but not `Sync`: keep it on one thread and use short
/// read timeouts when the same thread also has to draw images.
pub struct Module {
    kind: ModuleKind,
    serial: Option<String>,
    transport: Box<dyn Transport>,
    decoder: InputDecoder,
    /// Whether this handle switched a display module to software mode and
    /// saw it answer there.
    software_mode: Cell<bool>,
}

/// How often a backlight level is sent before giving up, and the pause
/// between attempts. A display module ignores the level for about 0.3 s
/// after it entered software mode (experiment 0021).
const BRIGHTNESS_ATTEMPTS: usize = 8;
const BRIGHTNESS_RETRY_PAUSE: Duration = Duration::from_millis(60);

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("kind", &self.kind)
            .field("serial", &self.serial)
            .finish_non_exhaustive()
    }
}

impl Module {
    pub(crate) fn new(
        kind: ModuleKind,
        serial: Option<String>,
        transport: Box<dyn Transport>,
    ) -> Self {
        Self {
            kind,
            serial,
            transport,
            decoder: InputDecoder::new(kind),
            software_mode: Cell::new(false),
        }
    }

    pub fn kind(&self) -> ModuleKind {
        self.kind
    }

    /// USB serial number, the stable identity of this module.
    pub fn serial(&self) -> Option<&str> {
        self.serial.as_deref()
    }

    /// Reads the HID report descriptor (reconstructed by the OS on Windows).
    pub fn report_descriptor(&self) -> Result<Vec<u8>> {
        self.transport.report_descriptor()
    }

    /// Waits up to `timeout` for one raw input report.
    ///
    /// Takes `&mut self` because reading consumes input: a module shared
    /// through [`Deck::module`](crate::Deck::module) cannot have its reports
    /// taken away from the deck.
    pub fn read_raw(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; INPUT_REPORT_LEN];
        let len = self.transport.read_input(&mut buf, timeout)?;
        Ok((len > 0).then(|| buf[..len.min(INPUT_REPORT_LEN)].to_vec()))
    }

    /// Decodes a raw input report into events, tracking control state.
    pub fn decode(&mut self, report: &[u8]) -> Result<Vec<InputEvent>> {
        self.decoder.decode(report)
    }

    /// Waits up to `timeout` for input and returns the resulting events.
    ///
    /// An empty list means the timeout elapsed or the report carried no
    /// event (for example the base heartbeat).
    /// [`Error::MalformedReport`] is not fatal; [`Error::Disconnected`] is.
    pub fn read_events(&mut self, timeout: Duration) -> Result<Vec<InputEvent>> {
        match self.read_raw(timeout)? {
            Some(report) => self.decode(&report),
            None => Ok(Vec::new()),
        }
    }

    fn read_feature(&self, id: u8) -> Result<Vec<u8>> {
        let mut buf = [0u8; FEATURE_REPORT_LEN];
        buf[0] = id;
        // Some backends return fewer bytes than requested; never parse the
        // untouched tail of the buffer as data.
        let len = self.transport.get_feature(&mut buf)?;
        Ok(buf[..len.min(FEATURE_REPORT_LEN)].to_vec())
    }

    /// Display modules answer some reads only in software mode. Asking in
    /// standalone mode returns nothing and makes the module carry out later
    /// feature writes one write late (experiment 0021), so the question is
    /// not put unless this handle enabled software mode.
    fn require_software_mode(&self) -> Result<()> {
        if self.kind.capabilities().display.is_some() && !self.software_mode.get() {
            return Err(self.not_in_software_mode());
        }
        Ok(())
    }

    fn not_in_software_mode(&self) -> Error {
        Error::NotInSoftwareMode {
            module: self.kind.name(),
        }
    }

    /// Reads a feature report that a display module leaves empty in
    /// standalone mode.
    fn read_software_mode_feature(&self, id: u8) -> Result<Vec<u8>> {
        let report = self.read_feature(id)?;
        if report.is_empty() && self.kind.capabilities().display.is_some() {
            // Something else switched the module back.
            self.software_mode.set(false);
            return Err(self.not_in_software_mode());
        }
        Ok(report)
    }

    /// Reads the running firmware version.
    ///
    /// A display module answers only in software mode: enable it with
    /// [`set_software_mode`](Self::set_software_mode) on this handle first.
    /// Without that [`Error::NotInSoftwareMode`] is returned and the module
    /// is not asked.
    pub fn firmware(&self) -> Result<FirmwareInfo> {
        self.require_software_mode()?;
        let id = firmware_report_id(self.kind);
        FirmwareInfo::parse(feature_payload(&self.read_software_mode_feature(id)?, id)?)
    }

    /// Reads the raw device info payload.
    pub fn device_info(&self) -> Result<Vec<u8>> {
        let id = device_info_report_id(self.kind);
        Ok(feature_payload(&self.read_feature(id)?, id)?.to_vec())
    }

    /// Reads the display geometry of a module that has a screen.
    pub fn display_info(&self) -> Result<DisplayInfo> {
        if self.kind.capabilities().display.is_none() {
            return Err(Error::Unsupported {
                module: self.kind.name(),
                capability: "a display",
            });
        }
        DisplayInfo::parse(&self.device_info()?)
    }

    /// Reads the current position (`0..=100`) of every fader.
    ///
    /// Faders only send input when they move, so call this once after
    /// opening the module to learn where they are. Later
    /// [`read_events`](Self::read_events) calls then report changes only.
    pub fn fader_positions(&mut self) -> Result<[u8; 5]> {
        if self.kind.capabilities().faders == 0 {
            return Err(Error::Unsupported {
                module: self.kind.name(),
                capability: "faders",
            });
        }
        let id = FADER_POSITIONS_REPORT_ID;
        let positions = parse_fader_positions(feature_payload(&self.read_feature(id)?, id)?)?;
        self.decoder.set_fader_positions(&positions);
        Ok(positions)
    }

    fn set_led(&self, index: u8, command: LedCommand) -> Result<()> {
        self.transport
            .write_output(&led_report(self.kind, index, command)?)
    }

    fn led_zones(&self) -> Result<u8> {
        match self.kind.capabilities().leds {
            0 => Err(Error::Unsupported {
                module: self.kind.name(),
                capability: "LED control",
            }),
            zones => Ok(zones),
        }
    }

    /// Reads the current brightness (`0..=100`) of every LED zone.
    ///
    /// The effect and the colour cannot be read back: the module's effect
    /// report keeps its stored value whatever is set at run time.
    pub fn led_brightness(&self) -> Result<Vec<u8>> {
        let zones = self.led_zones()?;
        let id = LED_BRIGHTNESS_REPORT_ID;
        let report = self.read_feature(id)?;
        parse_zone_values(feature_payload(&report, id)?, zones, LED_BRIGHTNESS_MAX)
    }

    /// Reads the current backlight level (`0..=100`) of a display module.
    ///
    /// Needs software mode, like [`firmware`](Self::firmware).
    pub fn display_brightness(&self) -> Result<u8> {
        let Some(id) = display_brightness_report_id(self.kind) else {
            return Err(Error::Unsupported {
                module: self.kind.name(),
                capability: "display brightness",
            });
        };
        self.require_software_mode()?;
        let report = self.read_software_mode_feature(id)?;
        let values = parse_zone_values(feature_payload(&report, id)?, 1, DISPLAY_BRIGHTNESS_MAX)?;
        Ok(values[0])
    }

    /// Selects the effect of LED zone `index`.
    ///
    /// LED settings made through the SDK are not stored by the module.
    pub fn set_led_mode(&self, index: u8, mode: LedMode) -> Result<()> {
        self.set_led(index, LedCommand::Mode(mode))
    }

    /// Sets the colour of LED zone `index`, used by the static, breathing
    /// and strobe effects.
    pub fn set_led_color(&self, index: u8, r: u8, g: u8, b: u8) -> Result<()> {
        self.set_led(index, LedCommand::Color { r, g, b })
    }

    /// Sets the brightness (`0..=100`) of LED zone `index`.
    pub fn set_led_brightness(&self, index: u8, level: u8) -> Result<()> {
        self.set_led(index, LedCommand::Brightness(level))
    }

    /// Hands a display module to the application (`true`) or back to
    /// standalone operation (`false`). The vendor application enables it
    /// before uploading images.
    ///
    /// In standalone mode, which is where a module starts after power-up,
    /// it plays a built-in animation, sends no input, shows no uploaded
    /// image and answers neither [`firmware`](Self::firmware) nor
    /// [`display_brightness`](Self::display_brightness). Enabling is
    /// confirmed by reading the firmware version;
    /// [`Error::NotInSoftwareMode`] means the module did not switch.
    pub fn set_software_mode(&self, enabled: bool) -> Result<()> {
        let report = software_mode_report(self.kind, enabled)?;
        self.software_mode.set(false);
        self.transport.send_feature(&report)?;
        if !enabled {
            return Ok(());
        }
        // A module that was asked for its firmware or backlight while in
        // standalone mode carries out a feature write only when the next
        // one arrives, and drops it if a read arrives first. The second
        // copy makes the first one take effect there; the read that
        // follows returns data and ends that state.
        self.transport.send_feature(&report)?;
        let id = firmware_report_id(self.kind);
        let answer = self.read_feature(id)?;
        if answer.is_empty() {
            return Err(self.not_in_software_mode());
        }
        FirmwareInfo::parse(feature_payload(&answer, id)?)?;
        self.software_mode.set(true);
        Ok(())
    }

    /// Sets the backlight (`0..=100`) of a display module.
    ///
    /// Needs software mode, like [`firmware`](Self::firmware). The level is
    /// read back and sent again until the module reports it, because a
    /// module ignores it for a moment after entering software mode. That
    /// can take up to half a second, after which
    /// [`Error::BrightnessNotApplied`] is returned.
    pub fn set_display_brightness(&self, level: u8) -> Result<()> {
        let report = display_brightness_report(self.kind, level)?;
        self.require_software_mode()?;
        let mut found = level;
        for attempt in 0..BRIGHTNESS_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(BRIGHTNESS_RETRY_PAUSE);
            }
            self.transport.send_feature(&report)?;
            found = self.display_brightness()?;
            if found == level {
                return Ok(());
            }
        }
        Err(Error::BrightnessNotApplied {
            module: self.kind.name(),
            wanted: level,
            found,
        })
    }

    /// Checks that [`set_image`](Self::set_image) would accept `jpeg` for
    /// `target`, without sending anything.
    pub fn check_image(&self, target: u8, jpeg: &[u8]) -> Result<()> {
        image_reports(self.kind, target, jpeg).map(drop)
    }

    /// Converts a PNG or JPEG file to the display's format and shows it on
    /// image `target`. The picture is stretched to the display size.
    #[cfg(feature = "image")]
    pub fn set_picture(&self, target: u8, file: &[u8]) -> Result<()> {
        let Some(display) = self.kind.capabilities().display else {
            return Err(Error::Unsupported {
                module: self.kind.name(),
                capability: "images",
            });
        };
        self.set_image(target, &crate::picture::convert(display, file)?)
    }

    /// Shows a JPEG on image `target` (key index on the 15 LCD Keys, `0` on
    /// the Dial Display).
    ///
    /// The image must be a baseline JPEG of exactly the display size with
    /// three components, stored as 4:4:4, 4:2:2 or 4:2:0. Nothing is sent
    /// if it is not. The module does not store the image.
    pub fn set_image(&self, target: u8, jpeg: &[u8]) -> Result<()> {
        for report in image_reports(self.kind, target, jpeg)? {
            self.transport.write_output(&report)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::image::tests::fake_jpeg;
    use crate::testing::{DIAL_FIRMWARE, KEYS_FIRMWARE, Mock, Sent};

    const DIAL_INFO: &[u8] = &[
        0x4b, 0x0b, 0x14, 0x00, 0x00, 0x00, 0xf0, 0x00, 0xf0, 0x00, 0x01, 0xf0, 0xf0,
    ];

    #[test]
    fn reads_events_and_timeouts() {
        let mock = Mock::default();
        mock.push_input(&[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0xff, 0xff, 0x01]);
        let mut module = mock.module(ModuleKind::Knobs3);
        assert_eq!(
            module.read_events(Duration::ZERO).unwrap(),
            [InputEvent::EncoderDelta { index: 2, delta: 1 }]
        );
        assert!(module.read_events(Duration::ZERO).unwrap().is_empty());
    }

    #[test]
    fn malformed_input_is_not_fatal() {
        let mock = Mock::default();
        mock.push_input(&[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0x07, 0xff, 0xff]);
        mock.push_input(&[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0x01, 0xff, 0xff]);
        let mut module = mock.module(ModuleKind::Knobs3);
        assert!(matches!(
            module.read_events(Duration::ZERO),
            Err(Error::MalformedReport(_))
        ));
        assert_eq!(module.read_events(Duration::ZERO).unwrap().len(), 1);
    }

    #[test]
    fn reads_firmware_and_display_info() {
        let module =
            Mock::with_features(&[DIAL_FIRMWARE, DIAL_INFO]).module(ModuleKind::DialDisplay);
        module.set_software_mode(true).unwrap();
        assert_eq!(module.firmware().unwrap().version, "01.00.04.00");
        assert_eq!(module.display_info().unwrap().image_width, 240);

        let base = Mock::with_features(&[&[0x2e, 0x07, 1, 2, 3, 4, b'2', b'.', b'0']]);
        assert_eq!(
            base.module(ModuleKind::Base).firmware().unwrap().version,
            "2.0"
        );
    }

    fn mode_report(command: u8, enabled: bool) -> Sent {
        let mut report = [0u8; 32];
        report[..3].copy_from_slice(&[0x03, command, u8::from(enabled)]);
        Sent::Feature(report.to_vec())
    }

    #[test]
    fn display_modules_are_not_asked_outside_software_mode() {
        let mock = Mock::with_features(&[KEYS_FIRMWARE, &[0x3c, 0x01, 30], &[0x3b, 0x01, 0x14]]);
        mock.0.lock().unwrap().standalone = true;
        let keys = mock.module(ModuleKind::Keys15);
        for result in [
            keys.firmware().map(drop),
            keys.display_brightness().map(drop),
            keys.set_display_brightness(40),
        ] {
            assert!(matches!(result, Err(Error::NotInSoftwareMode { .. })));
        }
        // The device info is answered in both modes.
        assert_eq!(keys.device_info().unwrap(), [0x14]);
        assert_eq!(mock.sent(), [Sent::FeatureRead(0x3b)]);
    }

    #[test]
    fn software_mode_is_sent_twice_and_confirmed() {
        // The state a module is in after it was asked for its firmware in
        // standalone mode: one write alone would be dropped by the read.
        let mock = Mock::with_features(&[KEYS_FIRMWARE]);
        {
            let mut state = mock.0.lock().unwrap();
            state.standalone = true;
            state.lagging = true;
        }
        let keys = mock.module(ModuleKind::Keys15);
        keys.set_software_mode(true).unwrap();
        assert_eq!(
            mock.sent(),
            [
                mode_report(0x3a, true),
                mode_report(0x3a, true),
                Sent::FeatureRead(0x38)
            ]
        );
        assert_eq!(keys.firmware().unwrap().version, "02.00.01.00");

        keys.set_software_mode(false).unwrap();
        assert_eq!(mock.sent().last(), Some(&mode_report(0x3a, false)));
        assert!(matches!(
            keys.firmware(),
            Err(Error::NotInSoftwareMode { .. })
        ));
    }

    #[test]
    fn a_module_that_does_not_switch_is_reported() {
        let mock = Mock::with_features(&[DIAL_FIRMWARE]);
        {
            let mut state = mock.0.lock().unwrap();
            state.standalone = true;
            state.stuck = true;
        }
        let dial = mock.module(ModuleKind::DialDisplay);
        assert!(matches!(
            dial.set_software_mode(true),
            Err(Error::NotInSoftwareMode { .. })
        ));
        // It is not asked for anything afterwards either.
        let sent = mock.sent().len();
        assert!(dial.firmware().is_err());
        assert_eq!(mock.sent().len(), sent);

        // The dial cannot be sent back, and the attempt changes nothing.
        mock.0.lock().unwrap().stuck = false;
        dial.set_software_mode(true).unwrap();
        assert!(matches!(
            dial.set_software_mode(false),
            Err(Error::Unsupported { .. })
        ));
        assert_eq!(dial.firmware().unwrap().version, "01.00.04.00");
        assert!(
            mock.module(ModuleKind::Knobs3)
                .set_software_mode(true)
                .is_err()
        );
    }

    #[test]
    fn a_module_switched_back_elsewhere_is_noticed() {
        let mock = Mock::with_features(&[KEYS_FIRMWARE, &[0x3c, 0x01, 30]]);
        let keys = mock.module(ModuleKind::Keys15);
        keys.set_software_mode(true).unwrap();
        mock.0.lock().unwrap().standalone = true;
        assert!(matches!(
            keys.display_brightness(),
            Err(Error::NotInSoftwareMode { .. })
        ));
        // Not asked a second time.
        let asked = mock.sent().len();
        assert!(keys.firmware().is_err());
        assert_eq!(mock.sent().len(), asked);
    }

    #[test]
    fn backlight_is_sent_until_the_module_reports_it() {
        let mock = Mock::with_features(&[KEYS_FIRMWARE, &[0x3c, 0x01, 100]]);
        mock.0.lock().unwrap().ignore_backlight = 2;
        let keys = mock.module(ModuleKind::Keys15);
        keys.set_software_mode(true).unwrap();
        keys.set_display_brightness(40).unwrap();
        let writes = mock
            .sent()
            .iter()
            .filter(|s| matches!(s, Sent::Feature(r) if r[1] == 0x34))
            .count();
        assert_eq!(writes, 3);
        assert_eq!(keys.display_brightness().unwrap(), 40);

        // An invalid level is refused before anything is sent.
        let sent = mock.sent().len();
        assert!(matches!(
            keys.set_display_brightness(101),
            Err(Error::InvalidArgument(_))
        ));
        assert_eq!(mock.sent().len(), sent);

        mock.0.lock().unwrap().ignore_backlight = usize::MAX;
        assert!(matches!(
            keys.set_display_brightness(70),
            Err(Error::BrightnessNotApplied {
                wanted: 70,
                found: 40,
                ..
            })
        ));
    }

    #[test]
    fn an_answer_that_is_not_a_firmware_version_does_not_confirm() {
        let mock = Mock::with_features(&[&[0x38, 0x01, 0x3c]]);
        let keys = mock.module(ModuleKind::Keys15);
        assert!(matches!(
            keys.set_software_mode(true),
            Err(Error::MalformedReport(_))
        ));
        assert!(matches!(
            keys.firmware(),
            Err(Error::NotInSoftwareMode { .. })
        ));
    }

    #[test]
    fn reads_back_led_and_brightness_state() {
        let knobs = Mock::with_features(&[&[0x32, 0x03, 100, 40, 100]]).module(ModuleKind::Knobs3);
        assert_eq!(knobs.led_brightness().unwrap(), [100, 40, 100]);
        assert!(knobs.display_brightness().is_err());

        let mock = Mock::with_features(&[KEYS_FIRMWARE, &[0x3c, 0x01, 30]]);
        let keys = mock.module(ModuleKind::Keys15);
        keys.set_software_mode(true).unwrap();
        assert_eq!(keys.display_brightness().unwrap(), 30);
        assert!(keys.led_brightness().is_err());
        assert_eq!(mock.sent().last(), Some(&Sent::FeatureRead(0x3c)));

        // A zone count that does not fit the module is an error, not a guess.
        let odd = Mock::with_features(&[&[0x32, 0x01, 50]]).module(ModuleKind::Knobs3);
        assert!(odd.led_brightness().is_err());
    }

    #[test]
    fn short_feature_reads_are_rejected() {
        // Two bytes announcing eleven: the zeroed rest of the buffer must
        // not be taken for payload.
        let module = Mock::with_features(&[&[0x4b, 0x0b]]).module(ModuleKind::DialDisplay);
        assert!(matches!(
            module.device_info(),
            Err(Error::MalformedReport(_))
        ));
    }

    #[test]
    fn fader_read_primes_the_decoder() {
        let mock = Mock::with_features(&[&[0x33, 0x05, 44, 45, 57, 58, 56]]);
        mock.push_input(&[0x01, 0x2a, 0x07, 0x00, 0x02, 0x05, 44, 45, 57, 58, 60]);
        let mut faders = mock.module(ModuleKind::Faders5);
        assert_eq!(faders.fader_positions().unwrap(), [44, 45, 57, 58, 56]);
        assert_eq!(
            faders.read_events(Duration::ZERO).unwrap(),
            [InputEvent::AnalogChanged {
                index: 4,
                value: 60
            }]
        );

        let knobs = Mock::default();
        assert!(knobs.module(ModuleKind::Knobs3).fader_positions().is_err());
        assert!(knobs.sent().is_empty());
    }

    #[test]
    fn uploads_image_in_order() {
        let mock = Mock::with_features(&[KEYS_FIRMWARE]);
        let keys = mock.module(ModuleKind::Keys15);
        keys.set_software_mode(true).unwrap();
        keys.set_image(14, &fake_jpeg(72, 72, 700)).unwrap();

        let sent = mock.sent();
        assert_eq!(sent[0], mode_report(0x3a, true));
        let outputs: Vec<&Vec<u8>> = sent[3..]
            .iter()
            .map(|s| match s {
                Sent::Output(report) => report,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(outputs.len(), 3);
        assert!(outputs.iter().all(|r| r.len() == 320 && r[6] == 14));
        assert_eq!(outputs.iter().map(|r| r[7]).collect::<Vec<_>>(), [0, 0, 1]);
        // 700 = 312 + 312 + 76: the unused tail of the last report is zero.
        assert!(outputs[2][8 + 76..].iter().all(|b| *b == 0));
    }

    #[test]
    fn invalid_images_send_nothing() {
        let mock = Mock::default();
        let keys = mock.module(ModuleKind::Keys15);
        assert!(keys.set_image(15, &fake_jpeg(72, 72, 700)).is_err());
        assert!(keys.set_image(0, &fake_jpeg(240, 240, 700)).is_err());
        assert!(keys.check_image(0, b"not a jpeg").is_err());
        assert!(keys.check_image(0, &fake_jpeg(72, 72, 700)).is_ok());
        assert!(mock.sent().is_empty());
    }

    #[test]
    fn a_failed_write_stops_the_upload() {
        let mock = Mock::default();
        mock.0.lock().unwrap().fail_output_after = Some(1);
        let keys = mock.module(ModuleKind::Keys15);
        assert!(matches!(
            keys.set_image(0, &fake_jpeg(72, 72, 1000)),
            Err(Error::Disconnected(_))
        ));
        assert_eq!(mock.sent().len(), 1);
    }

    /// A real picture file, for the conversion path.
    #[cfg(feature = "image")]
    fn png() -> Vec<u8> {
        let mut file = Vec::new();
        image::DynamicImage::new_rgb8(20, 10)
            .write_to(
                &mut std::io::Cursor::new(&mut file),
                image::ImageFormat::Png,
            )
            .unwrap();
        file
    }

    /// The SDK must only ever emit the commands that were verified on
    /// hardware, whatever a caller asks of whichever module.
    #[test]
    fn emits_only_the_verified_command_set() {
        let key = fake_jpeg(72, 72, 700);
        let dial = fake_jpeg(240, 240, 700);
        let mut seen = std::collections::BTreeSet::new();
        for kind in ModuleKind::ALL {
            let mock = Mock::with_features(&[
                &[0x2e, 0x00],
                KEYS_FIRMWARE,
                DIAL_FIRMWARE,
                &[0x31, 0x00],
                &[0x3b, 0x00],
                &[0x4b, 0x00],
                &[0x33, 0x05, 0, 0, 0, 0, 0],
                &[0x32, 0x00],
                &[0x3c, 0x01, 50],
                &[0x4c, 0x01, 50],
            ]);
            let mut module = mock.module(kind);
            let _ = module.set_software_mode(true);
            let _ = module.firmware();
            let _ = module.device_info();
            let _ = module.led_brightness();
            let _ = module.display_brightness();
            let _ = module.display_info();
            let _ = module.fader_positions();
            for level in [0, 50, 100, 101, 255] {
                let _ = module.set_display_brightness(level);
            }
            let _ = module.set_software_mode(false);
            for target in [0, 14, 15, 255] {
                let _ = module.set_image(target, &key);
                let _ = module.set_image(target, &dial);
                #[cfg(feature = "image")]
                let _ = module.set_picture(target, &png());
                for mode in LedMode::ALL {
                    let _ = module.set_led_mode(target, mode);
                }
                let _ = module.set_led_color(target, 1, 2, 3);
                let _ = module.set_led_brightness(target, 100);
                let _ = module.set_led_brightness(target, 255);
            }
            for sent in mock.sent() {
                if let Sent::Output(r) = &sent {
                    // LED reports never ask the module to store a setting.
                    match r[1] {
                        0x2c => assert_eq!(r[5], 0),
                        0x2d => assert_eq!(r[7], 0),
                        _ => {}
                    }
                }
                seen.insert(match sent {
                    Sent::Output(r) => (kind.name(), "out", r[0], r[1]),
                    Sent::Feature(r) => (kind.name(), "set", r[0], r[1]),
                    Sent::FeatureRead(id) => (kind.name(), "get", id, 0),
                });
            }
        }
        use ModuleKind::*;
        let expected = std::collections::BTreeSet::from([
            (Base.name(), "get", 0x2e, 0),
            (Base.name(), "get", 0x31, 0),
            (Base.name(), "get", 0x32, 0),
            (Base.name(), "out", 0x02, 0x2c),
            (Base.name(), "out", 0x02, 0x2d),
            (Base.name(), "out", 0x02, 0x2e),
            (Faders5.name(), "out", 0x02, 0x2c),
            (Faders5.name(), "out", 0x02, 0x2d),
            (Faders5.name(), "out", 0x02, 0x2e),
            (Faders5.name(), "get", 0x2e, 0),
            (Faders5.name(), "get", 0x31, 0),
            (Faders5.name(), "get", 0x32, 0),
            (Faders5.name(), "get", 0x33, 0),
            (Knobs3.name(), "get", 0x2e, 0),
            (Knobs3.name(), "get", 0x31, 0),
            (Knobs3.name(), "get", 0x32, 0),
            (Knobs3.name(), "out", 0x02, 0x2c),
            (Knobs3.name(), "out", 0x02, 0x2d),
            (Knobs3.name(), "out", 0x02, 0x2e),
            (Rollers2.name(), "get", 0x2e, 0),
            (Rollers2.name(), "get", 0x31, 0),
            (Keys15.name(), "get", 0x38, 0),
            (Keys15.name(), "get", 0x3b, 0),
            (Keys15.name(), "get", 0x3c, 0),
            (Keys15.name(), "set", 0x03, 0x34),
            (Keys15.name(), "set", 0x03, 0x3a),
            (Keys15.name(), "out", 0x02, 0x36),
            (DialDisplay.name(), "get", 0x48, 0),
            (DialDisplay.name(), "get", 0x4b, 0),
            (DialDisplay.name(), "get", 0x4c, 0),
            (DialDisplay.name(), "set", 0x03, 0x44),
            (DialDisplay.name(), "set", 0x03, 0x49),
            (DialDisplay.name(), "out", 0x02, 0x45),
        ]);
        assert_eq!(seen, expected);
    }
}
