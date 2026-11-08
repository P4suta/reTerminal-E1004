use device_core::{
    app::{RefreshState, REFRESH_INTERVAL_MS},
    geometry::{HEIGHT, WIDTH},
};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
mod media;
mod workspace;

#[wasm_bindgen]
pub struct WebPreview {
    inner: device_preview::Preview,
}

#[wasm_bindgen]
impl WebPreview {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: device_preview::Preview::default(),
        }
    }
    pub fn command(&mut self, text: &str) -> Result<(), JsValue> {
        self.inner.command(text).map_err(JsValue::from_str)
    }
    pub fn replay(&mut self, text: &str) -> Result<(), JsValue> {
        self.inner
            .replay(text)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn text(&self) -> String {
        self.inner.text()
    }
    pub fn recording(&self) -> String {
        self.inner.recording()
    }
    pub fn frame(&self) -> Vec<u8> {
        self.inner.frame()
    }
    pub fn import_image(&mut self, number: u8, file: &[u8]) -> Result<(), JsValue> {
        self.inner
            .import_image(number, file)
            .map_err(JsValue::from_str)
    }
    pub fn export_image(&self, number: u8) -> Option<Vec<u8>> {
        self.inner.export_image(number)
    }
    pub fn delete_image(&mut self, number: u8) -> Result<(), JsValue> {
        self.inner.delete_image(number).map_err(JsValue::from_str)
    }
    pub fn draw(&self, canvas: web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
        draw_frame(canvas, &self.inner.frame())
    }
}

fn draw_frame(canvas: web_sys::HtmlCanvasElement, frame: &[u8]) -> Result<(), JsValue> {
    let context = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("canvas context unavailable"))?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
    let pixels = device_preview::rgba(frame);
    let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        wasm_bindgen::Clamped(&pixels),
        u32::from(WIDTH),
        u32::from(HEIGHT),
    )?;
    #[cfg(target_arch = "wasm32")]
    let origin = 0;
    #[cfg(not(target_arch = "wasm32"))]
    let origin = 0.0;
    context.put_image_data(&image, origin, origin)
}

impl Default for WebPreview {
    fn default() -> Self {
        Self::new()
    }
}

type EventHandler = Closure<dyn FnMut(web_sys::Event)>;
thread_local! { static HANDLERS: RefCell<Vec<EventHandler>> = RefCell::new(Vec::new()); }

fn element(document: &web_sys::Document, id: &str) -> Result<web_sys::Element, JsValue> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str("missing preview element"))
}

