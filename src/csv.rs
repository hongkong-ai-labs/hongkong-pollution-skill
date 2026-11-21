//! Minimal CSV reading for the EPD, AFCD and WSD feeds.
//!
//! These feeds are simple RFC 4180 files with quoted fields, a UTF-8 BOM and
//! CRLF endings, which is not worth a dependency. [`rows`] returns owned fields
//! and silently drops short or blank lines; callers validate the header first.

/// Split one CSV line, honouring double quotes and doubled quote escapes.
#[must_use]
pub fn split_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            other => field.push(other),
        }
    }
    fields.push(field);
    fields
}

/// The header line and every non-empty following row, fields already split.
#[must_use]
pub fn rows(body: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let body = body.trim_start_matches('\u{feff}');
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .map(|line| {
            split_line(line)
                .into_iter()
                .map(|f| f.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let data = lines
        .map(|line| {
            split_line(line)
                .into_iter()
                .map(|field| field.trim().to_string())
                .collect::<Vec<String>>()
        })
        .filter(|fields| fields.iter().any(|field| !field.is_empty()))
        .collect();
    (header, data)
}

/// Index of the first header containing `needle`, case-insensitively.
#[must_use]
pub fn column(header: &[String], needle: &str) -> Option<usize> {
    header
        .iter()
        .position(|field| field.to_lowercase().contains(&needle.to_lowercase()))
}

/// Parse `2025-08-06` or `06/08/2025` into a compact `YYYY-MM-DD`.
#[must_use]
pub fn iso_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some((year, rest)) = raw.split_once('-') {
        let mut parts = rest.split('-');
        let month: u32 = parts.next()?.parse().ok()?;
        let day: u32 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || year.len() != 4 || !(1..=12).contains(&month) {
            return None;
        }
        return Some(format!("{year}-{month:02}-{day:02}"));
    }
    let mut parts = raw.split('/');
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    let day: u32 = day.parse().ok()?;
    let month: u32 = month.parse().ok()?;
    let year: u32 = year.parse().ok()?;
    if parts.next().is_some() || !(1..=31).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quotes_and_blank_lines() {
        assert_eq!(
            split_line("\"Anglers' Beach\",\"08/01/2025\",\"7\""),
            vec!["Anglers' Beach", "08/01/2025", "7"]
        );
        assert_eq!(split_line("\"a\"\"b\",c"), vec!["a\"b", "c"]);
        let (header, data) = rows("\u{feff}A,B\r\n1,2\r\n\r\n3,4\r\n");
        assert_eq!(header, vec!["A", "B"]);
        assert_eq!(data.len(), 2);
        assert_eq!(column(&header, "b"), Some(1));
    }

    #[test]
    fn parses_both_date_orders() {
        assert_eq!(iso_date("2025-08-06").as_deref(), Some("2025-08-06"));
        assert_eq!(iso_date("06/08/2025").as_deref(), Some("2025-08-06"));
        assert_eq!(iso_date("32/02/2025"), None);
        assert_eq!(iso_date("nonsense"), None);
    }
}
