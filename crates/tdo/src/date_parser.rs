use jiff::civil::Date;
use jiff::Zoned;

#[derive(Debug, Clone, PartialEq)]
pub enum DateParseError {
    InvalidFormat(String),
    UnknownKeyword(String),
}

impl std::fmt::Display for DateParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DateParseError::InvalidFormat(msg) => write!(f, "Invalid date format: {}", msg),
            DateParseError::UnknownKeyword(keyword) => {
                write!(f, "Unknown date keyword: '{}'. Supported: today, tomorrow, monday-sunday, next week, next <weekday>, or ISO format (YYYY-MM-DD)", keyword)
            }
        }
    }
}

impl std::error::Error for DateParseError {}

/// Parse a natural language date string into a jiff Date
///
/// Supported formats:
/// - "today" - current date
/// - "tomorrow" - next day
/// - "monday", "tuesday", etc. - next occurrence (minimum 1 day away)
/// - "next monday", "next tuesday", etc. - explicit next week occurrence
/// - "next week" - Monday of next week (1-7 days away)
/// - ISO dates: "2026-03-15"
pub fn parse_natural_date(input: &str) -> Result<Date, DateParseError> {
    let input = input.trim().to_lowercase();
    let today = Zoned::now().date();

    match input.as_str() {
        "today" => Ok(today),
        "tomorrow" => Ok(today
            .checked_add(jiff::Span::new().days(1))
            .map_err(|e| DateParseError::InvalidFormat(e.to_string()))?),

        // Handle "next week" -> Monday of next week
        "next week" => {
            let days_until_next_monday = match today.weekday() {
                jiff::civil::Weekday::Monday => 7,
                jiff::civil::Weekday::Tuesday => 6,
                jiff::civil::Weekday::Wednesday => 5,
                jiff::civil::Weekday::Thursday => 4,
                jiff::civil::Weekday::Friday => 3,
                jiff::civil::Weekday::Saturday => 2,
                jiff::civil::Weekday::Sunday => 1,
            };
            Ok(today
                .checked_add(jiff::Span::new().days(days_until_next_monday))
                .map_err(|e| DateParseError::InvalidFormat(e.to_string()))?)
        }

        // Handle weekday names (minimum 1 day away)
        weekday if is_weekday(weekday) => {
            let target_weekday = parse_weekday(weekday)?;
            let days_ahead = calculate_days_until_weekday(today, target_weekday, true);
            Ok(today
                .checked_add(jiff::Span::new().days(days_ahead))
                .map_err(|e| DateParseError::InvalidFormat(e.to_string()))?)
        }

        // Handle "next <weekday>" format
        _ if input.starts_with("next ") => {
            let weekday_part = input.strip_prefix("next ").unwrap();
            if is_weekday(weekday_part) {
                let target_weekday = parse_weekday(weekday_part)?;
                // For "next <weekday>", always go to next week's occurrence (minimum 1 day)
                let days_ahead = calculate_days_until_weekday(today, target_weekday, true);
                Ok(today
                    .checked_add(jiff::Span::new().days(days_ahead))
                    .map_err(|e| DateParseError::InvalidFormat(e.to_string()))?)
            } else {
                Err(DateParseError::UnknownKeyword(input.clone()))
            }
        }

        // Try parsing as ISO date (YYYY-MM-DD) or other jiff-supported formats
        _ => input
            .parse::<Date>()
            .map_err(|_| DateParseError::UnknownKeyword(input.clone())),
    }
}

fn is_weekday(s: &str) -> bool {
    matches!(
        s,
        "monday"
            | "mon"
            | "tuesday"
            | "tue"
            | "tues"
            | "wednesday"
            | "wed"
            | "thursday"
            | "thu"
            | "thur"
            | "thurs"
            | "friday"
            | "fri"
            | "saturday"
            | "sat"
            | "sunday"
            | "sun"
    )
}

fn parse_weekday(s: &str) -> Result<jiff::civil::Weekday, DateParseError> {
    use jiff::civil::Weekday;

    match s {
        "monday" | "mon" => Ok(Weekday::Monday),
        "tuesday" | "tue" | "tues" => Ok(Weekday::Tuesday),
        "wednesday" | "wed" => Ok(Weekday::Wednesday),
        "thursday" | "thu" | "thur" | "thurs" => Ok(Weekday::Thursday),
        "friday" | "fri" => Ok(Weekday::Friday),
        "saturday" | "sat" => Ok(Weekday::Saturday),
        "sunday" | "sun" => Ok(Weekday::Sunday),
        _ => Err(DateParseError::UnknownKeyword(s.to_string())),
    }
}

