/// EPICS alarm severity, matching caQtDM `alarmdefs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Severity {
    #[default]
    NoAlarm,
    Minor,
    Major,
    Invalid,
    /// Widget has no live connection. caQtDM uses 99 for this state.
    NotConnected,
}

impl Severity {
    pub fn from_epics(code: i16) -> Self {
        match code {
            0 => Self::NoAlarm,
            1 => Self::Minor,
            2 => Self::Major,
            3 => Self::Invalid,
            _ => Self::Invalid,
        }
    }

    pub fn as_i16(self) -> i16 {
        match self {
            Self::NoAlarm => 0,
            Self::Minor => 1,
            Self::Major => 2,
            Self::Invalid => 3,
            Self::NotConnected => 99,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::NoAlarm => "NO_ALARM",
            Self::Minor => "MINOR",
            Self::Major => "MAJOR",
            Self::Invalid => "INVALID",
            Self::NotConnected => "NOT_CONNECTED",
        }
    }
}
