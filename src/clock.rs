use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

fn utc_time(secs: u64) -> Time {
    let day = secs % 86_400;
    Time {
        hour: (day / 3600) as u32,
        minute: (day % 3600 / 60) as u32,
        second: (day % 60) as u32,
    }
}

#[cfg(unix)]
pub fn local_time() -> Time {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stamp = secs as libc::time_t;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    let converted = unsafe { !libc::localtime_r(&stamp, &mut parts).is_null() };
    if converted {
        Time {
            hour: parts.tm_hour as u32,
            minute: parts.tm_min as u32,
            second: parts.tm_sec as u32,
        }
    } else {
        utc_time(secs)
    }
}

#[cfg(not(unix))]
pub fn local_time() -> Time {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    utc_time(secs)
}

pub fn display_hour(hour: u32, hour12: bool) -> (u32, Option<&'static str>) {
    if !hour12 {
        return (hour, None);
    }
    let label = if hour < 12 { "AM" } else { "PM" };
    let shown = match hour % 12 {
        0 => 12,
        other => other,
    };
    (shown, Some(label))
}

pub fn format_clock(time: Time, hour12: bool, seconds: bool) -> String {
    let (hour, label) = display_hour(time.hour, hour12);
    let colon = if seconds || time.second & 1 == 0 {
        ':'
    } else {
        ' '
    };
    let mut out = if hour12 {
        format!("{hour}{colon}{:02}", time.minute)
    } else {
        format!("{hour:02}{colon}{:02}", time.minute)
    };
    if seconds {
        out.push_str(&format!(":{:02}", time.second));
    }
    if let Some(label) = label {
        out.push(' ');
        out.push_str(label);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32, second: u32) -> Time {
        Time {
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn hours() {
        assert_eq!(display_hour(0, false), (0, None));
        assert_eq!(display_hour(23, false), (23, None));
        assert_eq!(display_hour(0, true), (12, Some("AM")));
        assert_eq!(display_hour(9, true), (9, Some("AM")));
        assert_eq!(display_hour(12, true), (12, Some("PM")));
        assert_eq!(display_hour(13, true), (1, Some("PM")));
        assert_eq!(display_hour(23, true), (11, Some("PM")));
    }

    #[test]
    fn formats() {
        assert_eq!(format_clock(at(22, 38, 7), false, true), "22:38:07");
        assert_eq!(format_clock(at(7, 5, 9), false, true), "07:05:09");
        assert_eq!(format_clock(at(22, 38, 7), false, false), "22 38");
        assert_eq!(format_clock(at(22, 38, 8), false, false), "22:38");
        assert_eq!(format_clock(at(21, 5, 30), true, true), "9:05:30 PM");
        assert_eq!(format_clock(at(0, 0, 0), true, false), "12:00 AM");
        assert_eq!(format_clock(at(12, 59, 59), true, true), "12:59:59 PM");
    }

    #[test]
    fn utc_conversion() {
        assert_eq!(utc_time(86_400 * 3 + 3600 * 22 + 60 * 5 + 9), at(22, 5, 9));
    }

    #[test]
    fn local_time_is_valid() {
        let now = local_time();
        assert!(now.hour < 24 && now.minute < 60 && now.second < 62);
    }
}
