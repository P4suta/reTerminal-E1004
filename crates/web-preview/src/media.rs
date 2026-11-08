use super::{element, update, value, workspace, EventHandler, WebPreview, HANDLERS};
use device_core::{
    geometry::{Color, Rotation, FRAME_BYTES},
    image::{Anchor, Conversion, Dither, Fit, Palette, Placement, RgbaImage, Size},
    media::{ImageId, ImageRecord, PackedFrame, MAX_IMAGES},
    presentation::Presentation,
    settings::crc32,
};
use js_sys::{
    futures::{spawn_local, JsFuture},
    Array, Promise, Reflect, Uint8Array,
};
use std::{
    cell::{Cell, RefCell},
    fmt::Write,
    rc::Rc,
};
use wasm_bindgen::{prelude::*, JsCast};

fn failure(message: &str) -> JsValue {
    JsValue::from_str(message)
}
fn message(document: &web_sys::Document, id: &str, text: &str) {
    if let Ok(element) = element(document, id) {
        element.set_text_content(Some(text));
    }
}
fn number<T: core::str::FromStr>(document: &web_sys::Document, id: &str) -> Result<T, JsValue> {
    numeric(document, id, true)
}
fn numeric<T: core::str::FromStr>(
    document: &web_sys::Document,
    id: &str,
    focus: bool,
) -> Result<T, JsValue> {
    let control = element(document, id)?;
    let result = value(document, id)?.parse::<T>();
    let valid = control
        .dyn_ref::<web_sys::HtmlInputElement>()
        .is_none_or(web_sys::HtmlInputElement::check_validity);
    if result.is_err() || !valid {
        control.set_attribute("aria-invalid", "true")?;
        if focus {
            if let Some(details) = control.closest("details")? {
                details.set_attribute("open", "")?;
            }
            control
                .dyn_ref::<web_sys::HtmlElement>()
                .ok_or_else(|| failure("Invalid form control."))?
                .focus()?;
        }
        let name = match id {
            "image-brightness" => "Brightness must be between -100 and 100.",
            "image-contrast" => "Contrast must be between 0 and 200.",
            "image-saturation" => "Saturation must be between 0 and 200.",
            "poster-size" => "Text size must be between 16 and 160.",
            _ => "Enter a number within the field's limits.",
        };
        return Err(failure(name));
    }
    control.remove_attribute("aria-invalid")?;
    result.map_err(|_| failure("Enter a number within the field's limits."))
}
fn context(
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<web_sys::CanvasRenderingContext2d, JsValue> {
    Ok(canvas
        .get_context("2d")?
        .ok_or_else(|| failure("Canvas unavailable."))?
        .dyn_into()?)
}
fn canvas(
    document: &web_sys::Document,
    width: u32,
    height: u32,
) -> Result<web_sys::HtmlCanvasElement, JsValue> {
    let canvas = document
        .create_element("canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    canvas.set_width(width);
    canvas.set_height(height);
    Ok(canvas)
}
async fn delay(milliseconds: i32) -> Result<(), JsValue> {
    let promise = Promise::new(&mut |resolve, reject| {
        let result = web_sys::window()
            .ok_or_else(|| failure("Window unavailable."))
            .and_then(|window| {
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds)
            });
        if let Err(error) = result {
            let _ = reject.call1(&JsValue::UNDEFINED, &error);
        }
    });
    JsFuture::from(promise).await.map(|_| ())
}

struct Connection {
    port: web_sys::SerialPort,
    reader: web_sys::ReadableStreamDefaultReader,
    writer: web_sys::WritableStreamDefaultWriter,
    reply: RefCell<Option<(u32, js_sys::Function)>>,
    transcript: RefCell<String>,
    sequence: Cell<u32>,
    closed: Cell<bool>,
    reader_done: Cell<bool>,
}
impl Connection {
    async fn connect(
        port: Promise<web_sys::SerialPort>,
        document: web_sys::Document,
    ) -> Result<Rc<Self>, JsValue> {
        let port = JsFuture::from(port).await?;
        let options = web_sys::SerialOptions::new(115200);
        options.set_buffer_size(8192);
        JsFuture::from(port.open(&options)).await?;
        let reader = web_sys::ReadableStreamDefaultReader::new(&port.readable())?;
        let writer = port.writable().get_writer()?;
        let connection = Rc::new(Self {
            port,
            reader,
            writer,
            reply: RefCell::new(None),
            transcript: RefCell::new(String::new()),
            sequence: Cell::new(0),
            closed: Cell::new(false),
            reader_done: Cell::new(false),
        });
        let input = Rc::clone(&connection);
        spawn_local(async move {
            let mut line = Vec::new();
            loop {
                let result = match JsFuture::from(input.reader.read()).await {
                    Ok(result) => result,
                    Err(_) => break,
                };
                if Reflect::get(&result, &"done".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    != Some(false)
                {
                    break;
                }
                let bytes = match Reflect::get(&result, &"value".into())
                    .and_then(|v| v.dyn_into::<Uint8Array>())
                {
                    Ok(bytes) if bytes.length() <= 65536 => bytes.to_vec(),
                    _ => break,
                };
                for byte in bytes {
                    if byte == b'\n' {
                        if let Ok(text) = core::str::from_utf8(&line) {
                            let text = text.trim_end_matches('\r');
                            if let Some(shown) = text.strip_prefix("event=shown ") {
                                let image = shown
                                    .split_whitespace()
                                    .find_map(|field| {
                                        field
                                            .strip_prefix("image=")
                                            .and_then(|number| number.parse::<u8>().ok())
                                    })
                                    .and_then(ImageId::new);
                                if let Some(image) = image {
                                    message(
                                        &document,
                                        "connection-status",
                                        &format!("Showing Image {}.", image.number()),
                                    );
                                }
                            }
                            if text.starts_with("event=status ") {
                                if text.contains("refresh=Updating") {
                                    message(&document, "connection-status", "The display is updating. Full refresh can take a few moments.");
                                } else if text.contains("refresh=Failed") {
                                    message(&document, "connection-status", "The display could not update. Choose Refresh display to retry.");
                                } else if !element(&document, "connection-status")
                                    .ok()
                                    .and_then(|element| element.text_content())
                                    .is_some_and(|text| text.starts_with("Showing "))
                                {
                                    message(&document, "connection-status", "Display ready.");
                                }
                            }
                            if text == "event=display_complete" {
                                message(&document, "transfer-status", "Display updated.");
                                message(&document, "connection-status", "Display updated.");
                            }
                            if text == "event=display_failed" {
                                message(&document, "error", "The display could not update. Choose Refresh display to retry.");
                            }
                            let mut fields = text.split_whitespace();
                            if fields.next() == Some("reply") {
                                if let (Some(id), Some(outcome)) = (
                                    fields.next().and_then(|v| v.parse::<u32>().ok()),
                                    fields.next(),
                                ) {
                                    if fields.next().is_none() && matches!(outcome, "ok" | "error")
                                    {
                                        let matches = input
                                            .reply
                                            .borrow()
                                            .as_ref()
                                            .is_some_and(|(pending, _)| *pending == id);
                                        if matches {
                                            if let Some((_, resolve)) =
                                                input.reply.borrow_mut().take()
                                            {
                                                let _ = resolve.call1(
                                                    &JsValue::UNDEFINED,
                                                    &JsValue::from_bool(outcome == "ok"),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            if !text.starts_with("event=image_progress")
                                && !text.starts_with("reply ")
                            {
                                let mut transcript = input.transcript.borrow_mut();
                                if transcript.len() + text.len() > 16384 {
                                    transcript.clear();
                                }
                                transcript.push_str(text);
                                transcript.push('\n');
                                message(&document, "device-log", &transcript);
                            }
                        }
                        line.clear();
                    } else if line.len() < 2048 {
                        line.push(byte);
                    } else {
                        line.clear();
                        input.closed.set(true);
                        break;
                    }
                }
                if input.closed.get() {
                    break;
                }
            }
            input.closed.set(true);
            if let Some((_, resolve)) = input.reply.borrow_mut().take() {
                let _ = resolve.call1(&JsValue::UNDEFINED, &JsValue::FALSE);
            }
            input.reader.release_lock();
            input.reader_done.set(true);
            message(
                &document,
                "connection-status",
                "The display disconnected. Wake it and reconnect to continue.",
            );
        });
        Ok(connection)
    }
    async fn command(&self, command: &str) -> Result<(), JsValue> {
        if self.closed.get() {
            return Err(failure("Device disconnected."));
        }
        let id = self
            .sequence
            .get()
            .checked_add(1)
            .ok_or_else(|| failure("Reconnect to reset the command counter."))?;
        let line = format!("control {id} {command}\n");
        if line.len() > 257 || command.contains(['\r', '\n']) {
            return Err(failure(
                "Command exceeds 256 bytes or contains a line break.",
            ));
        }
        self.sequence.set(id);
        if self.reply.borrow().is_some() {
            return Err(failure("Another device command is pending."));
        }
        let reply = Promise::new(&mut |resolve, _| {
            *self.reply.borrow_mut() = Some((id, resolve));
        });
        let window = web_sys::window().ok_or_else(|| failure("Window unavailable."))?;
        let mut timer = None;
        let timeout = Promise::new(&mut |resolve, reject| match window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 180000)
        {
            Ok(handle) => timer = Some(handle),
            Err(error) => {
                let _ = reject.call1(&JsValue::UNDEFINED, &error);
            }
        });
        let written = self
            .writer
            .write_with_chunk(&Uint8Array::from(line.as_bytes()));
        let completion = Promise::all(&Array::of2(&written, &reply));
        let result = JsFuture::from(Promise::race(&Array::of2(&completion, &timeout))).await;
        if let Some(timer) = timer {
            window.clear_timeout_with_handle(timer);
        }
        self.reply.borrow_mut().take();
        match result?
            .dyn_into::<Array>()
            .ok()
            .and_then(|values| values.get(1).as_bool())
        {
            Some(true) => Ok(()),
            Some(false) => Err(failure(
                "The display rejected the request or disconnected. Wake it and try again.",
            )),
            None => Err(failure(
                "The display did not respond. Wake it and reconnect to try again.",
            )),
        }
    }
    async fn close(&self) -> Result<(), JsValue> {
        self.closed.set(true);
        if !self.reader_done.get() {
            let _ = JsFuture::from(self.reader.cancel()).await;
            while !self.reader_done.get() {
                delay(4).await?;
            }
        }
        self.writer.release_lock();
        JsFuture::from(self.port.close()).await.map(|_| ())
    }
}

#[derive(Clone, Copy)]
struct ImageOptions {
    rotation: Rotation,
    fit: Fit,
    anchor: Anchor,
    palette: Palette,
    dither: Dither,
    brightness: i16,
    contrast: u16,
    saturation: u16,
    invert: bool,
}
impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            rotation: Rotation::Portrait,
            fit: Fit::Contain,
            anchor: Anchor::Center,
            palette: Palette::SixColor,
            dither: Dither::Ordered,
            brightness: 0,
            contrast: 100,
            saturation: 100,
            invert: false,
        }
    }
}

#[derive(Clone)]
struct Source {
    size: Size,
    pixels: Rc<Vec<u8>>,
    options: ImageOptions,
    name: String,
}

struct Studio {
    sources: [Option<Source>; MAX_IMAGES],
    dirty: [bool; MAX_IMAGES],
    loading: Presentation<Option<ImageId>>,
    editing: Presentation<Option<ImageId>>,
    sequence: Option<u32>,
    removed: Option<(ImageId, Source, Vec<u8>)>,
    selected: ImageId,
    connection: Option<Rc<Connection>>,
    busy: bool,
    uploading: bool,
    cancelled: bool,
}
impl Default for Studio {
    fn default() -> Self {
        Self {
            sources: core::array::from_fn(|_| None),
            dirty: [false; MAX_IMAGES],
            loading: Presentation::new(None),
            editing: Presentation::new(None),
            sequence: Some(0),
            removed: None,
            selected: ImageId::default(),
            connection: None,
            busy: false,
            uploading: false,
            cancelled: false,
        }
    }
}
impl Studio {
    fn request(&mut self) -> Result<u32, JsValue> {
        let id = self
            .sequence
            .ok_or_else(|| failure("Save your images and reload the app to continue editing."))?;
        self.sequence = id.checked_add(1);
        Ok(id)
    }
    fn replace_source(&mut self, image: ImageId, source: Source) -> Result<(), JsValue> {
        let total = self
            .sources
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != image.index())
            .filter_map(|(_, source)| source.as_ref())
            .map(|source| source.pixels.len())
            .sum::<usize>()
            + source.pixels.len();
        if total > 128 * 1024 * 1024 {
            return Err(failure(
                "The image library is full. Remove an image or choose a smaller file.",
            ));
        }
        if self
            .removed
            .as_ref()
            .is_some_and(|(removed, _, _)| *removed == image)
        {
            self.removed = None;
        }
        self.sources[image.index()] = Some(source);
        self.dirty[image.index()] = true;
        Ok(())
    }
}

fn image_id(document: &web_sys::Document) -> Result<ImageId, JsValue> {
    ImageId::new(number(document, "image-slot")?)
        .ok_or_else(|| failure("Choose an image from 1 to 8."))
}

fn options(document: &web_sys::Document) -> Result<ImageOptions, JsValue> {
    Ok(ImageOptions {
        rotation: Rotation::from_code(number(document, "rotation")?)
            .ok_or_else(|| failure("Choose an orientation."))?,
        fit: Fit::from_code(number(document, "image-fit")?)
            .ok_or_else(|| failure("Choose an image fit."))?,
        anchor: Anchor::from_code(number(document, "image-anchor")?)
            .ok_or_else(|| failure("Choose an image position."))?,
        palette: match value(document, "image-palette")?.as_str() {
            "six" => Palette::SixColor,
            "mono" => Palette::Monochrome,
            _ => return Err(failure("Choose the image colors.")),
        },
        dither: match value(document, "image-dither")?.as_str() {
            "none" => Dither::None,
            "ordered" => Dither::Ordered,
            "diffusion" => Dither::FloydSteinberg,
            _ => return Err(failure("Choose a dithering method.")),
        },
        invert: element(document, "image-invert")?
            .dyn_into::<web_sys::HtmlInputElement>()?
            .checked(),
        brightness: numeric(document, "image-brightness", false)?,
        contrast: numeric(document, "image-contrast", false)?,
        saturation: numeric(document, "image-saturation", false)?,
    })
}

fn sync_options(document: &web_sys::Document, options: ImageOptions) -> Result<(), JsValue> {
    for (id, value) in [
        ("rotation", (options.rotation as u8).to_string()),
        ("image-fit", (options.fit as u8).to_string()),
        ("image-anchor", (options.anchor as u8).to_string()),
        (
            "image-palette",
            if options.palette == Palette::Monochrome {
                "mono"
            } else {
                "six"
            }
            .into(),
        ),
        (
            "image-dither",
            match options.dither {
                Dither::None => "none",
                Dither::Ordered => "ordered",
                Dither::FloydSteinberg => "diffusion",
            }
            .into(),
        ),
        ("image-brightness", options.brightness.to_string()),
        ("image-contrast", options.contrast.to_string()),
        ("image-saturation", options.saturation.to_string()),
    ] {
        let control = element(document, id)?;
        if let Some(input) = control.dyn_ref::<web_sys::HtmlInputElement>() {
            input.set_value(&value);
        } else {
            control
                .dyn_into::<web_sys::HtmlSelectElement>()?
                .set_value(&value);
        }
        element(document, id)?.remove_attribute("aria-invalid")?;
    }
    element(document, "image-invert")?
        .dyn_into::<web_sys::HtmlInputElement>()?
        .set_checked(options.invert);
    Ok(())
}

fn select_image(
    document: &web_sys::Document,
    studio: &mut Studio,
    preview: &mut WebPreview,
) -> Result<(), JsValue> {
    let previous = studio.selected;
    if studio.dirty[previous.index()] && studio.sources[previous.index()].is_some() {
        if let Ok(options) = options(document) {
            render_image(document, studio, preview, previous, options)?;
        }
        studio.dirty[previous.index()] = false;
    }
    studio.loading.cancel();
    studio.editing.cancel();
    let image = image_id(document)?;
    studio.selected = image;
    let source = studio.sources[image.index()].as_ref();
    sync_options(
        document,
        source.map_or_else(ImageOptions::default, |source| source.options),
    )?;
    message(
        document,
        "image-name",
        source.map_or("", |source| source.name.as_str()),
    );
    element(document, "image-file")?
        .dyn_into::<web_sys::HtmlInputElement>()?
        .set_value("");
    message(document, "media-error", "");
    preview.command(&format!("gallery select {}", image.number()))?;
    preview.command("screen gallery")?;
    update(document, preview)?;
    message(
        document,
        "media-status",
        if source.is_some() {
            "Preview ready."
        } else {
            "Choose a file to start."
        },
    );
    Ok(())
}

fn prepare(
    document: &web_sys::Document,
    studio: &mut Studio,
    preview: &mut WebPreview,
) -> Result<(), JsValue> {
    render_image(
        document,
        studio,
        preview,
        image_id(document)?,
        options(document)?,
    )
}

fn render_image(
    document: &web_sys::Document,
    studio: &mut Studio,
    preview: &mut WebPreview,
    id: ImageId,
    options: ImageOptions,
) -> Result<(), JsValue> {
    let source = studio.sources[id.index()]
        .as_ref()
        .ok_or_else(|| failure("Choose an image first."))?
        .clone();
    let rotation = options.rotation;
    let (w, h) = rotation.dimensions();
    let target = Size::new(w, h).ok_or_else(|| failure("Invalid target size."))?;
    let conversion = Conversion::new(
        options.palette,
        options.dither,
        options.brightness,
        options.contrast,
        options.saturation,
        options.invert,
    )
    .ok_or_else(|| {
        failure("Brightness must be -100 to 100; contrast and saturation must be 0 to 200.")
    })?;
    let mut pixels = vec![0x11; FRAME_BYTES];
    let mut errors = vec![[0; 3]; 2 * (usize::from(w) + 2)];
    RgbaImage::new(source.size, &source.pixels)
        .ok_or_else(|| failure("Invalid image dimensions."))?
        .render(
            &mut pixels,
            rotation,
            Placement::new(source.size, target, options.fit, options.anchor),
            conversion,
            &mut errors,
        )
        .map_err(|_| failure("Image conversion failed."))?;
    let mut file = ImageRecord::image(id, 0, rotation, crc32(&pixels))
        .encode()
        .to_vec();
    file.extend(pixels);
    preview.import_image(id.number(), &file)?;
    studio.sources[id.index()] = Some(Source { options, ..source });
    studio.dirty[id.index()] = false;
    update(document, preview)?;
    message(document, "media-status", "Preview updated.");
    message(document, "media-error", "");
    message(
        document,
        "image-name",
        &studio.sources[id.index()]
            .as_ref()
            .map_or_else(String::new, |source| source.name.clone()),
    );
    Ok(())
}

fn read_canvas(canvas: &web_sys::HtmlCanvasElement) -> Result<(Size, Vec<u8>), JsValue> {
    let width =
        u16::try_from(canvas.width()).map_err(|_| failure("Image width exceeds 16384 pixels."))?;
    let height = u16::try_from(canvas.height())
        .map_err(|_| failure("Image height exceeds 16384 pixels."))?;
    let size = Size::new(width, height)
        .ok_or_else(|| failure("Image exceeds 16 million pixels or has an empty dimension."))?;
    let pixels = context(canvas)?
        .get_image_data(0, 0, i32::from(width), i32::from(height))?
        .data()
        .0;
    Ok((size, pixels))
}

fn generated(document: &web_sys::Document, action: &str) -> Result<(Size, Vec<u8>), JsValue> {
    let rotation = Rotation::from_code(number(document, "rotation")?)
        .ok_or_else(|| failure("Choose an orientation."))?;
    let (width, height) = rotation.dimensions();
    let canvas = canvas(document, u32::from(width), u32::from(height))?;
    let context = context(&canvas)?;
    context.set_fill_style_str("white");
    context.fill_rect(0.0, 0.0, f64::from(width), f64::from(height));
    context.set_fill_style_str("black");
    let text = element(document, "poster-text")?
        .dyn_into::<web_sys::HtmlTextAreaElement>()?
        .value();
    match action {
        "make-qr" => {
            if text.is_empty() || text.len() > 2048 {
                return Err(failure("Enter 1 to 2048 UTF-8 bytes for the QR code."));
            }
            let qr = qrcodegen::QrCode::encode_text(&text, qrcodegen::QrCodeEcc::Medium)
                .map_err(|_| failure("QR content is too long."))?;
            let scale = (i32::from(width.min(height)) - 128) / (qr.size() + 8);
            if scale < 4 {
                return Err(failure("QR modules are too small."));
            }
            let left = (i32::from(width) - qr.size() * scale) / 2;
            let top = (i32::from(height) - qr.size() * scale) / 2;
            for y in 0..qr.size() {
                for x in 0..qr.size() {
                    if qr.get_module(x, y) {
                        context.fill_rect(
                            f64::from(left + x * scale),
                            f64::from(top + y * scale),
                            f64::from(scale),
                            f64::from(scale),
                        );
                    }
                }
            }
        }
        "make-text" => {
            if text.is_empty() || text.chars().count() > 2000 {
                return Err(failure("Enter 1 to 2000 characters for the text image."));
            }
            let size: u16 = number(document, "poster-size")?;
            if !(16..=160).contains(&size) {
                return Err(failure("Choose text size 16 to 160 pixels."));
            }
            context.set_font(&format!("{size}px sans-serif"));
            context.set_text_baseline("top");
            let mut line = String::new();
            let mut y = 64.0;
            let line_height = f64::from(size) * 1.4;
            for ch in text.chars().chain(core::iter::once('\n')) {
                if ch != '\n' && ch != '\r' {
                    line.push(ch);
                }
                if ch == '\n' || context.measure_text(&line)?.width() > f64::from(width) - 128.0 {
                    let carry = if ch != '\n' { line.pop() } else { None };
                    if y + line_height > f64::from(height) - 64.0 {
                        return Err(failure(
                            "Text does not fit. Reduce text size or shorten the text.",
                        ));
                    }
                    context.fill_text(&line, 64.0, y)?;
                    y += line_height;
                    line.clear();
                    if let Some(ch) = carry {
                        line.push(ch);
                    }
                }
            }
        }
        "make-palette" => {
            context.set_font("32px sans-serif");
            for (i, (color, name)) in Color::ALL
                .into_iter()
                .zip(["BLACK", "WHITE", "YELLOW", "RED", "BLUE", "GREEN"])
                .enumerate()
            {
                let [r, g, b] = color.rgb();
                let y = i as f64 * f64::from(height) / 6.0;
                context.set_fill_style_str(&format!("rgb({r},{g},{b})"));
                context.fill_rect(
                    64.0,
                    y + 64.0,
                    f64::from(width) - 128.0,
                    f64::from(height) / 6.0 - 96.0,
                );
                context.set_fill_style_str("black");
                context.fill_text(name, 64.0, y + 44.0)?;
            }
        }
        _ => return Err(failure("Unknown image source.")),
    }
    read_canvas(&canvas)
}

fn download(document: &web_sys::Document, bytes: &[u8], filename: &str) -> Result<(), JsValue> {
    let parts = Array::new();
    parts.push(&Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)?;
    download_blob(document, &blob, filename)
}

fn download_blob(
    document: &web_sys::Document,
    blob: &web_sys::Blob,
    filename: &str,
) -> Result<(), JsValue> {
    let url = web_sys::Url::create_object_url_with_blob(blob)?;
    let anchor = document
        .create_element("a")?
        .dyn_into::<web_sys::HtmlAnchorElement>()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url)
}

async fn download_png(
    document: &web_sys::Document,
    prepared: &PreparedUpload,
) -> Result<(), JsValue> {
    let image = PackedFrame::from_file(&prepared.bytes)
        .ok_or_else(|| failure("The image could not be saved. Choose it again and retry."))?;
    let canvas = canvas(document, 1200, 1600)?;
    super::draw_frame(canvas.clone(), image.bytes())?;
    let promise = Promise::new(&mut |resolve, reject| {
        if let Err(error) = canvas.to_blob_with_type(&resolve, "image/png") {
            let _ = reject.call1(&JsValue::UNDEFINED, &error);
        }
    });
    let blob = JsFuture::from(promise).await?.dyn_into::<web_sys::Blob>()?;
    download_blob(
        document,
        &blob,
        &format!("IMAGE{}.png", prepared.image.number()),
    )
}

fn serial_supported() -> bool {
    web_sys::window()
        .is_some_and(|window| Reflect::has(&window.navigator(), &"serial".into()).unwrap_or(false))
}

fn enabled(document: &web_sys::Document, id: &str, enabled: bool) -> Result<(), JsValue> {
    if enabled {
        element(document, id)?.remove_attribute("disabled")
    } else {
        element(document, id)?.set_attribute("disabled", "")
    }
}

fn refresh_actions(
    document: &web_sys::Document,
    studio: &Studio,
    preview: &WebPreview,
) -> Result<(), JsValue> {
    let image = image_id(document)?;
    let ready = preview.export_image(image.number()).is_some()
        && !studio.dirty[image.index()]
        && studio.loading.active_id().is_none();
    let connected = studio
        .connection
        .as_ref()
        .is_some_and(|connection| !connection.closed.get());
    let available = !studio.busy;
    enabled(document, "export-image", ready)?;
    enabled(document, "export-prepared", ready)?;
    enabled(
        document,
        "send-image",
        ready && available && serial_supported(),
    )?;
    enabled(document, "send-settings", available && serial_supported())?;
    enabled(
        document,
        "device-connect",
        available && !connected && serial_supported(),
    )?;
    enabled(
        document,
        "device-disconnect",
        available && studio.connection.is_some(),
    )?;
    for id in [
        "device-status",
        "device-refresh",
        "device-sleep",
        "device-image",
        "device-import",
    ] {
        enabled(document, id, connected && available)?;
    }
    enabled(
        document,
        "delete-image",
        studio.sources[image.index()].is_some(),
    )?;
    workspace::visible(document, "cancel-upload", studio.uploading)?;
    workspace::visible(document, "undo-delete", studio.removed.is_some())?;
    element(document, "display")?.set_attribute(
        "aria-busy",
        if studio.editing.active_id().is_some() || studio.loading.active_id().is_some() {
            "true"
        } else {
            "false"
        },
    )?;
    message(
        document,
        "connection-summary",
        if connected {
            "Display connected"
        } else {
            "No display connected"
        },
    );
    if !serial_supported() {
        message(document, "connection-help", "Use Chrome or Edge on localhost or HTTPS to connect a display. Image editing and saving remain available.");
    }
    Ok(())
}

fn report_error(document: &web_sys::Document, result: Result<(), JsValue>) {
    message(
        document,
        "media-error",
        &result
            .err()
            .map(|error| {
                error
                    .as_string()
                    .unwrap_or_else(|| "The operation failed. Try again.".into())
            })
            .unwrap_or_default(),
    );
}

fn queue_edit(
    document: web_sys::Document,
    studio: Rc<RefCell<Studio>>,
    preview: Rc<RefCell<WebPreview>>,
) -> Result<(), JsValue> {
    let image = image_id(&document)?;
    let request = {
        let mut studio = studio.borrow_mut();
        let request = studio.request()?;
        studio.editing.start(request, Some(image));
        studio.dirty[image.index()] = studio.sources[image.index()].is_some();
        request
    };
    if studio.borrow().sources[image.index()].is_some() {
        message(&document, "media-status", "Updating preview…");
    }
    refresh_actions(&document, &studio.borrow(), &preview.borrow())?;
    spawn_local(async move {
        if let Err(error) = delay(120).await {
            report_error(&document, Err(error));
            return;
        }
        if studio.borrow().editing.active_id() != Some(request) {
            return;
        }
        if studio.borrow().loading.active_id().is_some() {
            return;
        }
        let result = if studio.borrow().sources[image.index()].is_some() {
            prepare(
                &document,
                &mut studio.borrow_mut(),
                &mut preview.borrow_mut(),
            )
        } else {
            Ok(())
        };
        studio.borrow_mut().editing.cancel();
        report_error(&document, result);
        if let Err(error) = refresh_actions(&document, &studio.borrow(), &preview.borrow()) {
            report_error(&document, Err(error));
        }
    });
    Ok(())
}

struct LoadJob {
    id: u32,
    image: ImageId,
    file: web_sys::File,
}
struct PreparedUpload {
    image: ImageId,
    bytes: Vec<u8>,
    slideshow: Option<device_core::media::SlideInterval>,
}
enum PreparedAction {
    Image(PreparedUpload),
    Daily {
        settings: Box<device_core::settings::Settings>,
        screen: String,
    },
}

pub fn mount(document: web_sys::Document, preview: Rc<RefCell<WebPreview>>) -> Result<(), JsValue> {
    let studio = Rc::new(RefCell::new(Studio::default()));
    refresh_actions(&document, &studio.borrow(), &preview.borrow())?;
    for (id, event_name) in [
        ("rotation", "change"),
        ("image-fit", "change"),
        ("image-anchor", "change"),
        ("image-palette", "change"),
        ("image-dither", "change"),
        ("image-invert", "change"),
        ("image-brightness", "input"),
        ("image-contrast", "input"),
        ("image-saturation", "input"),
    ] {
        let control = element(&document, id)?;
        let document = document.clone();
        let studio = Rc::clone(&studio);
        let preview = Rc::clone(&preview);
        let handler = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let result = queue_edit(document.clone(), Rc::clone(&studio), Rc::clone(&preview));
            if result.is_err() {
                report_error(&document, result);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        control.add_event_listener_with_callback(event_name, handler.as_ref().unchecked_ref())?;
        HANDLERS.with(|handlers| handlers.borrow_mut().push(handler as EventHandler));
    }
    for (id, event_name) in [
        ("image-file", "change"),
        ("image-slot", "change"),
        ("reset-image", "click"),
        ("make-text", "click"),
        ("make-qr", "click"),
        ("make-palette", "click"),
        ("export-image", "click"),
        ("export-prepared", "click"),
        ("delete-image", "click"),
        ("undo-delete", "click"),
        ("device-connect", "click"),
        ("device-disconnect", "click"),
        ("send-image", "click"),
        ("cancel-upload", "click"),
        ("device-status", "click"),
        ("device-refresh", "click"),
        ("device-sleep", "click"),
        ("device-image", "click"),
        ("device-import", "click"),
        ("send-settings", "click"),
        ("device-command-form", "submit"),
        ("apply-slideshow", "click"),
    ] {
        let control = element(&document, id)?;
        let document = document.clone();
        let preview = Rc::clone(&preview);
        let studio = Rc::clone(&studio);
        let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
            event.prevent_default();
            if id == "cancel-upload" {
                if !studio.borrow().uploading {
                    message(&document, "transfer-status", "No image is being sent.");
                    return;
                }
                studio.borrow_mut().cancelled = true;
                message(&document, "transfer-status", "Cancelling…");
                return;
            }
            let device_action =
                id.starts_with("device-") || id.starts_with("send-") || id == "apply-slideshow";
            if device_action && studio.borrow().busy {
                message(
                    &document,
                    "media-error",
                    "Wait for sending to finish, or choose Cancel sending.",
                );
                return;
            }
            let setup = (|| -> Result<(Option<LoadJob>, Option<PreparedAction>), JsValue> {
                let image = image_id(&document)?;
                let load = if id == "image-file" {
                    let file = element(&document, "image-file")?
                        .dyn_into::<web_sys::HtmlInputElement>()?
                        .files()
                        .and_then(|files| files.get(0));
                    let Some(file) = file else {
                        return Ok((None, None));
                    };
                    let mut state = studio.borrow_mut();
                    let request = state.request()?;
                    state.loading.start(request, Some(image));
                    state.editing.cancel();
                    message(&document, "media-status", "Opening image…");
                    Some(LoadJob {
                        id: request,
                        image,
                        file,
                    })
                } else {
                    None
                };
                let upload = if matches!(id, "send-image" | "export-image" | "export-prepared") {
                    studio.borrow_mut().editing.cancel();
                    if studio.borrow().dirty[image.index()] {
                        prepare(
                            &document,
                            &mut studio.borrow_mut(),
                            &mut preview.borrow_mut(),
                        )?;
                    }
                    let bytes = preview
                        .borrow()
                        .export_image(image.number())
                        .ok_or_else(|| failure("Choose an image first."))?;
                    Some(PreparedAction::Image(PreparedUpload {
                        image,
                        bytes,
                        slideshow: preview.borrow().inner.app().gallery.settings().slideshow,
                    }))
                } else if id == "send-settings" {
                    let settings = super::apply_daily_drafts(&document, &mut preview.borrow_mut())?;
                    Some(PreparedAction::Daily {
                        settings: Box::new(settings),
                        screen: value(&document, "daily-screen")?,
                    })
                } else {
                    None
                };
                Ok((load, upload))
            })();
            let (load, upload) = match setup {
                Ok(value) => value,
                Err(error) => {
                    report_error(&document, Err(error));
                    return;
                }
            };
            if id == "image-file" && load.is_none() {
                return;
            }
            if device_action {
                studio.borrow_mut().busy = true;
            }
            if id == "send-image" {
                message(&document, "media-error", "");
            }
            let connected = studio
                .borrow()
                .connection
                .as_ref()
                .is_some_and(|connection| !connection.closed.get());
            let port = if matches!(id, "device-connect" | "send-image" | "send-settings")
                && !connected
                && serial_supported()
            {
                web_sys::window().map(|window| window.navigator().serial().request_port())
            } else {
                None
            };
            let document = document.clone();
            let preview = Rc::clone(&preview);
            let studio = Rc::clone(&studio);
            let request = load.as_ref().map(|load| load.id);
            if let Err(error) = refresh_actions(&document, &studio.borrow(), &preview.borrow()) {
                report_error(&document, Err(error));
            }
            spawn_local(async move {
                let result = perform(id, &document, &studio, &preview, port, load, upload).await;
                if let Some(request) = request {
                    if studio.borrow().loading.active_id() != Some(request) {
                        return;
                    }
                    studio.borrow_mut().loading.cancel();
                    studio.borrow_mut().editing.cancel();
                }
                if device_action {
                    studio.borrow_mut().busy = false;
                    studio.borrow_mut().uploading = false;
                    if let Err(error) = &result {
                        let text = error.as_string().unwrap_or_else(|| {
                            "The display could not complete the operation. Try again.".into()
                        });
                        message(&document, "error", &text);
                        if id == "send-settings" {
                            message(&document, "daily-send-status", &text);
                        }
                    } else {
                        message(&document, "error", "");
                    }
                }
                report_error(&document, result);
                if let Err(error) = refresh_actions(&document, &studio.borrow(), &preview.borrow())
                {
                    report_error(&document, Err(error));
                }
            });
        }) as Box<dyn FnMut(web_sys::Event)>);
        control.add_event_listener_with_callback(event_name, handler.as_ref().unchecked_ref())?;
        HANDLERS.with(|handlers| handlers.borrow_mut().push(handler as EventHandler));
    }
    Ok(())
}