fn update(document: &web_sys::Document, preview: &WebPreview) -> Result<(), JsValue> {
    preview.draw(element(document, "display")?.dyn_into()?)?;
    element(document, "state")?.set_text_content(Some(&preview.text()));
    let app = preview.inner.app();
    let screen = match app.screen {
        device_core::app::ScreenId::Status => "status",
        device_core::app::ScreenId::Calendar => "calendar",
        device_core::app::ScreenId::Checklist => "checklist",
        device_core::app::ScreenId::Notes => "notes",
        device_core::app::ScreenId::Diagnostics => "diagnostics",
        device_core::app::ScreenId::Drawing => "drawing",
        device_core::app::ScreenId::Gallery => "gallery",
    };
    element(document, "screen")?
        .dyn_into::<web_sys::HtmlSelectElement>()?
        .set_value(screen);
    element(document, "refresh")?.set_text_content(Some(match app.middle_label() {
        "TODAY" => "Today",
        "DONE" => "Mark done",
        "REOPEN" => "Reopen task",
        "START" => "Start slideshow",
        "PAUSE" => "Pause slideshow",
        _ => "Refresh",
    }));
    element(document, "gesture-help")?.set_text_content(Some(match app.screen {
        device_core::app::ScreenId::Calendar => "Hold side buttons to browse months. Middle returns to today. Hold middle to measure and refresh.",
        device_core::app::ScreenId::Checklist => "Hold side buttons to select a task. Middle marks it done or reopens it. Hold middle to measure and refresh.",
        device_core::app::ScreenId::Gallery => "Side buttons change images. Middle starts or pauses the slideshow. Hold a side button to change screens.",
        device_core::app::ScreenId::Status | device_core::app::ScreenId::Notes | device_core::app::ScreenId::Diagnostics | device_core::app::ScreenId::Drawing => "Side buttons change screens. Hold middle to measure and refresh from any screen.",
    }));
    let contextual = matches!(
        app.screen,
        device_core::app::ScreenId::Checklist | device_core::app::ScreenId::Gallery
    ) || (app.screen == device_core::app::ScreenId::Calendar
        && app.calendar().is_some());
    for id in ["hold-previous", "hold-next"] {
        if contextual {
            element(document, id)?.remove_attribute("disabled")?;
        } else {
            element(document, id)?.set_attribute("disabled", "")?;
        }
    }
    element(document, "update-status")?.set_text_content(Some(preview.inner.display_status()));
    element(document, "wait")?
        .dyn_into::<web_sys::HtmlInputElement>()?
        .set_checked(!preview.inner.immediate());
    element(document, "immediate")?
        .dyn_into::<web_sys::HtmlInputElement>()?
        .set_checked(preview.inner.immediate());
    let updating = matches!(preview.inner.app().refresh, RefreshState::Updating { .. });
    element(document, "completion-help")?.set_text_content(Some(if updating {
        "An update is waiting. Choose Complete update or Simulate panel timeout."
    } else if preview.inner.immediate() {
        "Choose Manual completion, then Hold middle, to pause an update."
    } else {
        "No update is waiting. Choose Hold middle to start one."
    }));
    for id in ["complete", "timeout"] {
        let control = element(document, id)?;
        if updating {
            control.remove_attribute("disabled")?;
        } else {
            control.set_attribute("disabled", "")?;
        }
    }
    update_task_controls(document, preview)?;
    if matches!(
        app.screen,
        device_core::app::ScreenId::Calendar
            | device_core::app::ScreenId::Checklist
            | device_core::app::ScreenId::Notes
    ) {
        element(document, "daily-screen")?
            .dyn_into::<web_sys::HtmlSelectElement>()?
            .set_value(screen);
    }
    workspace::refresh(document, preview)
}

const MEASUREMENT_FIELDS: [&str; 8] = [
    "clock",
    "temperature",
    "humidity",
    "battery",
    "charging",
    "sd",
    "wifi",
    "ble",
];

fn value(document: &web_sys::Document, id: &str) -> Result<String, JsValue> {
    let control = element(document, id)?;
    if let Some(input) = control.dyn_ref::<web_sys::HtmlInputElement>() {
        Ok(input.value())
    } else {
        Ok(control.dyn_into::<web_sys::HtmlSelectElement>()?.value())
    }
}

fn update_task_controls(
    document: &web_sys::Document,
    preview: &WebPreview,
) -> Result<device_core::daily::Task, JsValue> {
    let id = value(document, "task-number")?
        .parse()
        .ok()
        .and_then(device_core::daily::TaskId::new)
        .ok_or_else(|| JsValue::from_str("Choose a task from 1 to 6."))?;
    let task = preview.inner.app().daily.task(id);
    element(document, "task-toggle")?.set_text_content(Some(if task.done {
        "Reopen task"
    } else {
        "Mark done"
    }));
    for control in ["task-toggle", "task-clear"] {
        let control = element(document, control)?;
        if task.text.is_empty() {
            control.set_attribute("disabled", "")?;
        } else {
            control.remove_attribute("disabled")?;
        }
    }
    Ok(task)
}

fn sync_task(document: &web_sys::Document, preview: &WebPreview) -> Result<(), JsValue> {
    let task = update_task_controls(document, preview)?;
    element(document, "task-text")?
        .dyn_into::<web_sys::HtmlInputElement>()?
        .set_value(task.text.as_str());
    Ok(())
}

