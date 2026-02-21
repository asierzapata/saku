use jiff::civil::Date;

use crate::models::task::{Freq, MonthlyAnchor, Recurrence, SerdeWeekday};

#[derive(Debug, Clone, PartialEq)]
pub enum RecurrenceParseError {
    UnknownPattern(String),
}

impl std::fmt::Display for RecurrenceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecurrenceParseError::UnknownPattern(p) => write!(
                f,
                "Unknown recurrence pattern: '{}'. \
                Supported: daily, weekly, monthly, yearly, \
                a weekday (monday, mon,wed,fri), \
                a day of month (1st of month, 15th of month), \
                or a weekday of month (1st monday of month, last friday of month)",
                p
            ),
        }
    }
}

impl std::error::Error for RecurrenceParseError {}

/// Parse a user-supplied `--every` string into a `Recurrence`.
/// `dtstart` is the task's scheduled date (or today), used as the anchor.
pub fn parse_recurrence(
    input: &str,
    dtstart: Date,
) -> Result<Recurrence, RecurrenceParseError> {
    let s = input.trim().to_lowercase();

    // -- Simple keywords --
    match s.as_str() {
        "daily" | "day" | "every day" => {
            return Ok(Recurrence {
                freq: Freq::Daily,
                weekdays: vec![],
                monthly_anchor: None,
                until: None,
                dtstart,
            });
        }
        "weekly" | "week" | "every week" => {
            return Ok(Recurrence {
                freq: Freq::Weekly,
                weekdays: vec![],
                monthly_anchor: None,
                until: None,
                dtstart,
            });
        }
        "monthly" | "month" | "every month" => {
            return Ok(Recurrence {
                freq: Freq::Monthly,
                weekdays: vec![],
                monthly_anchor: Some(MonthlyAnchor::DayOfMonth {
                    day: dtstart.day() as u8,
                }),
                until: None,
                dtstart,
            });
        }
        "yearly" | "year" | "every year" | "annually" => {
            return Ok(Recurrence {
                freq: Freq::Weekly,
                weekdays: vec![],
                monthly_anchor: None,
                until: None,
                dtstart,
            }
            .into_yearly());
        }
        _ => {}
    }

    // -- "last <weekday> of month" --
    if let Some(rest) = s
        .strip_prefix("last ")
        .and_then(|r| r.strip_suffix(" of month"))
    {
        let weekday = parse_weekday(rest.trim())
            .ok_or_else(|| RecurrenceParseError::UnknownPattern(input.to_string()))?;
        return Ok(Recurrence {
            freq: Freq::Monthly,
            weekdays: vec![],
            monthly_anchor: Some(MonthlyAnchor::LastWeekday { weekday }),
            until: None,
            dtstart,
        });
    }

    // -- "<ordinal> <weekday> of month" e.g. "1st monday of month" --
    if let Some(rest) = s.strip_suffix(" of month") {
        let parts: Vec<&str> = rest.trim().splitn(2, ' ').collect();
        if parts.len() == 2 {
            if let Some(nth) = parse_ordinal(parts[0]).filter(|&n| (1..=5).contains(&n)) {
                if let Some(weekday) = parse_weekday(parts[1].trim()) {
                    return Ok(Recurrence {
                        freq: Freq::Monthly,
                        weekdays: vec![],
                        monthly_anchor: Some(MonthlyAnchor::NthWeekday { nth, weekday }),
                        until: None,
                        dtstart,
                    });
                }
            }
            // "<ordinal> of month" — e.g. "1st of month" (parts[1] would be empty after strip)
            // handled below
        }

        // "<ordinal> of month" (no weekday part, bare ordinal)
        if let Some(nth) = parse_ordinal(rest.trim()) {
            return Ok(Recurrence {
                freq: Freq::Monthly,
                weekdays: vec![],
                monthly_anchor: Some(MonthlyAnchor::DayOfMonth { day: nth }),
                until: None,
                dtstart,
            });
        }
    }

    // -- Single weekday: "monday" / "mon" --
    if let Some(weekday) = parse_weekday(&s) {
        return Ok(Recurrence {
            freq: Freq::Weekly,
            weekdays: vec![weekday],
            monthly_anchor: None,
            until: None,
            dtstart,
        });
    }

    // -- Comma-separated weekdays: "mon,wed,fri" --
    if s.contains(',') {
        let parts: Vec<&str> = s.split(',').collect();
        let mut weekdays = Vec::new();
        for part in &parts {
            match parse_weekday(part.trim()) {
                Some(w) => weekdays.push(w),
                None => return Err(RecurrenceParseError::UnknownPattern(input.to_string())),
            }
        }
        if !weekdays.is_empty() {
            return Ok(Recurrence {
                freq: Freq::Weekly,
                weekdays,
                monthly_anchor: None,
                until: None,
                dtstart,
            });
        }
    }

    Err(RecurrenceParseError::UnknownPattern(input.to_string()))
}

// Helper to convert a weekly Recurrence placeholder into Yearly
trait IntoYearly {
    fn into_yearly(self) -> Recurrence;
}

