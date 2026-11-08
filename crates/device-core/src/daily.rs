pub const TASK_COUNT: usize = 6;
pub const TASK_TEXT_BYTES: usize = 48;
pub const NOTE_TEXT_BYTES: usize = 160;
pub const DAILY_BYTES: usize = TASK_COUNT * (TASK_TEXT_BYTES + 2) + NOTE_TEXT_BYTES + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text<const N: usize> {
    bytes: [u8; N],
    length: u8,
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            length: 0,
        }
    }
}

impl<const N: usize> Text<N> {
    pub fn new(value: &str) -> Option<Self> {
        if value.len() > N
            || value.len() > u8::MAX as usize
            || !value.bytes().all(|byte| (b' '..=b'~').contains(&byte))
        {
            return None;
        }
        let value = value.trim();
        let mut text = Self::default();
        text.bytes[..value.len()].copy_from_slice(value.as_bytes());
        text.length = value.len() as u8;
        Some(text)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.length)]).unwrap_or_default()
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    fn write(self, output: &mut [u8]) {
        output[0] = self.length;
        output[1..].copy_from_slice(&self.bytes);
    }

    fn read(input: &[u8]) -> Option<Self> {
        let length = usize::from(*input.first()?);
        let bytes: &[u8; N] = input.get(1..)?.try_into().ok()?;
        let used = bytes.get(..length)?;
        if used.first() == Some(&b' ')
            || used.last() == Some(&b' ')
            || !used.iter().all(|byte| (b' '..=b'~').contains(byte))
            || bytes[length..].iter().any(|byte| *byte != 0)
        {
            return None;
        }
        Some(Self {
            bytes: *bytes,
            length: length as u8,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskId(u8);

impl TaskId {
    pub fn new(number: u8) -> Option<Self> {
        (number >= 1 && usize::from(number) <= TASK_COUNT).then_some(Self(number))
    }
    pub const fn number(self) -> u8 {
        self.0
    }
    pub const fn index(self) -> usize {
        (self.0 - 1) as usize
    }
    pub fn step(self, forward: bool) -> Self {
        let index = if forward {
            (self.index() + 1) % TASK_COUNT
        } else {
            (self.index() + TASK_COUNT - 1) % TASK_COUNT
        };
        Self(index as u8 + 1)
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Task {
    pub text: Text<TASK_TEXT_BYTES>,
    pub done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyEdit {
    SetTask {
        id: TaskId,
        text: Text<TASK_TEXT_BYTES>,
    },
    SetDone {
        id: TaskId,
        done: bool,
    },
    Toggle(TaskId),
    Clear(TaskId),
    Note(Text<NOTE_TEXT_BYTES>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Daily {
    tasks: [Task; TASK_COUNT],
    note: Text<NOTE_TEXT_BYTES>,
}

impl Daily {
    pub const fn tasks(&self) -> &[Task; TASK_COUNT] {
        &self.tasks
    }
    pub fn task(self, id: TaskId) -> Task {
        self.tasks[id.index()]
    }
    pub const fn note(&self) -> &Text<NOTE_TEXT_BYTES> {
        &self.note
    }
    pub fn counts(self) -> (usize, usize) {
        let total = self
            .tasks
            .iter()
            .filter(|task| !task.text.is_empty())
            .count();
        let done = self
            .tasks
            .iter()
            .filter(|task| !task.text.is_empty() && task.done)
            .count();
        (done, total)
    }
    pub fn edit(mut self, edit: DailyEdit) -> Self {
        match edit {
            DailyEdit::SetTask { id, text } => {
                if self.tasks[id.index()].text != text {
                    self.tasks[id.index()] = Task { text, done: false };
                }
            }
            DailyEdit::SetDone { id, done } => {
                self.tasks[id.index()].done = done && !self.tasks[id.index()].text.is_empty();
            }
            DailyEdit::Toggle(id) => {
                let task = &mut self.tasks[id.index()];
                task.done = !task.done && !task.text.is_empty();
            }
            DailyEdit::Clear(id) => self.tasks[id.index()] = Task::default(),
            DailyEdit::Note(text) => self.note = text,
        }
        self
    }
    pub fn encode(self) -> [u8; DAILY_BYTES] {
        let mut bytes = [0; DAILY_BYTES];
        for (task, chunk) in self.tasks.iter().zip(bytes.chunks_mut(TASK_TEXT_BYTES + 2)) {
            chunk[0] = u8::from(task.done);
            task.text.write(&mut chunk[1..]);
        }
        self.note
            .write(&mut bytes[TASK_COUNT * (TASK_TEXT_BYTES + 2)..]);
        bytes
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != DAILY_BYTES {
            return None;
        }
        let mut daily = Self::default();
        for (task, chunk) in daily
            .tasks
            .iter_mut()
            .zip(bytes.chunks(TASK_TEXT_BYTES + 2))
        {
            let done = match chunk[0] {
                0 => false,
                1 => true,
                _ => return None,
            };
            let text = Text::read(&chunk[1..])?;
            if done && text.is_empty() {
                return None;
            }
            *task = Task { text, done };
        }
        daily.note = Text::read(&bytes[TASK_COUNT * (TASK_TEXT_BYTES + 2)..])?;
        Some(daily)
    }
}
