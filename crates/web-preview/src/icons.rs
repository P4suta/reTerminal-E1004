use wasm_bindgen::{JsCast, JsValue};

const SVG: &str = "http://www.w3.org/2000/svg";

fn path(name: &str) -> Option<&'static str> {
    Some(match name {
        "image" => "M3 3h18v18H3z M3 16l5-5 5 5 3-3 5 5 M8 7h.01",
        "list" => "M9 6h12 M9 12h12 M9 18h12 M3 5l1 1 2-2 M3 11l1 1 2-2 M3 17l1 1 2-2",
        "calendar" => "M3 5h18v16H3z M7 3v4 M17 3v4 M3 10h18 M7 14h2 M13 14h2 M7 18h2",
        "note" => "M5 3h14v18H5z M8 8h8 M8 12h8 M8 16h5",
        "device" => "M4 3h16v18H4z M7 6h10v11H7z M11 19h2",
        "settings" => "M4 5h16 M4 12h16 M4 19h16 M8 3v4 M16 10v4 M10 17v4",
        "send" => "M21 3L3 10l7 4 4 7z M10 14L21 3",
        "download" => "M12 3v12 M7 10l5 5 5-5 M4 16v5h16v-5",
        "upload" => "M12 16V3 M7 8l5-5 5 5 M4 16v5h16v-5",
        "close" => "M5 5l14 14 M19 5L5 19",
        "left" => "M14 5l-7 7 7 7",
        "right" => "M10 5l7 7-7 7",
        "refresh" => "M20 7v5h-5 M4 17v-5h5 M6 7a7 7 0 0 1 12-1l2 3 M18 17a7 7 0 0 1-12 1l-2-3",
        "plus" => "M12 4v16 M4 12h16",
        "check" => "M4 12l5 5L20 6",
        "edit" => "M14 5l5 5 M4 20l5-1L21 7l-5-5L4 14z",
        "delete" => "M3 6h18 M9 6V3h6v3 M6 6l1 15h10l1-15 M10 10v7 M14 10v7",
        "undo" => "M9 4L3 10l6 6 M3 10h11a6 6 0 0 1 0 12",
        "save" => "M4 3h14l3 3v15H3V3z M7 3v6h10V3 M7 21v-8h10v8",
        "rotate" => "M4 8V3l5 5H4 M6 6a8 8 0 1 1-2 10 M9 9h7v7H9z",
        "fit" => "M3 9V3h6 M15 3h6v6 M21 15v6h-6 M9 21H3v-6 M7 7h10v10H7z",
        "palette" => "M12 3a9 9 0 1 0 0 18h2a2 2 0 0 0 0-4h-1a2 2 0 0 1 0-4h3a5 5 0 0 0 0-10z M7 9h.01 M10 6h.01 M15 7h.01 M6 14h.01",
        "dither" => "M4 4h2 M11 4h2 M18 4h2 M4 11h2 M11 11h2 M18 11h2 M4 18h2 M11 18h2 M18 18h2",
        "position" => "M12 3v18 M3 12h18 M8 7l4-4 4 4 M8 17l4 4 4-4 M7 8l-4 4 4 4 M17 8l4 4-4 4",
        "sun" => "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l2 2 M17 17l2 2 M5 19l2-2 M17 7l2-2",
        "contrast" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18 M12 3v18 M12 7h6 M12 11h8 M12 15h7 M12 19h4",
        "drop" => "M12 3s-7 8-7 12a7 7 0 0 0 14 0c0-4-7-12-7-12z M9 15a3 3 0 0 0 3 3",
        "text" => "M4 4h16 M12 4v16 M8 20h8",
        "qr" => "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h3v3h4v4h-7v-3 M20 14h1 M6 6h1 M17 6h1 M6 17h1",
        "play" => "M7 3l14 9-14 9z",
        "pause" => "M6 3h4v18H6z M14 3h4v18h-4z",
        "connect" => "M8 3v5 M16 3v5 M5 8h14v3a7 7 0 0 1-14 0z M12 18v4",
        "disconnect" => "M8 3v5 M16 3v5 M5 8h14v3a7 7 0 0 1-14 0 M12 18v4 M3 3l18 18",
        "info" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18 M12 11v6 M12 7h.01",
        "moon" => "M20 15a9 9 0 0 1-11-11 9 9 0 1 0 11 11z",
        "sd" => "M8 3h12v18H4V7z M9 3v6h8V3 M8 14h8 M8 18h8",
        "clock" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18 M12 7v5l4 2",
        "temperature" => "M9 14V5a3 3 0 0 1 6 0v9a5 5 0 1 1-6 0 M12 8v9 M11 18h2",
        "battery" => "M2 6h18v12H2z M22 10v4 M6 9v6 M10 9v6 M14 9v6",
        "wifi" => "M2 8a16 16 0 0 1 20 0 M5 12a11 11 0 0 1 14 0 M8 16a6 6 0 0 1 8 0 M12 20h.01",
        "bluetooth" => "M7 7l12 10-7 5V2l7 5L7 17",
        "terminal" => "M3 4h18v16H3z M6 8l4 4-4 4 M13 16h5",
        "record" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18 M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8",
        "alert" => "M12 3L2 21h20z M12 9v5 M12 18h.01",
        _ => return None,
    })
}

pub fn decorate(document: &web_sys::Document, control: &web_sys::Element) -> Result<(), JsValue> {
    let name = control
        .get_attribute("data-icon")
        .ok_or_else(|| JsValue::from_str("Missing action icon."))?;
    let drawing = path(&name).ok_or_else(|| JsValue::from_str("Unknown action icon."))?;
    if control.query_selector(":scope > .control-icon")?.is_some() {
        return Ok(());
    }
    let icon = document.create_element_ns(Some(SVG), "svg")?;
    for (name, value) in [
        ("class", "control-icon"),
        ("viewBox", "0 0 24 24"),
        ("aria-hidden", "true"),
        ("focusable", "false"),
    ] {
        icon.set_attribute(name, value)?;
    }
    let line = document.create_element_ns(Some(SVG), "path")?;
    line.set_attribute("d", drawing)?;
    icon.append_child(&line)?;
    control.insert_before(&icon, control.first_child().as_ref())?;
    Ok(())
}

pub fn mount(document: &web_sys::Document) -> Result<(), JsValue> {
    let controls = document.query_selector_all("[data-icon]")?;
    for index in 0..controls.length() {
        if let Some(control) = controls.item(index) {
            decorate(document, &control.dyn_into()?)?;
        }
    }
    Ok(())
}

pub fn caption(
    document: &web_sys::Document,
    id: &str,
    icon: &str,
    text: &str,
) -> Result<(), JsValue> {
    let control = super::element(document, id)?;
    control.set_text_content(Some(text));
    control.set_attribute("data-icon", icon)?;
    decorate(document, &control)
}
