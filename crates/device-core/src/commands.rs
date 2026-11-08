use crate::{
    app::{AppEvent, Button, ScreenId},
    daily::{DailyEdit, TaskId, Text},
    geometry::Rotation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceCommand {
    Screen(ScreenId),
    Hold(Button),
    Edit(DailyEdit),
    Rotation(Rotation),
    Gallery(crate::media::GalleryEvent),
    Awake(u32),
}

impl DeviceCommand {
    pub fn into_event(self) -> Result<AppEvent, Rotation> {
        Ok(match self {
            Self::Screen(screen) => AppEvent::SelectScreen(screen),
            Self::Hold(button) => AppEvent::Hold(button),
            Self::Edit(edit) => AppEvent::EditDaily(edit),
            Self::Rotation(rotation) => return Err(rotation),
            Self::Gallery(event) => AppEvent::Gallery(event),
            Self::Awake(seconds) => AppEvent::KeepAwake(seconds),
        })
    }
}

pub fn parse(line: &str) -> Option<Result<DeviceCommand, &'static str>> {
    let (name, value) = line.split_once(' ').unwrap_or((line, ""));
    let event = match name {
        "screen" => ScreenId::parse(value)
            .map(DeviceCommand::Screen)
            .ok_or("Choose status, calendar, checklist, notes, diagnostics, drawing, or gallery."),
        "awake" => value
            .parse::<u32>()
            .ok()
            .filter(|n| *n <= 3600)
            .map(DeviceCommand::Awake)
            .ok_or("Choose 0 to 3600 seconds."),
        "gallery" => parse_gallery(value).map(DeviceCommand::Gallery),
        "hold" => match value {
            "previous" => Ok(DeviceCommand::Hold(Button::Previous)),
            "next" => Ok(DeviceCommand::Hold(Button::Next)),
            "refresh" => Ok(DeviceCommand::Hold(Button::Refresh)),
            _ => Err("Choose hold previous, hold refresh, or hold next."),
        },
        "note" => {
            if value == "clear" {
                Ok(DeviceCommand::Edit(DailyEdit::Note(Text::default())))
            } else if let Some(value) = value.strip_prefix("set ").filter(|value| !value.is_empty())
            {
                Text::new(value)
                    .map(|text| DeviceCommand::Edit(DailyEdit::Note(text)))
                    .ok_or("Use at most 160 printable ASCII characters for the note.")
            } else {
                Err("Enter note set TEXT or note clear.")
            }
        }
        "task" => parse_task(value).map(DeviceCommand::Edit),
        "rotation" => {
            return Some(match value {
                "0" => Ok(DeviceCommand::Rotation(Rotation::Portrait)),
                "1" => Ok(DeviceCommand::Rotation(Rotation::Landscape)),
                "2" => Ok(DeviceCommand::Rotation(Rotation::InvertedPortrait)),
                "3" => Ok(DeviceCommand::Rotation(Rotation::InvertedLandscape)),
                _ => Err("Choose rotation 0, 1, 2, or 3."),
            })
        }
        _ => return None,
    };
    Some(event)
}

fn parse_gallery(value: &str) -> Result<crate::media::GalleryEvent, &'static str> {
    use crate::{
        image::{Anchor, Fit},
        media::{GalleryEvent, ImageId, SlideInterval},
    };
    let (name, value) = value.split_once(' ').unwrap_or((value, ""));
    match (name, value) {
        ("next", "") => Ok(GalleryEvent::Step(true)),
        ("previous", "") => Ok(GalleryEvent::Step(false)),
        ("select", value) => value
            .parse()
            .ok()
            .and_then(ImageId::new)
            .map(GalleryEvent::Select)
            .ok_or("Choose an image from 1 to 8."),
        ("fit", value) => value
            .parse()
            .ok()
            .and_then(Fit::from_code)
            .map(GalleryEvent::Fit)
            .ok_or("Choose fit 0 to 3."),
        ("anchor", value) => value
            .parse()
            .ok()
            .and_then(Anchor::from_code)
            .map(GalleryEvent::Anchor)
            .ok_or("Choose anchor 0 to 8."),
        ("slideshow", "off") => Ok(GalleryEvent::Slideshow(None)),
        ("slideshow", value) => value
            .parse()
            .ok()
            .and_then(SlideInterval::new)
            .map(|interval| GalleryEvent::Slideshow(Some(interval)))
            .ok_or("Choose 60 to 86400 seconds, or off."),
        _ => Err("Enter gallery select, previous, next, fit, anchor, or slideshow."),
    }
}

fn parse_task(value: &str) -> Result<DailyEdit, &'static str> {
    let (action, rest) = value
        .split_once(' ')
        .ok_or("Enter task set, done, open, toggle, or clear followed by a number from 1 to 6.")?;
    let (number, label) = rest.split_once(' ').unwrap_or((rest, ""));
    let id = number
        .parse()
        .ok()
        .and_then(TaskId::new)
        .ok_or("Choose a task number from 1 to 6.")?;
    match (action, label) {
        ("set", value) => {
            let text = Text::new(value)
                .filter(|text| !text.is_empty())
                .ok_or("Use 1 to 48 printable ASCII characters for the task.")?;
            Ok(DailyEdit::SetTask { id, text })
        }
        ("done", "") => Ok(DailyEdit::SetDone { id, done: true }),
        ("open", "") => Ok(DailyEdit::SetDone { id, done: false }),
        ("toggle", "") => Ok(DailyEdit::Toggle(id)),
        ("clear", "") => Ok(DailyEdit::Clear(id)),
        _ => Err("Enter task set, done, open, toggle, or clear followed by a number from 1 to 6."),
    }
}
