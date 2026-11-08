use super::{element, icons, update, value, workspace, EventHandler, WebPreview, HANDLERS};
use device_core::daily::{Task, TaskId, TASK_COUNT};
use std::{cell::RefCell, fmt::Write, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};

const SCREENS: [&str; 3] = ["checklist", "notes", "calendar"];

fn input(document: &web_sys::Document, id: &str) -> Result<web_sys::HtmlInputElement, JsValue> {
    Ok(element(document, id)?.dyn_into()?)
}

fn focus(document: &web_sys::Document, id: &str) -> Result<(), JsValue> {
    element(document, id)?
        .dyn_into::<web_sys::HtmlElement>()?
        .focus()
}

pub fn screen(document: &web_sys::Document) -> Result<&'static str, JsValue> {
    for name in SCREENS {
        if element(document, &format!("daily-{name}"))?
            .get_attribute("aria-pressed")
            .as_deref()
            == Some("true")
        {
            return Ok(name);
        }
    }
    Err(JsValue::from_str("Choose daily content."))
}

pub fn select(document: &web_sys::Document, screen: &str) -> Result<(), JsValue> {
    for name in SCREENS {
        let selected = screen == name;
        element(document, &format!("daily-{name}"))?
            .set_attribute("aria-pressed", if selected { "true" } else { "false" })?;
        workspace::visible(document, &format!("{name}-content"), selected)?;
    }
    Ok(())
}

fn clear_error(document: &web_sys::Document, field: &str) -> Result<(), JsValue> {
    element(document, field)?.remove_attribute("aria-invalid")?;
    element(document, field)?.remove_attribute("data-capacity-error")?;
    element(document, &format!("{field}-error"))?.set_text_content(None);
    Ok(())
}

fn invalid(document: &web_sys::Document, field: &str, message: &str) -> JsValue {
    let result = (|| -> Result<(), JsValue> {
        select(
            document,
            if field == "note-text" {
                "notes"
            } else {
                "checklist"
            },
        )?;
        if field == "task-text" {
            workspace::visible(document, "task-editor", true)?;
        }
        element(document, field)?.set_attribute("aria-invalid", "true")?;
        element(document, &format!("{field}-error"))?.set_text_content(Some(message));
        focus(document, field)
    })();
    result.err().unwrap_or_else(|| JsValue::from_str(message))
}

fn command(
    document: &web_sys::Document,
    preview: &mut device_preview::Preview,
    field: &str,
    command: &str,
) -> Result<(), JsValue> {
    clear_error(document, field)?;
    preview
        .command(command)
        .map_err(|message| invalid(document, field, message))
}

fn add(document: &web_sys::Document, preview: &mut device_preview::Preview) -> Result<u8, JsValue> {
    clear_error(document, "task-text")?;
    let text = value(document, "task-text")?;
    if text.trim().is_empty() {
        return Err(invalid(document, "task-text", "Enter a task to add."));
    }
    let Some(index) = preview
        .app()
        .daily
        .tasks()
        .iter()
        .position(|task| task.text.is_empty())
    else {
        element(document, "task-text")?.set_attribute("data-capacity-error", "true")?;
        return Err(invalid(
            document,
            "task-text",
            "Your list is full. Delete a task to add another.",
        ));
    };
    let number = index as u8 + 1;
    command(
        document,
        preview,
        "task-text",
        &format!("task set {number} {text}"),
    )?;
    Ok(number)
}

#[cfg(target_arch = "wasm32")]
pub fn drafts_changed(
    document: &web_sys::Document,
    preview: &device_preview::Preview,
) -> Result<bool, JsValue> {
    if !value(document, "task-text")?.trim().is_empty()
        || value(document, "note-text")? != preview.app().daily.note().as_str()
    {
        return Ok(true);
    }
    for (index, task) in preview.app().daily.tasks().iter().enumerate() {
        let number = index + 1;
        if !element(document, &format!("task-form-{number}"))?.has_attribute("hidden")
            && value(document, &format!("task-edit-text-{number}"))? != task.text.as_str()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_arch = "wasm32")]
pub fn apply_drafts(
    document: &web_sys::Document,
    preview: &mut device_preview::Preview,
) -> Result<(), JsValue> {
    for number in 1..=TASK_COUNT {
        if !element(document, &format!("task-form-{number}"))?.has_attribute("hidden") {
            let field = format!("task-edit-text-{number}");
            command(
                document,
                preview,
                &field,
                &format!("task set {number} {}", value(document, &field)?),
            )?;
        }
    }
    if !value(document, "task-text")?.trim().is_empty() {
        add(document, preview)?;
    }
    let note = value(document, "note-text")?;
    command(
        document,
        preview,
        "note-text",
        &if note.trim().is_empty() {
            "note clear".into()
        } else {
            format!("note set {note}")
        },
    )
}

