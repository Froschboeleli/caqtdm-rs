/// Display formats from caQtDM `formattype.h`, plus the user printf-like format
/// some line monitors accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatType {
    #[default]
    Decimal,
    Exponential,
    EngrNotation,
    Compact,
    /// Signed integer truncation, caQtDM `truncated`.
    Truncated,
    /// Unsigned integer truncation, caQtDM `utruncated`.
    UTruncated,
    Hexadecimal,
    Octal,
    String,
    Sexagesimal,
    SexagesimalHms,
    SexagesimalDms,
    /// Enum displayed as its integer index.
    Enumeric,
}

impl FormatType {
    pub fn parse(token: &str) -> Self {
        match token
            .trim()
            .rsplit("::")
            .next()
            .unwrap_or(token)
            .to_ascii_lowercase()
            .as_str()
        {
            "decimal" => Self::Decimal,
            "exponential" => Self::Exponential,
            "engr_notation" | "engr" | "engineering" => Self::EngrNotation,
            "compact" => Self::Compact,
            "truncated" => Self::Truncated,
            "utruncated" => Self::UTruncated,
            "hexadecimal" | "hex" => Self::Hexadecimal,
            "octal" => Self::Octal,
            "string" => Self::String,
            "sexagesimal" => Self::Sexagesimal,
            "sexagesimal_hms" => Self::SexagesimalHms,
            "sexagesimal_dms" => Self::SexagesimalDms,
            "enumeric" => Self::Enumeric,
            _ => Self::Decimal,
        }
    }
}

/// Format `value` the way caLineEdit does. `precision` is the number of digits
/// after the decimal point for floating formats. NaN is the literal `nan`.
pub fn format_number(value: f64, format: FormatType, precision: i32) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    let prec = precision.clamp(0, 17) as usize;
    match format {
        FormatType::Decimal | FormatType::String => format!("{value:.prec$}"),
        FormatType::Exponential => format_exp(value, prec),
        FormatType::EngrNotation => format_engr(value, prec),
        FormatType::Compact => {
            let abs = value.abs();
            if abs == 0.0 || (1e-4..1e4).contains(&abs) {
                format!("{value:.prec$}")
            } else {
                format_exp(value, prec)
            }
        }
        FormatType::Truncated | FormatType::Enumeric => format!("{}", value as i64),
        FormatType::UTruncated => format!("{}", value as u64),
        FormatType::Hexadecimal => format!("0x{:x}", value as i64),
        FormatType::Octal => format!("O{:o}", value as i64),
        FormatType::Sexagesimal => format_sexa(value, prec, SexaStyle::Colon),
        FormatType::SexagesimalHms => format_sexa(value, prec, SexaStyle::Hms),
        FormatType::SexagesimalDms => format_sexa(value, prec, SexaStyle::Dms),
    }
}

fn format_exp(value: f64, prec: usize) -> String {
    let raw = format!("{value:.prec$e}");
    let Some((mantissa, exp)) = raw.split_once('e') else {
        return raw;
    };
    let exp_num: i32 = exp.parse().unwrap_or(0);
    let sign = if exp_num < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exp_num.abs())
}

fn format_engr(value: f64, prec: usize) -> String {
    if value == 0.0 {
        return format!(" {0:.prec$}e+00", 0.0);
    }
    let negative = value.is_sign_negative();
    let abs = value.abs();
    let exp = abs.log10().floor() as i32;
    let mut eng = exp.div_euclid(3) * 3;
    let mut mantissa = abs / 10f64.powi(eng);
    if mantissa >= 1000.0 {
        mantissa /= 1000.0;
        eng += 3;
    } else if mantissa < 1.0 {
        mantissa *= 1000.0;
        eng -= 3;
    }
    let sign = if negative { "-" } else { " " };
    format!("{sign}{mantissa:.prec$}e{eng:+03}")
}

enum SexaStyle {
    Colon,
    Hms,
    Dms,
}

fn format_sexa(value: f64, prec: usize, style: SexaStyle) -> String {
    let negative = value.is_sign_negative();
    let abs = value.abs();
    let deg = abs.floor();
    let minutes_full = (abs - deg) * 60.0;
    let minutes = minutes_full.floor();
    let seconds = (minutes_full - minutes) * 60.0;
    let sign = if negative { "-" } else { "" };
    match style {
        SexaStyle::Colon => format!("{sign}{deg:.0}:{minutes:02.0}:{seconds:.prec$}"),
        SexaStyle::Hms => format!("{sign}{deg:.0}h {minutes:02.0}m {seconds:.prec$}s"),
        SexaStyle::Dms => format!("{sign}{deg:.0}d {minutes:02.0}' {seconds:.prec$}''"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_compact_and_integers() {
        assert_eq!(format_number(1.5, FormatType::Decimal, 2), "1.50");
        assert_eq!(format_number(12.9, FormatType::Truncated, 0), "12");
        assert_eq!(format_number(-3.2, FormatType::Truncated, 0), "-3");
        assert_eq!(format_number(255.0, FormatType::Hexadecimal, 0), "0xff");
        assert_eq!(format_number(8.0, FormatType::Octal, 0), "O10");
        assert_eq!(format_number(1.25e6, FormatType::Compact, 2), "1.25e+06");
        assert_eq!(format_number(12.0, FormatType::Compact, 1), "12.0");
        assert_eq!(format_number(f64::NAN, FormatType::Decimal, 2), "nan");
    }

    #[test]
    fn engineering_uses_multiples_of_three() {
        let s = format_number(12345.0, FormatType::EngrNotation, 3);
        assert!(s.contains("e+03"), "{s}");
    }
}