fn sync_daily(document: &web_sys::Document, preview: &WebPreview) -> Result<(), JsValue> {
    for field in ["task-text", "note-text"] {
        element(document, field)?.remove_attribute("aria-invalid")?;
        element(document, &format!("{field}-error"))?.set_text_content(None);
    }
    element(document, "daily-feedback")?.set_text_content(None);
    sync_task(document, preview)?;
    element(document, "note-text")?
        .dyn_into::<web_sys::HtmlInputElement>()?
        .set_value(preview.inner.app().daily.note().as_str());
    Ok(())
}

fn clear_validation(document: &web_sys::Document) -> Result<(), JsValue> {
    for id in MEASUREMENT_FIELDS {
        let control = element(document, id)?;
        control.remove_attribute("aria-invalid")?;
        element(document, &format!("{id}-error"))?.set_text_content(None);
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn refresh_local_clock(preview: &mut WebPreview) -> Result<(), JsValue> {
    use device_core::telemetry::{DateTime, Sample};
    let date = js_sys::Date::new_0();
    let now = DateTime {
        year: u16::try_from(date.get_full_year()).unwrap_or(0),
        month: (date.get_month() + 1) as u8,
        day: date.get_date() as u8,
        hour: date.get_hours() as u8,
        minute: date.get_minutes() as u8,
        second: date.get_seconds() as u8,
    };
    if !now.valid() {
        return preview.command("clock unknown");
    }
    if !matches!(preview.inner.app().telemetry.clock, Sample::Fresh { value, .. } if value == now) {
        preview.command(&format!(
            "clock {:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            now.year, now.month, now.day, now.hour, now.minute, now.second
        ))?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn apply_daily_drafts(
    document: &web_sys::Document,
    preview: &mut WebPreview,
) -> Result<device_core::settings::Settings, JsValue> {
    let mut candidate = preview.inner.clone();
    let task = value(document, "task-text")?;
    let note = value(document, "note-text")?;
    let number = value(document, "task-number")?;
    for (field, command) in [
        (
            "task-text",
            if task.trim().is_empty() {
                format!("task clear {number}")
            } else {
                format!("task set {number} {task}")
            },
        ),
        (
            "note-text",
            if note.trim().is_empty() {
                "note clear".into()
            } else {
                format!("note set {note}")
            },
        ),
    ] {
        element(document, field)?.remove_attribute("aria-invalid")?;
        element(document, &format!("{field}-error"))?.set_text_content(None);
        if let Err(message) = candidate.command(&command) {
            element(document, field)?.set_attribute("aria-invalid", "true")?;
            element(document, &format!("{field}-error"))?.set_text_content(Some(message));
            element(document, "daily-feedback")?
                .set_text_content(Some("Correct the field error before sending."));
            element(document, field)?
                .dyn_into::<web_sys::HtmlElement>()?
                .focus()?;
            return Err(JsValue::from_str(message));
        }
    }
    candidate
        .command(&format!("screen {}", value(document, "daily-screen")?))
        .map_err(JsValue::from_str)?;
    preview.inner = candidate;
    sync_daily(document, preview)?;
    update(document, preview)?;
    Ok(preview.inner.app().settings())
}

#[wasm_bindgen]
pub fn mount() -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| JsValue::from_str("document unavailable"))?;
    let preview = Rc::new(RefCell::new(WebPreview::new()));
    preview
        .borrow_mut()
        .inner
        .apply_measurements(&device_preview::MeasurementInputs::default())
        .map_err(|e| JsValue::from_str(e.message()))?;
    #[cfg(target_arch = "wasm32")]
    if !workspace::tools_enabled()? {
        refresh_local_clock(&mut preview.borrow_mut())?;
    }
    update(&document, &preview.borrow())?;
    sync_daily(&document, &preview.borrow())?;
    workspace::mount(document.clone(), Rc::clone(&preview))?;
    #[cfg(target_arch = "wasm32")]
    media::mount(document.clone(), Rc::clone(&preview))?;
    element(&document, "form-feedback")?.set_text_content(Some("Example measurements applied."));
    for (id, event_name, action) in [
        ("previous", "click", "previous"),
        ("refresh", "click", "refresh"),
        ("next", "click", "next"),
        ("screen", "change", "screen"),
        ("daily-screen", "change", "daily-screen"),
        ("hold-previous", "click", "hold previous"),
        ("hold-refresh", "click", "hold refresh"),
        ("hold-next", "click", "hold next"),
        ("task-editor", "submit", "task-save"),
        ("task-number", "change", "task-number"),
        ("task-toggle", "click", "task-toggle"),
        ("task-clear", "click", "task-clear"),
        ("note-editor", "submit", "note-save"),
        ("note-clear", "click", "note-clear"),
        ("wait", "change", "wait"),
        ("complete", "click", "complete"),
        ("immediate", "change", "immediate"),
        ("timeout", "click", "timeout"),
        ("sensor-fault", "click", "sensor-fault"),
        ("sensor-missing", "click", "sensor-missing"),
        ("rtc-invalid", "click", "rtc-invalid"),
        ("advance", "click", "advance"),
        ("measurements", "submit", "apply"),
        ("reset-example", "click", "reset"),
        ("record", "click", "record"),
        ("replay", "click", "replay"),
        ("rotation", "change", "rotation"),
    ]
    .into_iter()
    .chain(MEASUREMENT_FIELDS.map(|id| (id, "input", "validate")))
    .chain(MEASUREMENT_FIELDS.map(|id| (id, "invalid", "invalid")))
    {
        let document = document.clone();
        let preview = Rc::clone(&preview);
        let button = element(&document, id)?;
        let error = element(&document, "error")?;
        let form_error = element(&document, "form-error")?;
        let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
            event.prevent_default();
            let mut invalid_field = None;
            let result = (|| -> Result<(), JsValue> {
                let mut preview = preview.borrow_mut();
                match action {
                    "screen" => {
                        preview.command(&format!("screen {}", value(&document, "screen")?))?
                    }
                    "daily-screen" => {
                        preview.command(&format!("screen {}", value(&document, "daily-screen")?))?
                    }
                    "task-number" => sync_task(&document, &preview)?,
                    "task-save" | "task-toggle" | "task-clear" | "note-save" | "note-clear" => {
                        let field = if action.starts_with("task-") {
                            "task-text"
                        } else {
                            "note-text"
                        };
                        let number = value(&document, "task-number")?;
                        let command = match action {
                            "task-save" => {
                                format!("task set {number} {}", value(&document, "task-text")?)
                            }
                            "task-toggle" => format!("task toggle {number}"),
                            "task-clear" => format!("task clear {number}"),
                            "note-save" => {
                                let note = value(&document, "note-text")?;
                                if note.trim().is_empty() {
                                    "note clear".into()
                                } else {
                                    format!("note set {note}")
                                }
                            }
                            _ => "note clear".into(),
                        };
                        element(&document, field)?.remove_attribute("aria-invalid")?;
                        element(&document, &format!("{field}-error"))?.set_text_content(None);
                        if let Err(message) = preview.inner.command(&command) {
                            invalid_field = Some(device_preview::InputError::new(field, message));
                            return Err(JsValue::from_str(message));
                        }
                        preview.command(if field == "task-text" {
                            "screen checklist"
                        } else {
                            "screen notes"
                        })?;
                        if field == "task-text" {
                            sync_task(&document, &preview)?;
                        }
                        element(&document, "daily-feedback")?.set_text_content(Some(
                            if field == "task-text" {
                                "Checklist updated."
                            } else {
                                "Note updated."
                            },
                        ));
                        if field == "note-text" {
                            element(&document, "note-text")?
                                .dyn_into::<web_sys::HtmlInputElement>()?
                                .set_value(preview.inner.app().daily.note().as_str());
                        }
                    }
                    "apply" | "validate" | "invalid" => {
                        clear_validation(&document)?;
                        let fields = MEASUREMENT_FIELDS.map(|field| value(&document, field));
                        let [clock, temperature, humidity, battery, charging, sd, wifi, ble] =
                            fields;
                        let inputs = device_preview::MeasurementInputs {
                            clock: &clock?,
                            temperature: &temperature?,
                            humidity: &humidity?,
                            battery: &battery?,
                            charging: &charging?,
                            sd: &sd?,
                            wifi: &wifi?,
                            ble: &ble?,
                        };
                        let result = if action == "apply" {
                            preview.inner.apply_measurements(&inputs)
                        } else {
                            inputs.check()
                        };
                        result.map_err(|error| {
                            invalid_field = Some(error);
                            JsValue::from_str(error.message())
                        })?;
                        element(&document, "form-feedback")?.set_text_content(Some(
                            if action == "apply" {
                                "Measurements applied."
                            } else {
                                "Changes not applied. Choose Apply measurements."
                            },
                        ));
                    }
                    "reset" => {
                        clear_validation(&document)?;
                        element(&document, "form-error")?.set_text_content(None);
                        element(&document, "form-feedback")?
                            .set_text_content(Some("Example measurements applied."));
                        element(&document, "measurements")?
                            .dyn_into::<web_sys::HtmlFormElement>()?
                            .reset();
                        element(&document, "rotation")?
                            .dyn_into::<web_sys::HtmlSelectElement>()?
                            .set_value("0");
                        let mut example = device_preview::Preview::default();
                        example
                            .apply_measurements(&device_preview::MeasurementInputs::default())
                            .map_err(|e| JsValue::from_str(e.message()))?;
                        preview.inner = example;
                        sync_daily(&document, &preview)?;
                    }
                    "rotation" => {
                        preview.command(&format!("rotation {}", value(&document, "rotation")?))?
                    }
                    "sensor-fault" => preview.command("temperature fault")?,
                    "sensor-missing" => preview.command("temperature missing")?,
                    "rtc-invalid" => preview.command("clock fault")?,
                    "advance" => {
                        let now = preview
                            .inner
                            .app()
                            .now_ms
                            .saturating_add(REFRESH_INTERVAL_MS);
                        preview.command(&format!("tick {now}"))?;
                    }
                    "record" => element(&document, "events")?
                        .dyn_into::<web_sys::HtmlTextAreaElement>()?
                        .set_value(&preview.recording()),
                    "replay" => {
                        let log = element(&document, "events")?
                            .dyn_into::<web_sys::HtmlTextAreaElement>()?
                            .value();
                        preview.replay(&log)?;
                        sync_daily(&document, &preview)?;
                    }
                    command => preview.command(command)?,
                }
                if !matches!(action, "validate" | "invalid") {
                    update(&document, &preview)?;
                }
                Ok(())
            })();
            if let Some(failure) = invalid_field {
                let decorated = (|| -> Result<(), JsValue> {
                    let field = failure.field();
                    let control = element(&document, field)?;
                    control.set_attribute("aria-invalid", "true")?;
                    element(&document, &format!("{field}-error"))?
                        .set_text_content(Some(failure.message()));
                    let daily = matches!(field, "task-text" | "note-text");
                    element(
                        &document,
                        if daily {
                            "daily-feedback"
                        } else {
                            "form-feedback"
                        },
                    )?
                    .set_text_content(Some("Changes not applied. Correct the field error."));
                    if daily || matches!(action, "apply" | "invalid") {
                        if let Some(details) = control.closest("details")? {
                            details.set_attribute("open", "")?;
                        }
                        control.dyn_into::<web_sys::HtmlElement>()?.focus()?;
                    }
                    Ok(())
                })();
                if let Err(failure) = decorated {
                    error.set_text_content(Some(&format!(
                        "Could not focus the invalid field: {failure:?}"
                    )));
                    return;
                }
            }
            let message = result.err().and_then(|e| e.as_string()).unwrap_or_default();
            if matches!(action, "apply" | "validate" | "invalid") {
                form_error.set_text_content(Some(if action == "validate" { "" } else { &message }));
            } else {
                error.set_text_content(Some(&message));
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        button.add_event_listener_with_callback(event_name, handler.as_ref().unchecked_ref())?;
        HANDLERS.with(|handlers| handlers.borrow_mut().push(handler));
    }
    Ok(())
}