pub fn sync(document: &web_sys::Document, preview: &WebPreview) -> Result<(), JsValue> {
    input(document, "task-text")?.set_value("");
    for field in ["task-text", "note-text"] {
        clear_error(document, field)?;
    }
    for number in 1..=TASK_COUNT {
        workspace::visible(document, &format!("task-form-{number}"), false)?;
        clear_error(document, &format!("task-edit-text-{number}"))?;
    }
    input(document, "note-text")?.set_value(preview.inner.app().daily.note().as_str());
    refresh(document, preview)
}

pub fn refresh(document: &web_sys::Document, preview: &WebPreview) -> Result<(), JsValue> {
    let daily = preview.inner.app().daily;
    let (done, total) = daily.counts();
    if total < TASK_COUNT && element(document, "task-text")?.has_attribute("data-capacity-error") {
        clear_error(document, "task-text")?;
    }
    workspace::visible(
        document,
        "task-editor",
        total < TASK_COUNT
            || !value(document, "task-text")?.trim().is_empty()
            || element(document, "task-text")?.has_attribute("aria-invalid"),
    )?;
    element(document, "task-count")?
        .set_text_content(Some(&format!("{done} of {total} completed")));
    workspace::visible(document, "task-empty", total == 0)?;
    workspace::visible(document, "task-full", total == TASK_COUNT)?;
    for (name, disabled) in [
        ("task-save", total == TASK_COUNT),
        ("task-clear-completed", done == 0),
    ] {
        let control = element(document, name)?;
        if disabled {
            control.set_attribute("disabled", "")?;
        } else {
            control.remove_attribute("disabled")?;
        }
    }
    for (index, task) in daily.tasks().iter().enumerate() {
        let number = index + 1;
        workspace::visible(
            document,
            &format!("task-row-{number}"),
            !task.text.is_empty(),
        )?;
        let editing = !element(document, &format!("task-form-{number}"))?.has_attribute("hidden");
        workspace::visible(document, &format!("task-view-{number}"), !editing)?;
        element(document, &format!("task-label-{number}"))?
            .set_text_content(Some(task.text.as_str()));
        element(document, &format!("task-row-{number}"))?
            .set_attribute("data-completed", if task.done { "true" } else { "false" })?;
        input(document, &format!("task-done-{number}"))?.set_checked(task.done);
        for action in ["edit", "delete"] {
            let label = format!(
                "{} {}",
                if action == "edit" { "Edit" } else { "Delete" },
                task.text.as_str()
            );
            let control = element(document, &format!("task-{action}-{number}"))?;
            control.set_attribute("aria-label", &label)?;
            control.set_attribute("title", &label)?;
        }
    }
    let undo = !preview.removed_tasks.is_empty();
    workspace::visible(document, "task-undo", undo)?;
    let conflict = preview
        .removed_tasks
        .iter()
        .any(|(id, _)| !daily.task(*id).text.is_empty());
    workspace::visible(document, "task-undo-help", undo && conflict)?;
    if conflict {
        element(document, "task-undo")?.set_attribute("disabled", "")?;
    } else {
        element(document, "task-undo")?.remove_attribute("disabled")?;
    }
    let month = preview.inner.app().calendar();
    element(document, "calendar-month")?.set_text_content(Some(&month.map_or_else(
        || "Set the display time to browse the calendar.".into(),
        |month| format!("{} {}", month.name(), month.year()),
    )));
    let mut days = String::new();
    if let Some(month) = month {
        let today = match preview.inner.app().telemetry.clock {
            device_core::telemetry::Sample::Fresh { value, .. }
                if value.year == month.year() && value.month == month.number() =>
            {
                Some(value.day)
            }
            _ => None,
        };
        for week in 0..6 {
            days.push_str("<tr>");
            for weekday in 0..7 {
                if let Some(day) = month.day_at(week * 7 + weekday) {
                    write!(
                        days,
                        "<td><time datetime=\"{:04}-{:02}-{day:02}\"{}>{day}</time></td>",
                        month.year(),
                        month.number(),
                        if today == Some(day) {
                            " aria-current=\"date\""
                        } else {
                            ""
                        }
                    )
                    .map_err(|_| JsValue::from_str("Could not draw the calendar."))?;
                } else {
                    days.push_str("<td></td>");
                }
            }
            days.push_str("</tr>");
        }
    }
    element(document, "calendar-days")?.set_inner_html(&days);
    for name in ["calendar-previous", "calendar-today", "calendar-next"] {
        if month.is_none() {
            element(document, name)?.set_attribute("disabled", "")?;
        } else {
            element(document, name)?.remove_attribute("disabled")?;
        }
    }
    Ok(())
}