impl IntoYearly for Recurrence {
    fn into_yearly(mut self) -> Recurrence {
        self.freq = Freq::Yearly;
        self
    }
}

/// Parse ordinal strings: "1st"→1, "2nd"→2, "3rd"→3, "4th"→4, "5th"→5
/// Also accepts bare numbers: "1"→1, "15"→15.
/// Returns `None` for values outside 1..=31 (the widest valid range for any calendar field).
fn parse_ordinal(s: &str) -> Option<u8> {
    let stripped = s
        .trim_end_matches("st")
        .trim_end_matches("nd")
        .trim_end_matches("rd")
        .trim_end_matches("th");
    let value = stripped.parse::<u8>().ok()?;
    if (1..=31).contains(&value) { Some(value) } else { None }
}

fn parse_weekday(s: &str) -> Option<SerdeWeekday> {
    match s {
        "monday" | "mon" => Some(SerdeWeekday::Monday),
        "tuesday" | "tue" | "tues" => Some(SerdeWeekday::Tuesday),
        "wednesday" | "wed" => Some(SerdeWeekday::Wednesday),
        "thursday" | "thu" | "thur" | "thurs" => Some(SerdeWeekday::Thursday),
        "friday" | "fri" => Some(SerdeWeekday::Friday),
        "saturday" | "sat" => Some(SerdeWeekday::Saturday),
        "sunday" | "sun" => Some(SerdeWeekday::Sunday),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn dtstart() -> Date {
        date(2026, 2, 16) // a Monday
    }

    #[test]
    fn test_daily() {
        let r = parse_recurrence("daily", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Daily);
        assert!(r.weekdays.is_empty());
    }

    #[test]
    fn test_weekly() {
        let r = parse_recurrence("weekly", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Weekly);
        assert!(r.weekdays.is_empty());
    }

    #[test]
    fn test_monthly() {
        let r = parse_recurrence("monthly", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Monthly);
        assert_eq!(
            r.monthly_anchor,
            Some(MonthlyAnchor::DayOfMonth { day: 16 })
        );
    }

    #[test]
    fn test_yearly() {
        let r = parse_recurrence("yearly", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Yearly);
    }

    #[test]
    fn test_single_weekday() {
        let r = parse_recurrence("monday", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Weekly);
        assert_eq!(r.weekdays, vec![SerdeWeekday::Monday]);
    }

    #[test]
    fn test_abbreviated_weekday() {
        let r = parse_recurrence("fri", dtstart()).unwrap();
        assert_eq!(r.weekdays, vec![SerdeWeekday::Friday]);
    }

    #[test]
    fn test_multi_weekdays() {
        let r = parse_recurrence("mon,wed,fri", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Weekly);
        assert_eq!(
            r.weekdays,
            vec![SerdeWeekday::Monday, SerdeWeekday::Wednesday, SerdeWeekday::Friday]
        );
    }

    #[test]
    fn test_day_of_month() {
        let r = parse_recurrence("1st of month", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Monthly);
        assert_eq!(r.monthly_anchor, Some(MonthlyAnchor::DayOfMonth { day: 1 }));
    }

    #[test]
    fn test_day_of_month_15th() {
        let r = parse_recurrence("15th of month", dtstart()).unwrap();
        assert_eq!(
            r.monthly_anchor,
            Some(MonthlyAnchor::DayOfMonth { day: 15 })
        );
    }

    #[test]
    fn test_nth_weekday_of_month() {
        let r = parse_recurrence("1st monday of month", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Monthly);
        assert_eq!(
            r.monthly_anchor,
            Some(MonthlyAnchor::NthWeekday {
                nth: 1,
                weekday: SerdeWeekday::Monday
            })
        );
    }

    #[test]
    fn test_last_weekday_of_month() {
        let r = parse_recurrence("last friday of month", dtstart()).unwrap();
        assert_eq!(r.freq, Freq::Monthly);
        assert_eq!(
            r.monthly_anchor,
            Some(MonthlyAnchor::LastWeekday {
                weekday: SerdeWeekday::Friday
            })
        );
    }

    #[test]
    fn test_case_insensitive() {
        assert!(parse_recurrence("DAILY", dtstart()).is_ok());
        assert!(parse_recurrence("Monday", dtstart()).is_ok());
        assert!(parse_recurrence("1st Monday of Month", dtstart()).is_ok());
        assert!(parse_recurrence("Last Friday of Month", dtstart()).is_ok());
    }

    #[test]
    fn test_whitespace_trimmed() {
        assert!(parse_recurrence("  daily  ", dtstart()).is_ok());
        assert!(parse_recurrence(" monday ", dtstart()).is_ok());
    }

    #[test]
    fn test_unknown_pattern_error() {
        let err = parse_recurrence("every other tuesday", dtstart());
        assert!(matches!(err, Err(RecurrenceParseError::UnknownPattern(_))));
    }

    #[test]
    fn test_multi_weekdays_with_spaces() {
        let r = parse_recurrence("mon, wed, fri", dtstart()).unwrap();
        assert_eq!(r.weekdays.len(), 3);
    }
}
