#[derive(Debug, PartialEq, Eq)]
pub enum Command<'a> {
    Help,
    Status,
    Diagnose,
    Refresh,
    Sleep,
    SetRtc(&'a str),
    SetInterval(u32),
    Resume(device_core::settings::ResumePolicy),
    WifiScan,
    WifiConnect { ssid: &'a str, password: &'a str },
    WifiDisconnect,
    Ble(bool),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    Unknown,
    Invalid,
    TooLong,
}

impl ParseError {
    pub const fn message(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown command. Enter help for supported commands.",
            Self::Invalid => "Invalid argument. Enter help for accepted formats and limits.",
            Self::TooLong => "Command too long. Send a shorter line, then enter help.",
        }
    }
}

pub fn parse(line: &str) -> Result<Command<'_>, ParseError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(ParseError::TooLong);
    }
    let line = line.trim_start().trim_end_matches(['\r', '\n']);
    if let Some(value) = line.strip_prefix("wifi connect ") {
        let (ssid, password) = value.split_once('\t').ok_or(ParseError::Invalid)?;
        return if wifi_credential_lengths_valid(ssid.len(), password.len()) {
            Ok(Command::WifiConnect { ssid, password })
        } else {
            Err(ParseError::Invalid)
        };
    }
    let line = line.trim();
    match line {
        "help" => Ok(Command::Help),
        "status" => Ok(Command::Status),
        "diagnose" => Ok(Command::Diagnose),
        "refresh" => Ok(Command::Refresh),
        "sleep" => Ok(Command::Sleep),
        "wifi scan" => Ok(Command::WifiScan),
        "wifi disconnect" => Ok(Command::WifiDisconnect),
        "ble on" => Ok(Command::Ble(true)),
        "ble off" => Ok(Command::Ble(false)),
        _ => {
            if let Some(value) = line.strip_prefix("resume ") {
                return resume_policy(value)
                    .map(Command::Resume)
                    .ok_or(ParseError::Invalid);
            }
            if let Some(value) = line.strip_prefix("interval ") {
                let value = value.parse::<u32>().map_err(|_| ParseError::Invalid)?;
                return if (MIN_REFRESH_SECONDS..=MAX_REFRESH_SECONDS).contains(&value) {
                    Ok(Command::SetInterval(value))
                } else {
                    Err(ParseError::Invalid)
                };
            }
            if let Some(value) = line.strip_prefix("rtc ") {
                return if device_core::telemetry::DateTime::parse(value).is_some() {
                    Ok(Command::SetRtc(value))
                } else {
                    Err(ParseError::Invalid)
                };
            }
            Err(ParseError::Unknown)
        }
    }
}

pub(crate) const fn wifi_credential_lengths_valid(ssid: usize, password: usize) -> bool {
    ssid > 0 && ssid <= MAX_SSID_BYTES && password <= MAX_PASSWORD_BYTES
}

pub(crate) fn resume_policy(value: &str) -> Option<device_core::settings::ResumePolicy> {
    use device_core::settings::ResumePolicy;
    match value {
        "last" => Some(ResumePolicy::Last),
        "status" => Some(ResumePolicy::Status),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineResult {
    Pending,
    Complete,
    TooLong,
}

pub struct LineInput {
    bytes: [u8; MAX_LINE_BYTES],
    length: usize,
    discarding: bool,
    complete: bool,
}
impl Default for LineInput {
    fn default() -> Self {
        Self {
            bytes: [0; MAX_LINE_BYTES],
            length: 0,
            discarding: false,
            complete: false,
        }
    }
}
impl LineInput {
    #[cfg(any(test, kani))]
    pub(crate) fn proof_state(length: usize, discarding: bool, complete: bool) -> Self {
        Self {
            bytes: [0; MAX_LINE_BYTES],
            length,
            discarding,
            complete,
        }
    }
    #[cfg(any(test, kani))]
    pub(crate) fn proof_components(&self) -> (usize, bool, bool) {
        (self.length, self.discarding, self.complete)
    }
    #[cfg(any(test, kani))]
    pub(crate) fn invariant(&self) -> bool {
        self.length <= self.bytes.len()
            && (!self.discarding || self.length == 0)
            && (!self.complete || (!self.discarding && self.length > 0))
    }
    pub fn push(&mut self, byte: u8) -> LineResult {
        if self.complete {
            self.bytes.fill(0);
            self.length = 0;
            self.complete = false;
        }
        if byte == b'\n' || byte == b'\r' {
            if self.discarding {
                self.discarding = false;
                return LineResult::Pending;
            }
            if self.length == 0 {
                return LineResult::Pending;
            }
            self.complete = true;
            return LineResult::Complete;
        }
        if self.discarding {
            return LineResult::Pending;
        }
        if self.length == self.bytes.len() {
            self.bytes.fill(0);
            self.length = 0;
            self.discarding = true;
            return LineResult::TooLong;
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        LineResult::Pending
    }
    pub fn line(&self) -> Result<&str, ParseError> {
        core::str::from_utf8(&self.bytes[..self.length]).map_err(|_| ParseError::Invalid)
    }
}
use device_core::settings::{MAX_REFRESH_SECONDS, MIN_REFRESH_SECONDS};
const MAX_LINE_BYTES: usize = 256;
const MAX_SSID_BYTES: usize = 32;
const MAX_PASSWORD_BYTES: usize = 63;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_state_and_invariant_preserve_the_declared_input_domain() {
        for length in (0..=MAX_LINE_BYTES + 1).chain(core::iter::once(usize::MAX)) {
            for discarding in [false, true] {
                for complete in [false, true] {
                    let state = LineInput::proof_state(length, discarding, complete);
                    assert_eq!(
                        (state.length, state.discarding, state.complete),
                        (length, discarding, complete)
                    );
                    assert_eq!(state.proof_components(), (length, discarding, complete));
                    let valid = match (length, discarding, complete) {
                        (0, _, false) => true,
                        (length, false, _) => (1..=MAX_LINE_BYTES).contains(&length),
                        _ => false,
                    };
                    assert_eq!(state.invariant(), valid);
                    if valid {
                        for byte in 0..=u8::MAX {
                            let mut input = LineInput::proof_state(length, discarding, complete);
                            input.push(byte);
                            assert!(input.invariant());
                        }
                    }
                }
            }
        }
    }
}