fn remove(
    document: &web_sys::Document,
    preview: &mut WebPreview,
    tasks: Vec<(TaskId, Task)>,
) -> Result<(), JsValue> {
    let mut candidate = preview.inner.clone();
    for (id, _) in &tasks {
        candidate
            .command(&format!("task clear {}", id.number()))
            .map_err(JsValue::from_str)?;
        workspace::visible(document, &format!("task-form-{}", id.number()), false)?;
    }
    preview.inner = candidate;
    preview.removed_tasks = tasks;
    Ok(())
}

fn dispatch(
    document: &web_sys::Document,
    preview: &mut WebPreview,
    action: &str,
    number: u8,
) -> Result<(), JsValue> {
    let mut focus_target = None;
    let mut feedback = None;
    match action {
        "checklist" | "notes" | "calendar" => {
            if screen(document)? == action
                && device_core::app::ScreenId::parse(action) == Some(preview.inner.app().screen)
            {
                return Ok(());
            }
            select(document, action)?;
            preview.command(&format!("screen {action}"))?;
        }
        "add" => {
            let number = add(document, &mut preview.inner)?;
            input(document, "task-text")?.set_value("");
            focus_target = Some(if preview.inner.app().daily.counts().1 == TASK_COUNT {
                format!("task-done-{number}")
            } else {
                "task-text".to_owned()
            });
            feedback = Some("Task added.");
        }
        "edit" => {
            let id = TaskId::new(number).ok_or_else(|| JsValue::from_str("Unknown task."))?;
            let field = format!("task-edit-text-{number}");
            input(document, &field)?.set_value(preview.inner.app().daily.task(id).text.as_str());
            clear_error(document, &field)?;
            workspace::visible(document, &format!("task-form-{number}"), true)?;
            focus_target = Some(field);
        }
        "cancel" => {
            workspace::visible(document, &format!("task-form-{number}"), false)?;
            clear_error(document, &format!("task-edit-text-{number}"))?;
            focus_target = Some(format!("task-edit-{number}"));
        }
        "save" => {
            let field = format!("task-edit-text-{number}");
            command(
                document,
                &mut preview.inner,
                &field,
                &format!("task set {number} {}", value(document, &field)?),
            )?;
            workspace::visible(document, &format!("task-form-{number}"), false)?;
            focus_target = Some(format!("task-edit-{number}"));
            feedback = Some("Task saved.");
        }
        "toggle" => {
            let done = input(document, &format!("task-done-{number}"))?.checked();
            preview.command(&format!(
                "task {} {number}",
                if done { "done" } else { "open" }
            ))?;
            feedback = Some(if done {
                "Task completed."
            } else {
                "Task reopened."
            });
        }
        "delete" => {
            let id = TaskId::new(number).ok_or_else(|| JsValue::from_str("Unknown task."))?;
            remove(
                document,
                preview,
                vec![(id, preview.inner.app().daily.task(id))],
            )?;
            focus_target = Some("task-text".to_owned());
            feedback = Some("Task deleted. Choose Undo to restore it.");
        }
        "clear-completed" => {
            let tasks = preview
                .inner
                .app()
                .daily
                .tasks()
                .iter()
                .enumerate()
                .filter(|(_, task)| task.done && !task.text.is_empty())
                .filter_map(|(index, task)| TaskId::new(index as u8 + 1).map(|id| (id, *task)))
                .collect();
            remove(document, preview, tasks)?;
            focus_target = Some("task-text".to_owned());
            feedback = Some("Completed tasks deleted. Choose Undo to restore them.");
        }
        "undo" => {
            let mut candidate = preview.inner.clone();
            for (id, task) in &preview.removed_tasks {
                if !candidate.app().daily.task(*id).text.is_empty() {
                    return Err(JsValue::from_str(
                        "Undo is unavailable because the task list changed.",
                    ));
                }
                candidate
                    .command(&format!("task set {} {}", id.number(), task.text.as_str()))
                    .map_err(JsValue::from_str)?;
                if task.done {
                    candidate
                        .command(&format!("task done {}", id.number()))
                        .map_err(JsValue::from_str)?;
                }
            }
            focus_target = preview
                .removed_tasks
                .first()
                .map(|(id, _)| format!("task-done-{}", id.number()));
            preview.inner = candidate;
            preview.removed_tasks.clear();
            feedback = Some("Tasks restored.");
        }
        "note-save" | "note-clear" | "note-discard" => {
            if action == "note-discard" {
                clear_error(document, "note-text")?;
            } else {
                let note = value(document, "note-text")?;
                command(
                    document,
                    &mut preview.inner,
                    "note-text",
                    &if action == "note-clear" || note.trim().is_empty() {
                        "note clear".into()
                    } else {
                        format!("note set {note}")
                    },
                )?;
                feedback = Some("Note updated.");
            }
            input(document, "note-text")?.set_value(preview.inner.app().daily.note().as_str());
            preview.command("screen notes")?;
            select(document, "notes")?;
        }
        "calendar-previous" | "calendar-today" | "calendar-next" => {
            preview.command("screen calendar")?;
            select(document, "calendar")?;
            #[cfg(target_arch = "wasm32")]
            if action == "calendar-today" && !workspace::tools_enabled()? {
                super::refresh_local_clock(&mut preview.inner)?;
            }
            preview.command(match action {
                "calendar-previous" => "hold previous",
                "calendar-next" => "hold next",
                _ => "calendar today",
            })?;
        }
        _ => return Err(JsValue::from_str("Unknown daily action.")),
    }
    if matches!(
        action,
        "add" | "save" | "toggle" | "delete" | "clear-completed" | "undo"
    ) {
        preview.command("screen checklist")?;
        select(document, "checklist")?;
    }
    update(document, preview)?;
    if let Some(message) = feedback {
        element(document, "daily-feedback")?.set_text_content(Some(message));
    }
    if let Some(id) = focus_target {
        focus(document, &id)?;
    }
    Ok(())
}

