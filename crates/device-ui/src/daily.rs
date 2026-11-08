use super::*;
use device_core::{calendar::Month, daily::TASK_COUNT};

pub struct CalendarScreen;
pub struct ChecklistScreen;
pub struct NotesScreen;

fn wrapped_line(value: &str, columns: usize) -> (&str, &str) {
    if value.len() <= columns {
        return (value, "");
    }
    let split = value
        .get(..columns)
        .and_then(|line| line.rfind(' '))
        .filter(|index| *index != 0)
        .unwrap_or(columns);
    (
        value.get(..split).unwrap_or_default(),
        value.get(split..).unwrap_or_default().trim_start(),
    )
}

fn paragraph(target: &mut Framebuffer<'_>, value: &str, x: i32, y: i32, scale: i32, spacing: i32) {
    let (width, height) = target.dimensions();
    let columns = ((i32::from(width) - MARGIN - x) / (10 * scale)).max(1) as usize;
    let mut remaining = value;
    let mut y = y;
    while !remaining.is_empty() && y + 20 * scale < i32::from(height) - FOOTER_RULE_BOTTOM {
        let (line, rest) = wrapped_line(remaining, columns);
        text(target, line, x, y, scale);
        remaining = rest;
        y += spacing;
    }
}

impl Screen for CalendarScreen {
    fn draw(&self, target: &mut Framebuffer<'_>, app: &App) {
        heading(target, "Calendar");
        let Some(month) = app.calendar() else {
            text(target, "Date unavailable", MARGIN, 300, 3);
            paragraph(
                target,
                "Set the device clock with rtc YYYY-MM-DDTHH:MM:SS over USB.",
                MARGIN,
                410,
                2,
                60,
            );
            return;
        };
        let title = TextBuffer::<48>::formatted(format_args!("{} {}", month.name(), month.year()));
        text(target, title.as_str(), MARGIN, 280, 3);
        let today = match app.telemetry.clock {
            Sample::Fresh { value, .. } if value.valid() => Some(value),
            Sample::Stale { .. }
            | Sample::Unavailable
            | Sample::Missing
            | Sample::Fault(_)
            | Sample::Fresh { .. } => None,
        };
        let caption = if let Some(date) = today {
            TextBuffer::<64>::formatted(format_args!(
                "Today: {} {:04}-{:02}-{:02}",
                Month::new(date.year, date.month)
                    .and_then(|month| month.weekday(date.day))
                    .unwrap_or(""),
                date.year,
                date.month,
                date.day
            ))
        } else {
            TextBuffer::formatted(format_args!("Current date unavailable; no day is marked."))
        };
        text(target, caption.as_str(), MARGIN, 370, 1);
        let (width, height) = target.dimensions();
        let cell_width = (i32::from(width) - 2 * MARGIN) / 7;
        let row_height = (i32::from(height) - 700) / 6;
        for (column, day) in ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"]
            .iter()
            .enumerate()
        {
            text(
                target,
                day,
                MARGIN + column as i32 * cell_width + 12,
                430,
                2,
            );
        }
        for cell in 0..42u8 {
            if let Some(day) = month.day_at(cell) {
                let x = MARGIN + i32::from(cell % 7) * cell_width;
                let y = 490 + i32::from(cell / 7) * row_height;
                if today.is_some_and(|date| {
                    date.year == month.year() && date.month == month.number() && date.day == day
                }) {
                    painted(
                        Rectangle::new(
                            Point::new(x, y),
                            Size::new((cell_width - 8) as u32, (row_height - 8) as u32),
                        )
                        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 3))
                        .draw(target),
                    );
                }
                let label = TextBuffer::<4>::formatted(format_args!("{day:2}"));
                text(target, label.as_str(), x + 12, y + 10, 3);
            }
        }
        text(
            target,
            "Hold side buttons: browse months. Middle: today.",
            MARGIN,
            i32::from(height) - 180,
            1,
        );
    }
}

impl Screen for ChecklistScreen {
    fn draw(&self, target: &mut Framebuffer<'_>, app: &App) {
        heading(target, "Checklist");
        let (done, total) = app.daily.counts();
        let count = TextBuffer::<48>::formatted(format_args!("{done} of {total} complete"));
        text(target, count.as_str(), MARGIN, 280, 2);
        let (width, height) = target.dimensions();
        let row_height = (i32::from(height) - 540) / TASK_COUNT as i32;
        for (index, task) in app.daily.tasks().iter().enumerate() {
            let y = 360 + index as i32 * row_height;
            if app.selected_task.index() == index {
                painted(
                    Rectangle::new(
                        Point::new(MARGIN - 12, y - 8),
                        Size::new(
                            (i32::from(width) - 2 * MARGIN + 24) as u32,
                            (row_height - 8) as u32,
                        ),
                    )
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 3))
                    .draw(target),
                );
            }
            let marker = TextBuffer::<16>::formatted(format_args!(
                "{} [{}]",
                index + 1,
                if task.done { "x" } else { " " }
            ));
            text(target, marker.as_str(), MARGIN, y, 2);
            paragraph(
                target,
                if task.text.is_empty() {
                    "Empty"
                } else {
                    task.text.as_str()
                },
                MARGIN + 140,
                y,
                2,
                44,
            );
        }
        text(
            target,
            "Hold side buttons: select task. Middle: done or reopen.",
            MARGIN,
            i32::from(height) - 170,
            1,
        );
    }
}

impl Screen for NotesScreen {
    fn draw(&self, target: &mut Framebuffer<'_>, app: &App) {
        heading(target, "Notes");
        if app.daily.note().is_empty() {
            text(target, "No note saved", MARGIN, 300, 3);
            paragraph(
                target,
                "Set a note with note set TEXT over USB or in the preview.",
                MARGIN,
                420,
                2,
                60,
            );
        } else {
            paragraph(target, app.daily.note().as_str(), MARGIN, 300, 3, 90);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapping_preserves_words_and_progresses_for_unbroken_text() {
        assert_eq!(
            wrapped_line("Remember the library books.", 15),
            ("Remember the", "library books.")
        );
        assert_eq!(
            wrapped_line("abcdefghijklmnop", 10),
            ("abcdefghij", "klmnop")
        );
        assert_eq!(wrapped_line("done", 10), ("done", ""));
    }
}
