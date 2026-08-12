//! Best-effort year inference for Steam market history's year-less dates
//! (e.g. `"18 Jul"`). Rows arrive reverse-chronological (newest first), so a
//! walk-and-decrement-on-month-increase heuristic recovers the year: each
//! time a later row's month number is greater than the previous row's, that
//! signals a wrap to the previous year. Not guaranteed correct across long
//! gaps between transactions — the caller should let the user review/correct
//! the result rather than trust it silently (see
//! `.mallet/features/steam-session-import/state.md`).

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Parses a `"18 Jul"`-style string into `(day, month_number)`. The day/month
/// pair is read from the *last two* whitespace-separated tokens, so a
/// leading action label (Steam's actual `raw_date` shape is
/// `"Sold: 18 Jul"`/`"Purchased: 1 Jan"`) is tolerated without needing to be
/// stripped first.
fn parse_day_month(raw: &str) -> Option<(u32, u32)> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let month_name = tokens[tokens.len() - 1];
    let day: u32 = tokens[tokens.len() - 2].parse().ok()?;
    let month_number = MONTHS.iter().position(|m| *m == month_name)? as u32 + 1;
    Some((day, month_number))
}

/// Takes Steam's raw year-less date strings (reverse-chronological — newest
/// first) plus the year the newest row should start from, and returns each
/// row as an ISO `"YYYY-MM-DD"` string.
pub fn infer_years(raw_dates: &[String], current_year: i32) -> Vec<String> {
    let mut year = current_year;
    let mut previous_month: Option<u32> = None;
    let mut result = Vec::with_capacity(raw_dates.len());

    for raw in raw_dates {
        let Some((day, month)) = parse_day_month(raw) else {
            result.push(String::new());
            continue;
        };

        if let Some(prev) = previous_month {
            if month > prev {
                year -= 1;
            }
        }
        previous_month = Some(month);

        result.push(format!("{year:04}-{month:02}-{day:02}"));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_current_year_when_months_stay_non_increasing() {
        let raw = vec!["20 Jul".to_string(), "3 Jul".to_string(), "18 Jan".to_string()];
        let dates = infer_years(&raw, 2026);
        assert_eq!(dates, vec!["2026-07-20", "2026-07-03", "2026-01-18"]);
    }

    #[test]
    fn decrements_year_on_a_wrap_to_a_later_month() {
        // Reverse-chronological: 3 Jan 2026, then 20 Dec 2025, then 18 Jul 2025.
        let raw = vec!["3 Jan".to_string(), "20 Dec".to_string(), "18 Jul".to_string()];
        let dates = infer_years(&raw, 2026);
        assert_eq!(dates, vec!["2026-01-03", "2025-12-20", "2025-07-18"]);
    }

    #[test]
    fn tolerates_the_real_steam_action_label_prefix() {
        // steam_history.rs's SteamTransaction.raw_date is the full
        // "Sold: 18 Jul" text, not a bare "18 Jul" — confirmed against the
        // real sample fixture (see steam_history.rs's tests).
        let raw = vec!["Sold: 18 Jul".to_string(), "Purchased: 1 Jan".to_string()];
        let dates = infer_years(&raw, 2026);
        assert_eq!(dates, vec!["2026-07-18", "2026-01-01"]);
    }

    #[test]
    fn handles_an_unparseable_row_without_panicking() {
        let raw = vec!["not a date".to_string(), "18 Jul".to_string()];
        let dates = infer_years(&raw, 2026);
        assert_eq!(dates[0], "");
        assert_eq!(dates[1], "2026-07-18");
    }
}
