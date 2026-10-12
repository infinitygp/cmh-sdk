//! An open MasterHUB module.

use std::time::Duration;

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::input::{InputDecoder, InputEvent};
use crate::protocol::feature::{
    DisplayInfo, FADER_POSITIONS_REPORT_ID, FEATURE_REPORT_LEN, FirmwareInfo,
    device_info_report_id, display_brightness_report, feature_payload, firmware_report_id,
    parse_fader_positions, software_mode_report,
};
use crate::protocol::frame::INPUT_REPORT_LEN;
use crate::protocol::image::image_reports;
use crate::protocol::led::{LedCommand, LedMode, led_report};
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
    pub fn read_raw(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
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

    /// Reads the running firmware version.
    pub fn firmware(&self) -> Result<FirmwareInfo> {
        let id = firmware_report_id(self.kind);
        FirmwareInfo::parse(feature_payload(&self.read_feature(id)?, id)?)
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
    pub fn set_software_mode(&self, enabled: bool) -> Result<()> {
        self.transport
            .send_feature(&software_mode_report(self.kind, enabled)?)
    }

    /// Sets the backlight (`0..=100`) of a display module.
    pub fn set_display_brightness(&self, level: u8) -> Result<()> {
        self.transport
            .send_feature(&display_brightness_report(self.kind, level)?)
    }

    /// Checks that [`set_image`](Self::set_image) would accept `jpeg` for
    /// `target`, without sending anything.
    pub fn check_image(&self, target: u8, jpeg: &[u8]) -> Result<()> {
        image_reports(self.kind, target, jpeg).map(drop)
    }

    /// Shows a JPEG on image `target` (key index on the 15 LCD Keys, `0` on
    /// the Dial Display).
    ///
    /// The image must be a baseline JPEG of exactly the display size with
    /// three components and no chroma subsampling. Nothing is sent if it is
    /// not. The module does not store the image.
    pub fn set_image(&self, target: u8, jpeg: &[u8]) -> Result<()> {
        for report in image_reports(self.kind, target, jpeg)? {
            self.transport.write_output(&report)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::protocol::image::tests::fake_jpeg;

    /// Everything a module was asked to do, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Sent {
        Output(Vec<u8>),
        Feature(Vec<u8>),
        FeatureRead(u8),
    }

    #[derive(Default)]
    struct State {
        inputs: VecDeque<Vec<u8>>,
        features: Vec<Vec<u8>>,
        sent: Vec<Sent>,
        fail_output_after: Option<usize>,
    }

    #[derive(Clone, Default)]
    struct Mock(Arc<Mutex<State>>);

    impl Mock {
        fn with_features(features: &[&[u8]]) -> Self {
            let mock = Mock::default();
            mock.0.lock().unwrap().features = features.iter().map(|f| f.to_vec()).collect();
            mock
        }

        fn push_input(&self, report: &[u8]) {
            self.0.lock().unwrap().inputs.push_back(report.to_vec());
        }

        fn sent(&self) -> Vec<Sent> {
            self.0.lock().unwrap().sent.clone()
        }

        fn module(&self, kind: ModuleKind) -> Module {
            Module::new(kind, None, Box::new(self.clone()))
        }
    }

    impl Transport for Mock {
        fn read_input(&self, buf: &mut [u8], _timeout: Duration) -> Result<usize> {
            match self.0.lock().unwrap().inputs.pop_front() {
                Some(report) => {
                    buf[..report.len()].copy_from_slice(&report);
                    Ok(INPUT_REPORT_LEN)
                }
                None => Ok(0),
            }
        }

        fn write_output(&self, report: &[u8]) -> Result<()> {
            let mut state = self.0.lock().unwrap();
            let outputs = state
                .sent
                .iter()
                .filter(|s| matches!(s, Sent::Output(_)))
                .count();
            if state.fail_output_after == Some(outputs) {
                return Err(Error::Disconnected("mock".to_owned()));
            }
            state.sent.push(Sent::Output(report.to_vec()));
            Ok(())
        }

        fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
            let mut state = self.0.lock().unwrap();
            state.sent.push(Sent::FeatureRead(buf[0]));
            let report = state
                .features
                .iter()
                .find(|f| f[0] == buf[0])
                .expect("unexpected feature read")
                .clone();
            buf[..report.len()].copy_from_slice(&report);
            Ok(report.len())
        }

        fn send_feature(&self, report: &[u8]) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .sent
                .push(Sent::Feature(report.to_vec()));
            Ok(())
        }

        fn report_descriptor(&self) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    const DIAL_FIRMWARE: &[u8] = &[
        0x48, 0x0f, 0x26, 0xa0, 0xd8, 0x0a, b'0', b'1', b'.', b'0', b'0', b'.', b'0', b'4', b'.',
        b'0', b'0',
    ];
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
        assert_eq!(module.firmware().unwrap().version, "01.00.04.00");
        assert_eq!(module.display_info().unwrap().image_width, 240);
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
        let mock = Mock::default();
        let keys = mock.module(ModuleKind::Keys15);
        keys.set_software_mode(true).unwrap();
        keys.set_image(14, &fake_jpeg(72, 72, 700)).unwrap();

        let sent = mock.sent();
        let mut mode = [0u8; 32];
        mode[..3].copy_from_slice(&[0x03, 0x3a, 0x01]);
        assert_eq!(sent[0], Sent::Feature(mode.to_vec()));
        let outputs: Vec<&Vec<u8>> = sent[1..]
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
                &[0x38, 0x00],
                &[0x48, 0x00],
                &[0x31, 0x00],
                &[0x3b, 0x00],
                &[0x4b, 0x00],
                &[0x33, 0x05, 0, 0, 0, 0, 0],
            ]);
            let mut module = mock.module(kind);
            let _ = module.firmware();
            let _ = module.device_info();
            let _ = module.display_info();
            let _ = module.fader_positions();
            let _ = module.set_software_mode(true);
            let _ = module.set_software_mode(false);
            for level in [0, 50, 100, 101, 255] {
                let _ = module.set_display_brightness(level);
            }
            for target in [0, 14, 15, 255] {
                let _ = module.set_image(target, &key);
                let _ = module.set_image(target, &dial);
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
            (Base.name(), "out", 0x02, 0x2c),
            (Base.name(), "out", 0x02, 0x2d),
            (Base.name(), "out", 0x02, 0x2e),
            (Faders5.name(), "out", 0x02, 0x2c),
            (Faders5.name(), "out", 0x02, 0x2d),
            (Faders5.name(), "out", 0x02, 0x2e),
            (Faders5.name(), "get", 0x2e, 0),
            (Faders5.name(), "get", 0x31, 0),
            (Faders5.name(), "get", 0x33, 0),
            (Knobs3.name(), "get", 0x2e, 0),
            (Knobs3.name(), "get", 0x31, 0),
            (Knobs3.name(), "out", 0x02, 0x2c),
            (Knobs3.name(), "out", 0x02, 0x2d),
            (Knobs3.name(), "out", 0x02, 0x2e),
            (Rollers2.name(), "get", 0x2e, 0),
            (Rollers2.name(), "get", 0x31, 0),
            (Keys15.name(), "get", 0x38, 0),
            (Keys15.name(), "get", 0x3b, 0),
            (Keys15.name(), "set", 0x03, 0x34),
            (Keys15.name(), "set", 0x03, 0x3a),
            (Keys15.name(), "out", 0x02, 0x36),
            (DialDisplay.name(), "get", 0x48, 0),
            (DialDisplay.name(), "get", 0x4b, 0),
            (DialDisplay.name(), "set", 0x03, 0x44),
            (DialDisplay.name(), "set", 0x03, 0x49),
            (DialDisplay.name(), "out", 0x02, 0x45),
        ]);
        assert_eq!(seen, expected);
    }
}