async fn perform(
    id: &str,
    document: &web_sys::Document,
    studio: &Rc<RefCell<Studio>>,
    preview: &Rc<RefCell<WebPreview>>,
    port: Option<Promise<web_sys::SerialPort>>,
    load: Option<LoadJob>,
    upload: Option<PreparedAction>,
) -> Result<(), JsValue> {
    if let Some(port) = port {
        let old = studio.borrow_mut().connection.take();
        if let Some(old) = old {
            old.close().await?;
        }
        let connection = Connection::connect(port, document.clone()).await?;
        studio.borrow_mut().connection = Some(Rc::clone(&connection));
        message(document, "connection-status", "Display connected.");
        connection.command("awake 600").await?;
        connection.command("status").await?;
    }
    match id {
        "image-file" => {
            let LoadJob {
                id: request,
                image: target,
                file,
            } = load.ok_or_else(|| failure("Choose an image file."))?;
            if file.size() > 20.0 * 1024.0 * 1024.0 {
                return Err(failure("Choose an image no larger than 20 MiB."));
            }
            if file.name().to_ascii_lowercase().ends_with(".e1004-image")
                || file.name().to_ascii_lowercase().ends_with(".epd")
            {
                let bytes = Uint8Array::new(&JsFuture::from(file.array_buffer()).await?).to_vec();
                if studio.borrow().loading.active_id() != Some(request) {
                    return Ok(());
                }
                let packed = PackedFrame::from_file(&bytes).ok_or_else(|| {
                    failure("The saved image is damaged or incomplete. Choose another file.")
                })?;
                let (width, height) = packed.rotation().dimensions();
                let size =
                    Size::new(width, height).ok_or_else(|| failure("Invalid image dimensions."))?;
                let mut pixels = Vec::with_capacity(FRAME_BYTES * 8);
                for y in 0..height {
                    for x in 0..width {
                        let color = packed
                            .pixel(x, y)
                            .ok_or_else(|| failure("Invalid image pixel."))?;
                        pixels.extend(color.rgb());
                        pixels.push(255);
                    }
                }
                let options = ImageOptions {
                    rotation: packed.rotation(),
                    dither: Dither::None,
                    ..Default::default()
                };
                studio.borrow_mut().replace_source(
                    target,
                    Source {
                        size,
                        pixels: Rc::new(pixels),
                        options,
                        name: file.name(),
                    },
                )?;
                preview.borrow_mut().import_image(target.number(), &bytes)?;
                studio.borrow_mut().dirty[target.index()] = false;
                sync_options(document, options)?;
                message(document, "image-name", &file.name());
                update(document, &preview.borrow())?;
                message(document, "media-status", "Preview ready.");
            } else {
                if !matches!(
                    file.type_().as_str(),
                    "image/png" | "image/jpeg" | "image/webp" | "image/bmp" | "image/gif"
                ) {
                    return Err(failure(
                        "Choose PNG, JPEG, WebP, BMP, GIF, or an .e1004-image file.",
                    ));
                }
                let bitmap = JsFuture::from(
                    web_sys::window()
                        .ok_or_else(|| failure("Window unavailable."))?
                        .create_image_bitmap_with_blob(&file)?,
                )
                .await?
                .dyn_into::<web_sys::ImageBitmap>()?;
                if studio.borrow().loading.active_id() != Some(request) {
                    bitmap.close();
                    return Ok(());
                }
                if bitmap.width() > 16384
                    || bitmap.height() > 16384
                    || u64::from(bitmap.width()) * u64::from(bitmap.height()) > 16777216
                {
                    bitmap.close();
                    return Err(failure(
                        "Image exceeds 16384 pixels per side or 16 million pixels.",
                    ));
                }
                let canvas = canvas(document, bitmap.width(), bitmap.height())?;
                let rendered = context(&canvas)?.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0);
                bitmap.close();
                rendered?;
                let (size, pixels) = read_canvas(&canvas)?;
                let options = studio.borrow().sources[target.index()]
                    .as_ref()
                    .map_or_else(ImageOptions::default, |source| source.options);
                studio.borrow_mut().replace_source(
                    target,
                    Source {
                        size,
                        pixels: Rc::new(pixels),
                        options,
                        name: file.name(),
                    },
                )?;
                prepare(
                    document,
                    &mut studio.borrow_mut(),
                    &mut preview.borrow_mut(),
                )?;
            }
        }
        "reset-image" => {
            studio.borrow_mut().loading.cancel();
            studio.borrow_mut().editing.cancel();
            sync_options(document, ImageOptions::default())?;
            let image = image_id(document)?;
            if studio.borrow().sources[image.index()].is_some() {
                prepare(
                    document,
                    &mut studio.borrow_mut(),
                    &mut preview.borrow_mut(),
                )?;
            }
        }
        "make-text" | "make-qr" | "make-palette" => {
            let image = image_id(document)?;
            let (size, pixels) = generated(document, id)?;
            studio.borrow_mut().loading.cancel();
            studio.borrow_mut().editing.cancel();
            studio.borrow_mut().replace_source(
                image,
                Source {
                    size,
                    pixels: Rc::new(pixels),
                    options: ImageOptions::default(),
                    name: match id {
                        "make-text" => "Text image",
                        "make-qr" => "QR code",
                        _ => "Color palette",
                    }
                    .into(),
                },
            )?;
            prepare(
                document,
                &mut studio.borrow_mut(),
                &mut preview.borrow_mut(),
            )?;
        }
        "image-slot" => {
            select_image(
                document,
                &mut studio.borrow_mut(),
                &mut preview.borrow_mut(),
            )?;
        }
        "export-image" => {
            let Some(PreparedAction::Image(upload)) = upload else {
                return Err(failure("Choose an image first."));
            };
            download_png(document, &upload).await?;
        }
        "export-prepared" => {
            let Some(PreparedAction::Image(upload)) = upload else {
                return Err(failure("Choose an image first."));
            };
            download(
                document,
                &upload.bytes,
                &format!("IMAGE{}.EPD", upload.image.number()),
            )?;
        }
        "delete-image" => {
            let image = image_id(document)?;
            let bytes = preview.borrow().export_image(image.number());
            let source = studio.borrow_mut().sources[image.index()].take();
            if let (Some(source), Some(bytes)) = (source, bytes) {
                studio.borrow_mut().removed = Some((image, source, bytes));
            }
            studio.borrow_mut().dirty[image.index()] = false;
            preview.borrow_mut().delete_image(image.number())?;
            select_image(
                document,
                &mut studio.borrow_mut(),
                &mut preview.borrow_mut(),
            )?;
            update(document, &preview.borrow())?;
            message(
                document,
                "media-status",
                "Image removed. Choose Undo removal to restore it.",
            );
        }
        "undo-delete" => {
            let (image, source, bytes) = studio
                .borrow()
                .removed
                .as_ref()
                .cloned()
                .ok_or_else(|| failure("No image to restore."))?;
            studio.borrow_mut().replace_source(image, source)?;
            studio.borrow_mut().dirty[image.index()] = false;
            preview.borrow_mut().import_image(image.number(), &bytes)?;
            element(document, "image-slot")?
                .dyn_into::<web_sys::HtmlSelectElement>()?
                .set_value(&image.number().to_string());
            select_image(
                document,
                &mut studio.borrow_mut(),
                &mut preview.borrow_mut(),
            )?;
            message(document, "media-status", "Image restored.");
        }
        "apply-slideshow" => {
            let command = format!(
                "gallery slideshow {}",
                value(document, "slideshow-seconds")?
            );
            preview.borrow_mut().command(&command)?;
            update(document, &preview.borrow())?;
            let connection = studio.borrow().connection.clone();
            if let Some(connection) = connection.filter(|connection| !connection.closed.get()) {
                connection.command("awake 600").await?;
                connection.command(&command).await?;
                connection.command("resume last").await?;
                connection.command("screen gallery").await?;
                message(document, "media-status", "Slideshow saved on the display.");
            } else {
                message(
                    document,
                    "media-status",
                    "Slideshow saved here. Connect your display to apply it.",
                );
            }
        }
        "device-connect" => {
            if studio.borrow().connection.is_none() {
                return Err(failure("Use Chrome or Edge to connect your display."));
            }
        }
        "device-disconnect" => {
            let connection = studio
                .borrow_mut()
                .connection
                .take()
                .ok_or_else(|| failure("No device is connected."))?;
            if !connection.closed.get() {
                let _ = connection.command("awake 0").await;
            }
            connection.close().await?;
            message(document, "connection-status", "Display disconnected.");
        }
        _ => {
            let connection = studio
                .borrow()
                .connection
                .clone()
                .ok_or_else(|| failure("Connect a USB device first."))?;
            connection.command("awake 600").await?;
            match id {
                "send-image" => {
                    let Some(PreparedAction::Image(prepared)) = upload else {
                        return Err(failure("Choose an image first."));
                    };
                    let slot = prepared.image.number();
                    let image = PackedFrame::from_file(&prepared.bytes)
                        .ok_or_else(|| failure("Invalid prepared image."))?;
                    let transfer = connection
                        .sequence
                        .get()
                        .checked_add(1)
                        .ok_or_else(|| failure("Reconnect the device."))?;
                    studio.borrow_mut().cancelled = false;
                    studio.borrow_mut().uploading = true;
                    refresh_actions(document, &studio.borrow(), &preview.borrow())?;
                    connection
                        .command(&format!(
                            "image begin {transfer} {slot} {} {}",
                            image.rotation() as u8,
                            crc32(image.bytes())
                        ))
                        .await?;
                    let upload = async {
                        for (index, chunk) in image.bytes().chunks(64).enumerate() {
                            if studio.borrow().cancelled {
                                return Err(failure(
                                    "Sending cancelled. Your saved display image is unchanged.",
                                ));
                            }
                            let offset = index * 64;
                            let mut command = format!("image data {transfer} {offset} ");
                            for byte in chunk {
                                write!(command, "{byte:02x}")
                                    .map_err(|_| failure("Could not encode image data."))?;
                            }
                            connection.command(&command).await?;
                            if index % 128 == 0 {
                                message(
                                    document,
                                    "transfer-status",
                                    &format!(
                                        "Sending image: {}%. Keep the display connected.",
                                        offset * 100 / FRAME_BYTES
                                    ),
                                );
                            }
                        }
                        if studio.borrow().cancelled {
                            return Err(failure(
                                "Sending cancelled. Your saved display image is unchanged.",
                            ));
                        }
                        studio.borrow_mut().uploading = false;
                        refresh_actions(document, &studio.borrow(), &preview.borrow())?;
                        message(document, "transfer-status", "Saving image on the display…");
                        connection
                            .command(&format!("image commit {transfer}"))
                            .await
                    }
                    .await;
                    if let Err(error) = upload {
                        let _ = connection.command(&format!("image abort {transfer}")).await;
                        return Err(error);
                    }
                    connection
                        .command(&format!("rotation {}", image.rotation() as u8))
                        .await?;
                    connection
                        .command(&format!(
                            "gallery slideshow {}",
                            prepared.slideshow.map_or_else(
                                || "off".into(),
                                |interval| interval.seconds().to_string()
                            )
                        ))
                        .await?;
                    connection.command("resume last").await?;
                    connection
                        .command(&format!("gallery select {slot}"))
                        .await?;
                    connection.command("screen gallery").await?;
                    let newer = image_id(document).ok() == Some(prepared.image)
                        && preview.borrow().export_image(slot).is_some_and(|file| {
                            PackedFrame::from_file(&file).is_some_and(|current| {
                                current.rotation() != image.rotation()
                                    || current.bytes() != image.bytes()
                            })
                        });
                    message(
                        document,
                        "transfer-status",
                        if newer {
                            "Image sent. Send again to include your newer edits."
                        } else {
                            "Image sent. The display is updating; allow a few moments for the full refresh."
                        },
                    );
                }
                "send-settings" => {
                    let Some(PreparedAction::Daily {
                        settings: app,
                        screen,
                    }) = upload
                    else {
                        return Err(failure("Check the daily content and try sending again."));
                    };
                    for command in [
                        format!("rotation {}", app.rotation),
                        format!("interval {}", app.refresh_seconds),
                        format!(
                            "gallery slideshow {}",
                            app.gallery.slideshow.map_or_else(
                                || "off".into(),
                                |interval| interval.seconds().to_string()
                            )
                        ),
                    ] {
                        connection.command(&command).await?;
                    }
                    for (i, task) in app.daily.tasks().iter().enumerate() {
                        connection
                            .command(&if task.text.is_empty() {
                                format!("task clear {}", i + 1)
                            } else {
                                format!("task set {} {}", i + 1, task.text.as_str())
                            })
                            .await?;
                        if task.done {
                            connection.command(&format!("task done {}", i + 1)).await?;
                        }
                    }
                    connection
                        .command(&if app.daily.note().is_empty() {
                            "note clear".into()
                        } else {
                            format!("note set {}", app.daily.note().as_str())
                        })
                        .await?;
                    connection.command("resume last").await?;
                    connection.command(&format!("screen {screen}")).await?;
                    message(
                        document,
                        "daily-send-status",
                        "Content sent. The display is updating.",
                    );
                }
                "device-status" => {
                    message(document, "connection-status", "Checking display…");
                    connection.command("status").await?;
                    if element(document, "connection-status")?
                        .text_content()
                        .as_deref()
                        == Some("Checking display…")
                    {
                        message(document, "connection-status", "Display responded.");
                    }
                }
                "device-image" => {
                    connection
                        .command(&format!(
                            "gallery select {}",
                            value(document, "device-slot")?
                        ))
                        .await?;
                    connection.command("screen gallery").await?;
                    message(
                        document,
                        "connection-status",
                        "Image selected. The display is updating.",
                    );
                }
                "device-import" => {
                    connection
                        .command(&format!("image import {}", value(document, "device-slot")?))
                        .await?;
                    message(
                        document,
                        "connection-status",
                        "Image imported from the card. The display is updating.",
                    );
                }
                "device-refresh" => {
                    connection.command("refresh").await?;
                    message(
                        document,
                        "connection-status",
                        "Refresh requested. The display is updating.",
                    );
                }
                "device-sleep" => {
                    connection.command("awake 0").await?;
                    connection.command("sleep").await?;
                    message(
                        document,
                        "connection-status",
                        "Sleep requested. Press a display button to wake it.",
                    );
                }
                "device-command-form" => {
                    connection
                        .command(&value(document, "device-command")?)
                        .await?;
                    message(
                        document,
                        "connection-status",
                        "Command accepted. Read Device output for state and display results.",
                    );
                }
                _ => return Err(failure("Unknown device action.")),
            }
        }
    }
    Ok(())
}
