use crate::{
    app::ScreenId,
    calendar::Month,
    daily::{Daily, TaskId, DAILY_BYTES},
    image::{Anchor, Fit},
    media::{GallerySettings, ImageId, SlideInterval},
};

pub const SLOT_BYTES: usize = 512;
pub const LEGACY_SLOT_BYTES: usize = 24;
pub const DEFAULT_REFRESH_SECONDS: u32 = 900;
pub const MIN_REFRESH_SECONDS: u32 = 60;
pub const MAX_REFRESH_SECONDS: u32 = 86400;
pub const ROTATION_COUNT: u8 = 4;
pub const COMMIT_OFFSET: usize = SLOT_BYTES - COMMIT_BYTES;
pub const COMMIT_BYTES: usize = 4;
const MAGIC: &[u8; 4] = b"E104";
const FORMAT_VERSION: u8 = 4;
const DAILY_FORMAT_VERSION: u8 = 3;
const PREVIOUS_FORMAT_VERSION: u8 = 1;
const VERSION_OFFSET: usize = 4;
const ROTATION_OFFSET: usize = 5;
const RESUME_OFFSET: usize = 6;
const SCREEN_OFFSET: usize = 7;
const GENERATION_RANGE: core::ops::Range<usize> = 8..12;
const INTERVAL_RANGE: core::ops::Range<usize> = 12..16;
const DAILY_OFFSET: usize = 16;
const SELECTION_OFFSET: usize = DAILY_OFFSET + DAILY_BYTES;
const GALLERY_OFFSET: usize = SELECTION_OFFSET + 4;
const CRC_PAYLOAD_BYTES: usize = COMMIT_OFFSET - 4;
const CRC_RANGE: core::ops::Range<usize> = CRC_PAYLOAD_BYTES..COMMIT_OFFSET;
const COMMIT_MARKER: &[u8; COMMIT_BYTES] = b"DONE";
const CRC_BITS_PER_BYTE: usize = 8;
const CRC_POLYNOMIAL: u32 = 0xedb8_8320;
const GENERATION_HALF_RANGE: u32 = 1 << 31;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub refresh_seconds: u32,
    pub rotation: u8,
    pub resume: ResumePolicy,
    pub screen: ScreenId,
    pub daily: Daily,
    pub selected_task: TaskId,
    pub calendar_month: Option<Month>,
    pub gallery: GallerySettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Configuration {
    pub refresh_seconds: u32,
    pub rotation: u8,
    pub resume: ResumePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigurationId(u32);

impl ConfigurationId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigurationRequest {
    id: ConfigurationId,
    value: Configuration,
}

impl ConfigurationRequest {
    pub const fn new(id: ConfigurationId, value: Configuration) -> Self {
        Self { id, value }
    }
    pub const fn id(self) -> ConfigurationId {
        self.id
    }
    pub const fn value(self) -> Configuration {
        self.value
    }
    pub fn acknowledge(self, saved: ConfigurationId) -> Option<Self> {
        if self.id == saved {
            None
        } else {
            Some(self)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResumePolicy {
    #[default]
    Last,
    Status,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
            rotation: 0,
            resume: ResumePolicy::Last,
            screen: ScreenId::Status,
            daily: Daily::default(),
            selected_task: TaskId::default(),
            calendar_month: None,
            gallery: GallerySettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    pub const fn new() -> Self {
        Self(!0)
    }
    pub fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u32::from(*byte);
            for _ in 0..CRC_BITS_PER_BYTE {
                self.0 = (self.0 >> 1) ^ if self.0 & 1 == 1 { CRC_POLYNOMIAL } else { 0 };
            }
        }
    }
    pub const fn finish(self) -> u32 {
        !self.0
    }
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

impl Settings {
    pub fn retained_selection(
        self,
        committed: Self,
        wake: crate::telemetry::WakeReason,
    ) -> Option<ImageId> {
        if wake == crate::telemetry::WakeReason::PowerOn {
            return None;
        }
        let normalized = Self {
            gallery: GallerySettings {
                selected: committed.gallery.selected,
                ..self.gallery
            },
            ..self
        };
        (self.valid() && normalized == committed).then_some(self.gallery.selected)
    }
    pub const fn configuration(self) -> Configuration {
        Configuration {
            refresh_seconds: self.refresh_seconds,
            rotation: self.rotation,
            resume: self.resume,
        }
    }
    pub fn with_configuration(self, configuration: Configuration, screen: ScreenId) -> Self {
        let settings = Self {
            refresh_seconds: configuration.refresh_seconds,
            rotation: configuration.rotation,
            resume: configuration.resume,
            screen,
            ..self
        };
        Self {
            screen: settings.wake_screen(),
            ..settings
        }
    }
    pub fn wake_screen(self) -> ScreenId {
        match self.resume {
            ResumePolicy::Last => self.screen,
            ResumePolicy::Status => ScreenId::Status,
        }
    }
    pub fn valid(self) -> bool {
        (MIN_REFRESH_SECONDS..=MAX_REFRESH_SECONDS).contains(&self.refresh_seconds)
            && self.rotation < ROTATION_COUNT
    }
}

pub fn encode(settings: Settings, generation: u32) -> [u8; SLOT_BYTES] {
    let mut bytes = [0u8; SLOT_BYTES];
    bytes[..MAGIC.len()].copy_from_slice(MAGIC);
    bytes[VERSION_OFFSET] = FORMAT_VERSION;
    bytes[ROTATION_OFFSET] = settings.rotation;
    bytes[RESUME_OFFSET] = match settings.resume {
        ResumePolicy::Last => 0,
        ResumePolicy::Status => 1,
    };
    bytes[SCREEN_OFFSET] = settings.screen.code();
    bytes[GENERATION_RANGE].copy_from_slice(&generation.to_le_bytes());
    bytes[INTERVAL_RANGE].copy_from_slice(&settings.refresh_seconds.to_le_bytes());
    bytes[DAILY_OFFSET..DAILY_OFFSET + DAILY_BYTES].copy_from_slice(&settings.daily.encode());
    bytes[SELECTION_OFFSET] = settings.selected_task.number();
    if let Some(month) = settings.calendar_month {
        bytes[SELECTION_OFFSET + 1..SELECTION_OFFSET + 3]
            .copy_from_slice(&month.year().to_le_bytes());
        bytes[SELECTION_OFFSET + 3] = month.number();
    }
    bytes[GALLERY_OFFSET] = settings.gallery.selected.number();
    bytes[GALLERY_OFFSET + 1] = settings.gallery.fit as u8;
    bytes[GALLERY_OFFSET + 2] = settings.gallery.anchor as u8;
    bytes[GALLERY_OFFSET + 3..GALLERY_OFFSET + 7].copy_from_slice(
        &settings
            .gallery
            .slideshow
            .map_or(0, SlideInterval::seconds)
            .to_le_bytes(),
    );
    let crc = crc32(&bytes[..CRC_PAYLOAD_BYTES]);
    bytes[CRC_RANGE].copy_from_slice(&crc.to_le_bytes());
    bytes[COMMIT_OFFSET..].copy_from_slice(COMMIT_MARKER);
    bytes
}

pub(crate) struct ValidatedRecord<'a> {
    bytes: &'a [u8],
    version: u8,
}

pub(crate) fn validate_record(bytes: &[u8]) -> Option<ValidatedRecord<'_>> {
    if !matches!(bytes.len(), LEGACY_SLOT_BYTES | SLOT_BYTES) || bytes.get(..MAGIC.len())? != MAGIC
    {
        return None;
    }
    let version = bytes[VERSION_OFFSET];
    let (payload, checksum, marker) = match version {
        1 | 2 => (16, 16..20, 20..24),
        DAILY_FORMAT_VERSION | FORMAT_VERSION if bytes.len() == SLOT_BYTES => {
            (CRC_PAYLOAD_BYTES, CRC_RANGE, COMMIT_OFFSET..SLOT_BYTES)
        }
        _ => return None,
    };
    if &bytes[marker] != COMMIT_MARKER
        || crc32(&bytes[..payload]) != u32::from_le_bytes(bytes[checksum].try_into().ok()?)
    {
        return None;
    }
    Some(ValidatedRecord { bytes, version })
}

pub fn decode(bytes: &[u8]) -> Option<(Settings, u32)> {
    let record = validate_record(bytes)?;
    let bytes = record.bytes;
    let version = record.version;
    let daily = if version >= DAILY_FORMAT_VERSION {
        Daily::decode(&bytes[DAILY_OFFSET..DAILY_OFFSET + DAILY_BYTES])?
    } else {
        Daily::default()
    };
    let generation = u32::from_le_bytes(bytes[GENERATION_RANGE].try_into().ok()?);
    let (resume, screen) = match version {
        PREVIOUS_FORMAT_VERSION if bytes[RESUME_OFFSET] == 0 && bytes[SCREEN_OFFSET] == 0 => {
            (ResumePolicy::Last, ScreenId::Status)
        }
        2 | DAILY_FORMAT_VERSION | FORMAT_VERSION => (
            match bytes[RESUME_OFFSET] {
                0 => ResumePolicy::Last,
                1 => ResumePolicy::Status,
                _ => return None,
            },
            ScreenId::from_code(bytes[SCREEN_OFFSET]).filter(|screen| match version {
                1 | 2 => screen.code() <= 2,
                3 => screen.code() <= 5,
                _ => true,
            })?,
        ),
        _ => return None,
    };
    let settings = Settings {
        refresh_seconds: u32::from_le_bytes(bytes[INTERVAL_RANGE].try_into().ok()?),
        rotation: bytes[ROTATION_OFFSET],
        resume,
        screen,
        daily,
        selected_task: if version >= DAILY_FORMAT_VERSION {
            TaskId::new(bytes[SELECTION_OFFSET])?
        } else {
            TaskId::default()
        },
        calendar_month: if version >= DAILY_FORMAT_VERSION {
            let year = u16::from_le_bytes(
                bytes[SELECTION_OFFSET + 1..SELECTION_OFFSET + 3]
                    .try_into()
                    .ok()?,
            );
            let month = bytes[SELECTION_OFFSET + 3];
            match (year, month) {
                (0, 0) => None,
                _ => Some(Month::new(year, month)?),
            }
        } else {
            None
        },
        gallery: if version == FORMAT_VERSION {
            GallerySettings {
                selected: ImageId::new(bytes[GALLERY_OFFSET])?,
                fit: Fit::from_code(bytes[GALLERY_OFFSET + 1])?,
                anchor: Anchor::from_code(bytes[GALLERY_OFFSET + 2])?,
                slideshow: match u32::from_le_bytes(
                    bytes[GALLERY_OFFSET + 3..GALLERY_OFFSET + 7]
                        .try_into()
                        .ok()?,
                ) {
                    0 => None,
                    value => Some(SlideInterval::new(value)?),
                },
            }
        } else {
            GallerySettings::default()
        },
    };
    settings.valid().then_some((settings, generation))
}

pub fn recover(first: &[u8], second: &[u8]) -> Option<(Settings, u32, usize)> {
    match (decode(first), decode(second)) {
        (Some((a, ga)), Some((b, gb))) => {
            let distance = gb.wrapping_sub(ga);
            if distance != 0 && distance < GENERATION_HALF_RANGE {
                Some((b, gb, 1))
            } else {
                Some((a, ga, 0))
            }
        }
        (Some((a, ga)), None) => Some((a, ga, 0)),
        (None, Some((b, gb))) => Some((b, gb, 1)),
        (None, None) => None,
    }
}
