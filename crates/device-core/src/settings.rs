pub const SLOT_BYTES: usize = 24;
pub const DEFAULT_REFRESH_SECONDS: u32 = 900;
pub const MIN_REFRESH_SECONDS: u32 = 60;
pub const MAX_REFRESH_SECONDS: u32 = 86400;
pub const ROTATION_COUNT: u8 = 4;
pub const COMMIT_OFFSET: usize = 20;
pub const COMMIT_BYTES: usize = 4;
const MAGIC: &[u8; 4] = b"E104";
const FORMAT_VERSION: u8 = 1;
const VERSION_OFFSET: usize = 4;
const ROTATION_OFFSET: usize = 5;
const RESERVED_OFFSETS: [usize; 2] = [6, 7];
const GENERATION_RANGE: core::ops::Range<usize> = 8..12;
const INTERVAL_RANGE: core::ops::Range<usize> = 12..16;
const CRC_PAYLOAD_BYTES: usize = 16;
const CRC_RANGE: core::ops::Range<usize> = 16..20;
const COMMIT_MARKER: &[u8; COMMIT_BYTES] = b"DONE";
const CRC_BITS_PER_BYTE: usize = 8;
const CRC_POLYNOMIAL: u32 = 0xedb8_8320;
const GENERATION_HALF_RANGE: u32 = 1 << 31;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub refresh_seconds: u32,
    pub rotation: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
            rotation: 0,
        }
    }
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..CRC_BITS_PER_BYTE {
            crc = (crc >> 1) ^ if crc & 1 == 1 { CRC_POLYNOMIAL } else { 0 };
        }
    }
    !crc
}

impl Settings {
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
    bytes[GENERATION_RANGE].copy_from_slice(&generation.to_le_bytes());
    bytes[INTERVAL_RANGE].copy_from_slice(&settings.refresh_seconds.to_le_bytes());
    let crc = crc32(&bytes[..CRC_PAYLOAD_BYTES]);
    bytes[CRC_RANGE].copy_from_slice(&crc.to_le_bytes());
    bytes[COMMIT_OFFSET..].copy_from_slice(COMMIT_MARKER);
    bytes
}

pub fn decode(bytes: &[u8]) -> Option<(Settings, u32)> {
    let bytes: &[u8; SLOT_BYTES] = bytes.try_into().ok()?;
    if &bytes[..MAGIC.len()] != MAGIC
        || bytes[VERSION_OFFSET] != FORMAT_VERSION
        || bytes[RESERVED_OFFSETS[0]] != 0
        || bytes[RESERVED_OFFSETS[1]] != 0
        || &bytes[COMMIT_OFFSET..] != COMMIT_MARKER
    {
        return None;
    }
    let crc = u32::from_le_bytes(bytes[CRC_RANGE].try_into().ok()?);
    if crc32(&bytes[..CRC_PAYLOAD_BYTES]) != crc {
        return None;
    }
    let generation = u32::from_le_bytes(bytes[GENERATION_RANGE].try_into().ok()?);
    let settings = Settings {
        refresh_seconds: u32::from_le_bytes(bytes[INTERVAL_RANGE].try_into().ok()?),
        rotation: bytes[ROTATION_OFFSET],
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
