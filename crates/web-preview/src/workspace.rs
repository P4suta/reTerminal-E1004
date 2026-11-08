use super::{daily, draw_frame, element, value, EventHandler, WebPreview, HANDLERS};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};

const VIEWS: [&str; 4] = ["images", "daily", "device", "tools"];

pub fn tools_enabled() -> Result<bool, JsValue> {
    Ok(web_sys::window()
        .ok_or_else(|| JsValue::from_str("Window unavailable."))?
        .location()
        .search()?
        .trim_start_matches('?')
        .split('&')
        .any(|field| matches!(field, "tools" | "tools=1")))
}

pub fn visible(document: &web_sys::Document, id: &str, show: bool) -> Result<(), JsValue> {
    let control = element(document, id)?;
    if show {
        control.remove_attribute("hidden")
    } else {
        control.set_attribute("hidden", "")
    }
}

fn selected(document: &web_sys::Document, view: &str) -> Result<bool, JsValue> {
    Ok(
        element(document, &format!("tab-{view}"))?.get_attribute("aria-selected")
            == Some("true".into()),
    )
}

pub fn refresh(document: &web_sys::Document, preview: &WebPreview) -> Result<(), JsValue> {
    let images = selected(document, "images")?;
    let device = selected(document, "device")?;
    let daily = selected(document, "daily")?;
    let tools = selected(document, "tools")?;
    visible(document, "preview-pane", !device)?;
    visible(document, "preview-controls", tools)?;
    visible(document, "image-source", images)?;
    visible(document, "media-status", images)?;
    visible(document, "image-name", images)?;
    element(document, "workspace")?.set_class_name(if device {
        "workspace connection-only"
    } else if tools {
        "workspace tools"
    } else if daily {
        "workspace daily"
    } else {
        "workspace"
    });
    let image = value(document, "image-slot")?
        .parse()
        .ok()
        .and_then(|number| preview.export_image(number));
    visible(document, "empty-image", images && image.is_none())?;
    visible(document, "image-actions", images && image.is_some())?;
    visible(document, "image-send-help", images && image.is_some())?;
    visible(document, "display", !images || image.is_some())?;
    if images {
        if let Some(file) = image {
            if let Some(frame) = device_core::media::PackedFrame::from_file(&file) {
                draw_frame(element(document, "display")?.dyn_into()?, frame.bytes())?;
                if let Some(record) = device_core::media::ImageRecord::decode(
                    &file[..device_core::media::HEADER_BYTES],
                ) {
                    element(document, "display")?.set_attribute(
                        "data-image-generation",
                        &record.descriptor().generation.to_string(),
                    )?;
                    element(document, "display")?
                        .set_attribute("data-image-id", &record.id().number().to_string())?;
                }
            }
        }
    }
    element(document, "preview-caption")?.set_text_content(Some(if images {
        "Updates automatically. Save a copy or send to your display."
    } else if tools {
        "Simulated display. Completion and fault controls affect the shown frame."
    } else {
        "Your content preview. Choose Send to display when ready."
    }));
    Ok(())
}

fn activate(
    document: &web_sys::Document,
    preview: &Rc<RefCell<WebPreview>>,
    view: &str,
    focus: bool,
) -> Result<(), JsValue> {
    for name in VIEWS {
        let active = name == view;
        let tab = element(document, &format!("tab-{name}"))?;
        tab.set_attribute("aria-selected", if active { "true" } else { "false" })?;
        tab.set_attribute("tabindex", if active { "0" } else { "-1" })?;
        visible(document, &format!("panel-{name}"), active)?;
    }
    if view == "daily" {
        #[cfg(target_arch = "wasm32")]
        if !tools_enabled()? {
            super::refresh_local_clock(&mut preview.borrow_mut().inner)?;
        }
        let screen = daily::screen(document)?;
        if device_core::app::ScreenId::parse(screen) != Some(preview.borrow().inner.app().screen) {
            preview.borrow_mut().command(&format!("screen {screen}"))?;
        }
    }
    let workspace = element(document, "workspace")?;
    let first = element(
        document,
        if view == "daily" {
            "task-pane"
        } else {
            "preview-pane"
        },
    )?;
    if workspace.first_element_child().as_ref() != Some(&first) {
        workspace.insert_before(&first, workspace.first_child().as_ref())?;
    }
    super::update(document, &preview.borrow())?;
    if focus {
        element(document, &format!("tab-{view}"))?
            .dyn_into::<web_sys::HtmlElement>()?
            .focus()?;
    }
    Ok(())
}

pub fn mount(document: web_sys::Document, preview: Rc<RefCell<WebPreview>>) -> Result<(), JsValue> {
    let tools = tools_enabled()?;
    visible(&document, "tab-tools", tools)?;
    activate(
        &document,
        &preview,
        if tools { "tools" } else { "images" },
        false,
    )?;
    let views = &VIEWS[..if tools { 4 } else { 3 }];
    for (index, view) in views.iter().copied().enumerate() {
        for event_name in ["click", "keydown", "focus"] {
            let control = element(&document, &format!("tab-{view}"))?;
            let document = document.clone();
            let preview = Rc::clone(&preview);
            let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
                let target = if event_name == "keydown" {
                    let Some(event) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                        return;
                    };
                    match event.key().as_str() {
                        "ArrowLeft" => (index + views.len() - 1) % views.len(),
                        "ArrowRight" => (index + 1) % views.len(),
                        "Home" => 0,
                        "End" => views.len() - 1,
                        _ => return,
                    }
                } else {
                    if selected(&document, view).unwrap_or(false) {
                        return;
                    }
                    index
                };
                event.prevent_default();
                if let Err(error) =
                    activate(&document, &preview, views[target], event_name == "keydown")
                {
                    if let Ok(output) = element(&document, "error") {
                        output.set_text_content(Some(
                            &error
                                .as_string()
                                .unwrap_or_else(|| "Could not change workspace.".into()),
                        ));
                    }
                }
            }) as Box<dyn FnMut(web_sys::Event)>);
            control
                .add_event_listener_with_callback(event_name, handler.as_ref().unchecked_ref())?;
            HANDLERS.with(|handlers| handlers.borrow_mut().push(handler as EventHandler));
        }
    }
    Ok(())
}