/// Calculate days until target weekday
/// If min_one_day is true, result is always >= 1 (if today is Monday and target is Monday, returns 7)
/// If min_one_day is false, result can be 0 (if today is target day)
fn calculate_days_until_weekday(
    from: Date,
    target: jiff::civil::Weekday,
    min_one_day: bool,
) -> i64 {
    let current_weekday = from.weekday();
    let current = current_weekday as i64;
    let target = target as i64;

    let mut days = (target - current + 7) % 7;

    // If min_one_day is true and days is 0, set it to 7 (next week)
    if min_one_day && days == 0 {
        days = 7;
    }

    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_today_tomorrow() {
        let today = Zoned::now().date();
        let tomorrow = today.checked_add(jiff::Span::new().days(1)).unwrap();

        assert_eq!(parse_natural_date("today").unwrap(), today);
        assert_eq!(parse_natural_date("tomorrow").unwrap(), tomorrow);
    }

    #[test]
    fn test_iso_date() {
        let date = parse_natural_date("2026-03-15").unwrap();
        assert_eq!(date.to_string(), "2026-03-15");
    }

    #[test]
    fn test_weekday_minimum_one_day() {
        // If today is Monday, "monday" should give next Monday (7 days)
        // This is tested conceptually - actual behavior depends on when test runs
        let result = parse_natural_date("friday");
        assert!(result.is_ok());

        let today = Zoned::now().date();
        let parsed_date = result.unwrap();

        // Ensure it's at least 1 day in the future
        assert!(parsed_date > today);
    }

    #[test]
    fn test_next_week() {
        let today = Zoned::now().date();
        let result = parse_natural_date("next week").unwrap();

        // Should be Monday and between 1-7 days away
        assert_eq!(result.weekday(), jiff::civil::Weekday::Monday);

        let days_diff = result.since(today).unwrap().get_days();
        assert!(days_diff >= 1 && days_diff <= 7);
    }

    #[test]
    fn test_invalid_input() {
        let result = parse_natural_date("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_case_insensitive() {
        // Should handle various capitalizations
        assert!(parse_natural_date("TODAY").is_ok());
        assert!(parse_natural_date("Tomorrow").is_ok());
        assert!(parse_natural_date("FRIDAY").is_ok());
        assert!(parse_natural_date("Next Week").is_ok());
    }

    #[test]
    fn test_weekday_abbreviations() {
        // Test short forms
        assert!(parse_natural_date("mon").is_ok());
        assert!(parse_natural_date("tue").is_ok());
        assert!(parse_natural_date("wed").is_ok());
        assert!(parse_natural_date("thu").is_ok());
        assert!(parse_natural_date("fri").is_ok());
        assert!(parse_natural_date("sat").is_ok());
        assert!(parse_natural_date("sun").is_ok());
    }

    #[test]
    fn test_next_weekday_format() {
        let today = Zoned::now().date();

        // "next monday" should always be at least 1 day away
        let result = parse_natural_date("next monday").unwrap();
        assert!(result > today);
        assert_eq!(result.weekday(), jiff::civil::Weekday::Monday);

        // "next friday" should be a Friday
        let result = parse_natural_date("next friday").unwrap();
        assert_eq!(result.weekday(), jiff::civil::Weekday::Friday);
    }

    #[test]
    fn test_whitespace_handling() {
        // Should handle extra whitespace
        assert!(parse_natural_date("  today  ").is_ok());
        assert!(parse_natural_date(" tomorrow ").is_ok());
        assert!(parse_natural_date("  next week  ").is_ok());
    }

    #[test]
    fn test_calculate_days_until_weekday_logic() {
        // Create a known date for testing: Monday, Feb 16, 2026
        let monday = Date::new(2026, 2, 16).unwrap();

        // From Monday, requesting Monday with min_one_day=true should be 7 days
        assert_eq!(
            calculate_days_until_weekday(monday, jiff::civil::Weekday::Monday, true),
            7
        );

        // From Monday, requesting Tuesday should be 1 day
        assert_eq!(
            calculate_days_until_weekday(monday, jiff::civil::Weekday::Tuesday, true),
            1
        );

        // From Monday, requesting Sunday should be 6 days
        assert_eq!(
            calculate_days_until_weekday(monday, jiff::civil::Weekday::Sunday, true),
            6
        );

        // Test with min_one_day=false
        assert_eq!(
            calculate_days_until_weekday(monday, jiff::civil::Weekday::Monday, false),
            0
        );
    }

    #[test]
    fn test_error_messages() {
        match parse_natural_date("invalid_keyword") {
            Err(DateParseError::UnknownKeyword(kw)) => {
                assert_eq!(kw, "invalid_keyword");
            }
            _ => panic!("Expected UnknownKeyword error"),
        }
    }

    #[test]
    fn test_iso_date_formats() {
        // Test various ISO date formats
        assert!(parse_natural_date("2026-12-31").is_ok());
        assert!(parse_natural_date("2026-01-01").is_ok());

        // Invalid ISO dates should fail
        assert!(parse_natural_date("2026-13-01").is_err()); // Invalid month
        assert!(parse_natural_date("2026-02-30").is_err()); // Invalid day
    }
}
