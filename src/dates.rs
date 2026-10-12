use crate::Result;
use chrono::NaiveDate;

pub fn parse_due_date(input: &str) -> Result<Option<NaiveDate>> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }
    let valid_shape = input.len() == 10
        && input.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        });
    if !valid_shape || input.starts_with("0000") {
        return Err("Due date must be a valid YYYY-MM-DD date".into());
    }
    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .map_err(|_| "Due date must be a valid YYYY-MM-DD date")?;
    Ok(Some(date))
}

pub fn is_overdue(date: NaiveDate, done: bool, today: NaiveDate) -> bool {
    !done && date < today
}

#[cfg(test)]
#[path = "tests/dates.rs"]
mod tests;
