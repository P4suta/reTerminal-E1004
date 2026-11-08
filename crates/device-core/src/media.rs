use crate::{
    geometry::{Color, Rotation, FRAME_BYTES},
    image::{Anchor, Fit},
    settings::{crc32, Crc32},
};

pub const MAX_IMAGES: usize = 8;
pub const IMAGE_SLOTS: usize = MAX_IMAGES + 1;
pub const HEADER_BYTES: usize = 32;
pub const CHUNK_BYTES: usize = 256;
const MAGIC: &[u8; 8] = b"E104IMG\x01";
const COMMITTED: &[u8; 4] = b"DONE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageId(u8);

impl ImageId {
    pub const fn new(value: u8) -> Option<Self> {
        if value >= 1 && value <= MAX_IMAGES as u8 {
            Some(Self(value))
        } else {
            None
        }
    }
    pub const fn number(self) -> u8 {
        self.0
    }
    pub const fn index(self) -> usize {
        self.0 as usize - 1
    }
}

impl Default for ImageId {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalSlot(u8);

impl PhysicalSlot {
    pub const fn new(value: u8) -> Option<Self> {
        if value < IMAGE_SLOTS as u8 {
            Some(Self(value))
        } else {
            None
        }
    }
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDescriptor {
    pub generation: u64,
    pub rotation: Rotation,
    pub checksum: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageRecord {
    id: ImageId,
    descriptor: ImageDescriptor,
    removed: bool,
}

impl ImageRecord {
    pub const fn image(id: ImageId, generation: u64, rotation: Rotation, checksum: u32) -> Self {
        Self {
            id,
            descriptor: ImageDescriptor {
                generation,
                rotation,
                checksum,
            },
            removed: false,
        }
    }
    pub const fn deleted(id: ImageId, generation: u64) -> Self {
        Self {
            id,
            descriptor: ImageDescriptor {
                generation,
                rotation: Rotation::Portrait,
                checksum: 0,
            },
            removed: true,
        }
    }
    pub const fn id(self) -> ImageId {
        self.id
    }
    pub const fn descriptor(self) -> ImageDescriptor {
        self.descriptor
    }
    pub const fn is_deleted(self) -> bool {
        self.removed
    }
    pub fn encode(self) -> [u8; HEADER_BYTES] {
        let mut bytes = [0; HEADER_BYTES];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8] = self.id.number();
        bytes[9] = self.descriptor.rotation as u8;
        bytes[10] = u8::from(self.removed);
        bytes[12..20].copy_from_slice(&self.descriptor.generation.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.descriptor.checksum.to_le_bytes());
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
        bytes[28..].copy_from_slice(COMMITTED);
        bytes
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != HEADER_BYTES
            || &bytes[..8] != MAGIC
            || &bytes[28..] != COMMITTED
            || bytes[11] != 0
        {
            return None;
        }
        if crc32(&bytes[..24]) != u32::from_le_bytes(bytes[24..28].try_into().ok()?) {
            return None;
        }
        let id = ImageId::new(bytes[8])?;
        let rotation = Rotation::from_code(bytes[9])?;
        let generation = u64::from_le_bytes(bytes[12..20].try_into().ok()?);
        let checksum = u32::from_le_bytes(bytes[20..24].try_into().ok()?);
        match bytes[10] {
            0 => Some(Self::image(id, generation, rotation, checksum)),
            1 if rotation == Rotation::Portrait && checksum == 0 => {
                Some(Self::deleted(id, generation))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Catalog([Option<ImageDescriptor>; MAX_IMAGES]);

impl Catalog {
    pub fn len(self) -> usize {
        self.0.iter().filter(|value| value.is_some()).count()
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn get(self, id: ImageId) -> Option<ImageDescriptor> {
        self.0[id.index()]
    }
    pub const fn entries(&self) -> &[Option<ImageDescriptor>; MAX_IMAGES] {
        &self.0
    }
    pub fn first(self) -> Option<ImageId> {
        self.0
            .iter()
            .position(Option::is_some)
            .map(|index| ImageId(index as u8 + 1))
    }
    pub fn step(self, current: ImageId, forward: bool) -> Option<ImageId> {
        for offset in 1..=MAX_IMAGES {
            let index =
                (current.index() + if forward { offset } else { MAX_IMAGES - offset }) % MAX_IMAGES;
            if self.0[index].is_some() {
                return Some(ImageId(index as u8 + 1));
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageIndex {
    records: [Option<ImageRecord>; IMAGE_SLOTS],
}

impl Default for ImageIndex {
    fn default() -> Self {
        Self::new([None; IMAGE_SLOTS])
    }
}

impl ImageIndex {
    pub const fn new(records: [Option<ImageRecord>; IMAGE_SLOTS]) -> Self {
        Self { records }
    }
    pub fn current(self, id: ImageId) -> Option<(PhysicalSlot, ImageRecord)> {
        let mut result: Option<(PhysicalSlot, ImageRecord)> = None;
        for (index, record) in self.records.into_iter().enumerate() {
            if let Some(record) = record.filter(|record| record.id == id) {
                if result.is_none_or(|(_, saved)| {
                    record.descriptor.generation > saved.descriptor.generation
                }) {
                    result = Some((PhysicalSlot(index as u8), record));
                }
            }
        }
        result
    }
    pub fn catalog(self) -> Catalog {
        let mut values = [None; MAX_IMAGES];
        for (index, value) in values.iter_mut().enumerate() {
            if let Some((_, record)) = self.current(ImageId(index as u8 + 1)) {
                *value = (!record.removed).then_some(record.descriptor);
            }
        }
        Catalog(values)
    }
    pub fn staging(self) -> Option<PhysicalSlot> {
        self.records.iter().enumerate().find_map(|(index, record)| {
            let slot = PhysicalSlot(index as u8);
            if record.is_none_or(|record| {
                self.current(record.id)
                    .is_none_or(|(current, _)| current != slot)
            }) {
                Some(slot)
            } else {
                None
            }
        })
    }
    pub fn next_generation(self) -> Option<u64> {
        match self
            .records
            .iter()
            .flatten()
            .map(|record| record.descriptor.generation)
            .max()
        {
            Some(value) => value.checked_add(1),
            None => Some(0),
        }
    }
    pub fn committed(mut self, slot: PhysicalSlot, record: ImageRecord) -> Self {
        self.records[slot.index()] = Some(record);
        self
    }
}

pub fn valid_palette_byte(byte: u8) -> bool {
    Color::from_code(byte >> 4).is_some() && Color::from_code(byte & 15).is_some()
}

#[derive(Clone, Copy)]
pub struct PackedFrame<'a> {
    rotation: Rotation,
    bytes: &'a [u8],
}

impl<'a> PackedFrame<'a> {
    pub fn new(rotation: Rotation, bytes: &'a [u8]) -> Option<Self> {
        (bytes.len() == FRAME_BYTES && bytes.iter().copied().all(valid_palette_byte))
            .then_some(Self { rotation, bytes })
    }
    pub fn from_file(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() != HEADER_BYTES + FRAME_BYTES {
            return None;
        }
        let record = ImageRecord::decode(&bytes[..HEADER_BYTES])?;
        let data = &bytes[HEADER_BYTES..];
        if record.removed || crc32(data) != record.descriptor.checksum {
            return None;
        }
        Self::new(record.descriptor.rotation, data)
    }
    pub const fn rotation(self) -> Rotation {
        self.rotation
    }
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn pixel(self, x: u16, y: u16) -> Option<Color> {
        let (x, y) = self.rotation.physical(x, y)?;
        let (index, high) = crate::geometry::pixel_address(x, y)?;
        let byte = self.bytes[index];
        Color::from_code(if high { byte >> 4 } else { byte & 15 })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    Identity,
    Offset,
    Length,
    Palette,
    Incomplete,
    Checksum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    id: u32,
    slot: PhysicalSlot,
    record: ImageRecord,
    received: usize,
    checksum: Crc32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePlan {
    Append(Transfer),
    Retry,
}

impl Transfer {
    pub fn new(id: u32, slot: PhysicalSlot, record: ImageRecord) -> Option<Self> {
        (!record.removed).then_some(Self {
            id,
            slot,
            record,
            received: 0,
            checksum: Crc32::new(),
        })
    }
    pub const fn id(self) -> u32 {
        self.id
    }
    pub const fn slot(self) -> PhysicalSlot {
        self.slot
    }
    pub const fn record(self) -> ImageRecord {
        self.record
    }
    pub const fn received(self) -> usize {
        self.received
    }
    pub fn plan(self, id: u32, offset: usize, bytes: &[u8]) -> Result<WritePlan, TransferError> {
        if id != self.id {
            return Err(TransferError::Identity);
        }
        let end = offset
            .checked_add(bytes.len())
            .ok_or(TransferError::Length)?;
        if bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || !bytes.len().is_multiple_of(4)
            || !offset.is_multiple_of(4)
            || end > FRAME_BYTES
        {
            return Err(TransferError::Length);
        }
        if !bytes.iter().copied().all(valid_palette_byte) {
            return Err(TransferError::Palette);
        }
        if offset == self.received {
            let mut next = self;
            next.received = end;
            next.checksum.update(bytes);
            Ok(WritePlan::Append(next))
        } else if end <= self.received {
            Ok(WritePlan::Retry)
        } else {
            Err(TransferError::Offset)
        }
    }
    pub fn complete(self, id: u32) -> Result<ImageRecord, TransferError> {
        if id != self.id {
            Err(TransferError::Identity)
        } else if self.received != FRAME_BYTES {
            Err(TransferError::Incomplete)
        } else if self.checksum.finish() != self.record.descriptor.checksum {
            Err(TransferError::Checksum)
        } else {
            Ok(self.record)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlideInterval(u32);

impl SlideInterval {
    pub fn new(seconds: u32) -> Option<Self> {
        (60..=86400).contains(&seconds).then_some(Self(seconds))
    }
    pub const fn seconds(self) -> u32 {
        self.0
    }
}

impl Default for SlideInterval {
    fn default() -> Self {
        Self(300)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GallerySettings {
    pub selected: ImageId,
    pub fit: Fit,
    pub anchor: Anchor,
    pub slideshow: Option<SlideInterval>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalleryEvent {
    Ready(Catalog),
    Committed(Catalog, ImageId),
    Busy(bool),
    Select(ImageId),
    Step(bool),
    Fit(Fit),
    Anchor(Anchor),
    Slideshow(Option<SlideInterval>),
    Tick,
    WakeTimer,
    Resume(ImageId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GalleryPhase {
    Loading,
    Ready,
    Receiving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GalleryView {
    pub image: Option<(ImageId, ImageDescriptor)>,
    pub fit: Fit,
    pub anchor: Anchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gallery {
    settings: GallerySettings,
    stored_selection: ImageId,
    catalog: Catalog,
    phase: GalleryPhase,
    next_at: Option<u64>,
    advance_on_load: bool,
}

impl Default for Gallery {
    fn default() -> Self {
        Self::new(GallerySettings::default(), Catalog::default())
    }
}

impl Gallery {
    pub const fn new(settings: GallerySettings, catalog: Catalog) -> Self {
        Self {
            settings,
            stored_selection: settings.selected,
            catalog,
            phase: GalleryPhase::Ready,
            next_at: None,
            advance_on_load: false,
        }
    }
    pub fn loading(settings: GallerySettings) -> Self {
        Self {
            phase: GalleryPhase::Loading,
            ..Self::new(settings, Catalog::default())
        }
    }
    pub const fn settings(self) -> GallerySettings {
        self.settings
    }
    pub fn persistent_settings(self) -> GallerySettings {
        GallerySettings {
            selected: self.stored_selection,
            ..self.settings
        }
    }
    pub const fn catalog(self) -> Catalog {
        self.catalog
    }
    pub fn busy(self) -> bool {
        self.phase != GalleryPhase::Ready
    }
    pub fn view(self) -> GalleryView {
        GalleryView {
            image: self
                .catalog
                .get(self.settings.selected)
                .map(|value| (self.settings.selected, value)),
            fit: self.settings.fit,
            anchor: self.settings.anchor,
        }
    }
    fn schedule(&mut self, now: u64) {
        self.next_at = self
            .settings
            .slideshow
            .and_then(|interval| now.checked_add(u64::from(interval.seconds()) * 1000));
    }
    fn step(&mut self, forward: bool) {
        if let Some(selected) = self.catalog.step(self.settings.selected, forward) {
            self.settings.selected = selected;
        }
    }
    pub fn transition(mut self, event: GalleryEvent, now: u64) -> Self {
        match event {
            GalleryEvent::Ready(catalog) | GalleryEvent::Committed(catalog, _) => {
                self.catalog = catalog;
                self.phase = GalleryPhase::Ready;
                if let GalleryEvent::Committed(_, selected) = event {
                    self.settings.selected = selected;
                    self.stored_selection = selected;
                }
                if self.advance_on_load {
                    self.step(true);
                }
                self.advance_on_load = false;
                if self.catalog.get(self.settings.selected).is_none() {
                    if let Some(first) = self.catalog.first() {
                        self.settings.selected = first;
                    }
                }
                if self.catalog.get(self.stored_selection).is_none() {
                    self.stored_selection = self.settings.selected;
                }
                self.schedule(now);
            }
            GalleryEvent::Busy(true) if self.phase != GalleryPhase::Loading => {
                self.phase = GalleryPhase::Receiving
            }
            GalleryEvent::Busy(false) if self.phase == GalleryPhase::Receiving => {
                self.phase = GalleryPhase::Ready;
                self.schedule(now);
            }
            GalleryEvent::Busy(_) => {}
            GalleryEvent::Select(selected) => {
                if self.catalog.get(selected).is_some() {
                    self.settings.selected = selected;
                    self.stored_selection = selected;
                    self.schedule(now);
                }
            }
            GalleryEvent::Step(forward) => {
                self.step(forward);
                self.stored_selection = self.settings.selected;
                self.schedule(now);
            }
            GalleryEvent::Fit(fit) => self.settings.fit = fit,
            GalleryEvent::Anchor(anchor) => self.settings.anchor = anchor,
            GalleryEvent::Slideshow(interval) => {
                self.settings.slideshow = interval;
                self.stored_selection = self.settings.selected;
                self.schedule(now);
            }
            GalleryEvent::Tick => {
                if self.phase == GalleryPhase::Ready
                    && self.next_at.is_some_and(|deadline| now >= deadline)
                {
                    self.step(true);
                    self.schedule(now);
                }
            }
            GalleryEvent::WakeTimer if self.settings.slideshow.is_some() => {
                if self.phase == GalleryPhase::Loading {
                    self.advance_on_load = true;
                } else {
                    self.step(true);
                    self.schedule(now);
                }
            }
            GalleryEvent::WakeTimer => {}
            GalleryEvent::Resume(selected) if self.phase == GalleryPhase::Loading => {
                self.settings.selected = selected
            }
            GalleryEvent::Resume(_) => {}
        }
        self
    }
}