fn listen(
    document: &web_sys::Document,
    preview: &Rc<RefCell<WebPreview>>,
    id: &str,
    event_name: &'static str,
    action: &'static str,
    number: u8,
) -> Result<(), JsValue> {
    let control = element(document, id)?;
    let document = document.clone();
    let preview = Rc::clone(preview);
    let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
        if event_name == "keydown" {
            let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                return;
            };
            if key.key() != "Escape" || key.is_composing() {
                return;
            }
        }
        event.prevent_default();
        let result = dispatch(&document, &mut preview.borrow_mut(), action, number);
        if let Ok(output) = element(&document, "error") {
            output.set_text_content(result.err().and_then(|error| error.as_string()).as_deref());
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    control.add_event_listener_with_callback(event_name, handler.as_ref().unchecked_ref())?;
    HANDLERS.with(|handlers| handlers.borrow_mut().push(handler as EventHandler));
    Ok(())
}

pub fn mount(
    document: &web_sys::Document,
    preview: &Rc<RefCell<WebPreview>>,
) -> Result<(), JsValue> {
    let list = element(document, "task-list")?;
    for number in 1..=TASK_COUNT as u8 {
        let row = document.create_element("li")?;
        row.set_id(&format!("task-row-{number}"));
        row.set_class_name("task-row");
        row.set_attribute("hidden", "")?;
        row.set_inner_html(&format!(r#"
            <div id="task-view-{number}" class="task-view">
              <label class="task-check" for="task-done-{number}"><input type="checkbox" id="task-done-{number}"><span id="task-label-{number}"></span></label>
              <div class="task-actions"><button type="button" id="task-edit-{number}" data-icon="edit"><span class="sr-only">Edit</span></button><button type="button" id="task-delete-{number}" data-icon="delete"><span class="sr-only">Delete</span></button></div>
            </div>
            <form id="task-form-{number}" novalidate hidden>
              <label for="task-edit-text-{number}" data-icon="edit">Edit task</label><input type="text" id="task-edit-text-{number}" maxlength="48" required aria-describedby="task-help task-edit-text-{number}-error">
              <p id="task-edit-text-{number}-error" class="field-error" role="alert"></p>
              <div class="buttons"><button type="submit" id="task-save-{number}" data-icon="check">Save</button><button type="button" id="task-cancel-{number}" data-icon="close">Cancel</button></div>
            </form>"#));
        list.append_child(&row)?;
        for (id, event, action) in [
            (format!("task-done-{number}"), "change", "toggle"),
            (format!("task-edit-{number}"), "click", "edit"),
            (format!("task-delete-{number}"), "click", "delete"),
            (format!("task-form-{number}"), "submit", "save"),
            (format!("task-cancel-{number}"), "click", "cancel"),
            (format!("task-edit-text-{number}"), "keydown", "cancel"),
        ] {
            listen(document, preview, &id, event, action, number)?;
        }
    }
    for name in SCREENS {
        listen(
            document,
            preview,
            &format!("daily-{name}"),
            "click",
            name,
            0,
        )?;
    }
    for (id, event, action) in [
        ("task-editor", "submit", "add"),
        ("task-undo", "click", "undo"),
        ("task-clear-completed", "click", "clear-completed"),
        ("note-editor", "submit", "note-save"),
        ("note-clear", "click", "note-clear"),
        ("note-discard", "click", "note-discard"),
        ("calendar-previous", "click", "calendar-previous"),
        ("calendar-today", "click", "calendar-today"),
        ("calendar-next", "click", "calendar-next"),
    ] {
        listen(document, preview, id, event, action, 0)?;
    }
    icons::mount(document)
}
