# Daily use

The firmware provides Device Status, Calendar, Checklist, Notes, Diagnostics, Drawing, and Gallery screens.
Daily operation works without Wi-Fi, Bluetooth Low Energy, an SD card, or a host computer.
USB provides text entry and initial configuration.

## Device controls

Use a short left or right press to change screens outside Gallery.
Gallery uses short side presses for images and held side presses for screens.
Hold a button for 800 ms to perform its held action once.
A short press takes effect after release.
Hold the middle button to measure and refresh from any screen.

On Calendar, hold a side button to browse the previous or following month.
Tap the middle button to return to the current month.
The calendar uses Monday as the first weekday and marks today only when the clock has a fresh valid reading.
The supported dates run from 2000 through 2099.

On Checklist, hold a side button to select one of six entries.
Tap the middle button to mark the selected task done or reopen it.
An empty selection uses the middle button for refresh.
Changing a task's text resets its completion state.
Setting identical text preserves completion.

## Configure daily content

Connect the USB console at 115200 baud and enter `help` for supported commands.
Configure the device clock and daily content with commands such as the following.

```text
rtc 2026-11-08T09:00:00
task set 1 Buy groceries
task set 2 Return library books
task done 1
note set Pack lunch and take the umbrella.
screen checklist
calendar 2028-02
calendar today
```

Task text accepts 1 through 48 printable ASCII characters.
A note accepts up to 160 printable ASCII characters and wraps across display lines.
The parser rejects unsupported characters, control characters, and out-of-range task numbers before changing state.
The shared font supports English text.

Use `task open 1`, `task toggle 1`, or `task clear 1` to change an entry.
Use `calendar YYYY-MM` to select a month and open Calendar, or `calendar today` to follow the device clock.
Use `note clear` to remove the note.
Use `note set clear` to save the literal word `clear`.
`screen` accepts `status`, `calendar`, `checklist`, `notes`, `diagnostics`, `drawing`, and `gallery`.
`rotation 0` through `rotation 3` select the four orientations.
`interval SECONDS` accepts 60 through 86400 seconds.
`resume last` restores the selected screen, while `resume status` opens Device Status after restart.
`status` reports daily content and whether content matches the last successful save.

## Saving and recovery

The firmware saves content, selected task, viewed month, orientation, refresh interval, and screen policy to internal flash before sleep.
Unchanged timer wakes avoid flash writes.
Settings use two transactional records, so an interrupted save preserves a committed record.
Earlier settings formats remain readable with empty daily content.
Edits during a save remain pending until the next save.
Power loss before a successful save can restore earlier content.
A settings failure appears as `SettingsStorage` in diagnostics and console output.

## Preview and backup

Run `mise x -- just web` to open the browser preview at the printed address.
Open the Daily workspace to edit the checklist and note or preview the calendar.
Choose To-do list, Note, or Calendar to show its editor.
Enter a task, then press Enter.
Each task has a checkbox, an Edit pencil, and a Delete trash icon.
Use Space on a checkbox to complete or reopen its task.
Save an inline edit with Enter or choose Save, and cancel with Escape or Cancel.
Undo restores deleted tasks, including their completion states, while the original positions remain available.
Clear completed removes completed tasks together.
The list holds six tasks.
The addition form disappears when the list is full.
Unfinished additions, inline edits, and note drafts survive workspace changes.
Choose Save note to update the note preview, or Discard changes to return to its saved text.
Send to display validates every pending task and note draft before applying and sending the content.
Edits made during sending remain in the editor, with a message asking for another send.
The calendar preview uses the computer's local date.
Previous month, Next month, and Today navigate the calendar.
Sending a calendar also sets the display clock to the captured local date and time and selects the shown month.
Invalid input preserves the current state, displayed image, and recording.
The preview runs locally and sends no daily content to a service.

Actions and fields use monochrome icons with text labels or names for assistive technology.
Task editing and deletion use named icon buttons beside their task.
Native checkboxes and radio buttons keep their standard state indicators.
The app explains unavailable USB sending beside the send action.
The calendar keeps its weekday relationships in a table that supports horizontal keyboard scrolling when enlarged text needs extra space.

Add `?tools` to the app address to reveal simulated measurements, held-button controls, and event recording.
The held-button controls perform the same gestures as the device.
Show the event recording and copy the log to save a development preview session.
Replay restores a session only after every line passes validation.
The event log can contain personal content, so store and share it accordingly.
Browser edits affect the preview.
Send to display applies daily content to the connected device.
USB commands also configure the physical device.

Run `mise x -- just sim` for the desktop preview.
Left, Space, and Right perform the three short presses.
Hold Shift while pressing a key to perform the corresponding held action.
Use `--event 'screen calendar'` or a replay log to select initial content.
