//! Locale-agnostic currency-string parsing: handles prefix or suffix
//! currency symbols (£18.48, 0,03€) and either comma or dot as the decimal
//! separator, distinguishing it from a thousands separator by how many
//! digits follow it.

/// Parses a currency-formatted string into an `f64`, stripping any
/// surrounding symbol/whitespace. Returns `None` if no digits are present.
pub fn parse_amount(raw: &str) -> Option<f64> {
    let filtered: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();

    if filtered.is_empty() {
        return None;
    }

    match filtered.rfind([',', '.']) {
        None => filtered.parse::<f64>().ok(),
        Some(pos) => {
            let digits_after = filtered.len() - pos - 1;
            if digits_after == 1 || digits_after == 2 {
                // The last separator is a decimal point; anything before it
                // (including any other separator) is thousands-grouping.
                let mut integer_part: String =
                    filtered[..pos].chars().filter(char::is_ascii_digit).collect();
                if integer_part.is_empty() {
                    integer_part.push('0');
                }
                let fractional_part = &filtered[pos + 1..];
                format!("{integer_part}.{fractional_part}").parse::<f64>().ok()
            } else {
                // No decimal part — every separator present is thousands-grouping.
                let digits_only: String = filtered.chars().filter(char::is_ascii_digit).collect();
                if digits_only.is_empty() {
                    None
                } else {
                    digits_only.parse::<f64>().ok()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prefix_pound_with_thousands_separator() {
        assert_eq!(parse_amount("£18.48"), Some(18.48));
        assert_eq!(parse_amount("£1,234.56"), Some(1234.56));
    }

    #[test]
    fn parses_suffix_euro_with_comma_decimal() {
        assert_eq!(parse_amount("0,03€"), Some(0.03));
    }

    #[test]
    fn parses_dot_thousands_comma_decimal() {
        assert_eq!(parse_amount("1.234,56€"), Some(1234.56));
    }

    #[test]
    fn parses_prefix_dollar() {
        assert_eq!(parse_amount("$9.99"), Some(9.99));
    }

    #[test]
    fn returns_none_for_text_without_digits() {
        assert_eq!(parse_amount("not a price"), None);
    }

    #[test]
    fn parses_thousands_only_with_no_decimal_part() {
        assert_eq!(parse_amount("£1,234"), Some(1234.0));
    }
}
