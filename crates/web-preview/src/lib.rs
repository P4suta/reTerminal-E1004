use device_core::{
    app::{RefreshState, REFRESH_INTERVAL_MS},
    geometry::{HEIGHT, WIDTH},
};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;

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
    pub fn draw(&self, canvas: web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
        let context = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("canvas context unavailable"))?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
        let pixels = device_preview::rgba(&self.inner.frame());
        let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&pixels),
            u32::from(WIDTH),
            u32::from(HEIGHT),
        )?;
        context.put_image_data(&image, 0.0, 0.0)
    }
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
    element(document, "update-status")?.set_text_content(Some(preview.inner.display_status()));
    element(document, "wait")?.set_attribute(
        "aria-pressed",
        if preview.inner.immediate() {
            "false"
        } else {
            "true"
        },
    )?;
    element(document, "immediate")?.set_attribute(
        "aria-pressed",
        if preview.inner.immediate() {
            "true"
        } else {
            "false"
        },
    )?;
    for id in ["complete", "timeout"] {
        let control = element(document, id)?;
        if matches!(preview.inner.app().refresh, RefreshState::Updating { .. }) {
            control.remove_attribute("disabled")?;
        } else {
            control.set_attribute("disabled", "")?;
        }
    }
    Ok(())
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

fn clear_validation(document: &web_sys::Document) -> Result<(), JsValue> {
    for id in MEASUREMENT_FIELDS {
        let control = element(document, id)?;
        control.remove_attribute("aria-invalid")?;
        let description = control
            .get_attribute("aria-describedby")
            .unwrap_or_default()
            .split_whitespace()
            .filter(|id| *id != "error")
            .collect::<Vec<_>>()
            .join(" ");
        control.set_attribute("aria-describedby", &description)?;
    }
    Ok(())
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
    update(&document, &preview.borrow())?;
    for (id, event_name, action) in [
        ("previous", "click", "previous"),
        ("refresh", "click", "refresh"),
        ("next", "click", "next"),
        ("wait", "click", "wait"),
        ("complete", "click", "complete"),
        ("immediate", "click", "immediate"),
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
    ] {
        let document = document.clone();
        let preview = Rc::clone(&preview);
        let button = element(&document, id)?;
        let error = element(&document, "error")?;
        let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
            event.prevent_default();
            let mut invalid_field = None;
            let result = (|| -> Result<(), JsValue> {
                let mut preview = preview.borrow_mut();
                clear_validation(&document)?;
                match action {
                    "apply" => {
                        let fields = MEASUREMENT_FIELDS.map(|field| value(&document, field));
                        let [clock, temperature, humidity, battery, charging, sd, wifi, ble] =
                            fields;
                        preview
                            .inner
                            .apply_measurements(&device_preview::MeasurementInputs {
                                clock: &clock?,
                                temperature: &temperature?,
                                humidity: &humidity?,
                                battery: &battery?,
                                charging: &charging?,
                                sd: &sd?,
                                wifi: &wifi?,
                                ble: &ble?,
                            })
                            .map_err(|e| {
                                invalid_field = Some(e.field());
                                JsValue::from_str(e.message())
                            })?;
                    }
                    "reset" => {
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
                    }
                    command => preview.command(command)?,
                }
                update(&document, &preview)?;
                Ok(())
            })();
            if let Some(field) = invalid_field {
                let decorated = (|| -> Result<(), JsValue> {
                    let control = element(&document, field)?;
                    control.set_attribute("aria-invalid", "true")?;
                    control.set_attribute(
                        "aria-describedby",
                        &format!(
                            "{} error",
                            control
                                .get_attribute("aria-describedby")
                                .unwrap_or_default()
                        ),
                    )?;
                    control.dyn_into::<web_sys::HtmlElement>()?.focus()
                })();
                if let Err(failure) = decorated {
                    error.set_text_content(Some(&format!(
                        "Could not focus the invalid field: {failure:?}"
                    )));
                    return;
                }
            }
            error.set_text_content(Some(
                &result.err().and_then(|e| e.as_string()).unwrap_or_default(),
            ));
        }) as Box<dyn FnMut(web_sys::Event)>);
        button.add_event_listener_with_callback(event_name, handler.as_ref().unchecked_ref())?;
        HANDLERS.with(|handlers| handlers.borrow_mut().push(handler));
    }
    Ok(())
}
