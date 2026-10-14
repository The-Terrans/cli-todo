use crate::{types::DatePhrase, Result};
use chrono::{Datelike, Days, Months, NaiveDate, Weekday};
use std::ops::Range;

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

pub fn date_phrases(text: &str, today: NaiveDate) -> Vec<DatePhrase> {
    let mut offset = 0;
    let mut words = Vec::new();
    for chunk in text.split_inclusive(char::is_whitespace) {
        let start = offset;
        offset += chunk.len();
        if chunk.contains("://") || chunk.contains('@') {
            continue;
        }
        let word = chunk
            .trim()
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if !word.is_empty() {
            let start = start + chunk.find(word).unwrap_or(0);
            words.push((start..start + word.len(), word));
        }
    }
    let mut phrases = Vec::new();
    for index in 0..words.len() {
        let Some((date, count)) = date_words(&words[index..], today) else {
            continue;
        };
        if !(1..=9999).contains(&date.year()) {
            continue;
        }
        let matched = &words[index..index + count];
        if matched.windows(2).any(|pair| {
            !text[pair[0].0.end..pair[1].0.start]
                .chars()
                .all(char::is_whitespace)
        }) {
            continue;
        }
        let range = matched[0].0.start..matched[count - 1].0.end;
        phrases.push(DatePhrase { range, date });
    }
    phrases
}

fn date_words(words: &[(Range<usize>, &str)], today: NaiveDate) -> Option<(NaiveDate, usize)> {
    let first = words.first()?.1.to_ascii_lowercase();
    match first.as_str() {
        "today" | "tod" => return Some((today, 1)),
        "tomorrow" | "tom" => return Some((today.checked_add_days(Days::new(1))?, 1)),
        "next" => {
            let second = words.get(1)?.1.to_ascii_lowercase();
            if second == "month" {
                return Some((today.checked_add_months(Months::new(1))?, 2));
            }
            let days = if second == "week" {
                7
            } else {
                let weekday = match second.as_str() {
                    "mon" | "monday" => Weekday::Mon,
                    "tue" | "tues" | "tuesday" => Weekday::Tue,
                    "wed" | "wednesday" => Weekday::Wed,
                    "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
                    "fri" | "friday" => Weekday::Fri,
                    "sat" | "saturday" => Weekday::Sat,
                    "sun" | "sunday" => Weekday::Sun,
                    _ => return None,
                };
                let days = (weekday.num_days_from_monday() + 7
                    - today.weekday().num_days_from_monday())
                    % 7;
                u64::from(if days == 0 { 7 } else { days })
            };
            return Some((today.checked_add_days(Days::new(days))?, 2));
        }
        "in" => {
            let count = words.get(1)?.1.parse::<u64>().ok()?;
            let unit = words.get(2)?.1.to_ascii_lowercase();
            let days = match unit.as_str() {
                "day" | "days" => count,
                "week" | "weeks" => count.checked_mul(7)?,
                _ => return None,
            };
            return Some((today.checked_add_days(Days::new(days))?, 3));
        }
        _ => {}
    }
    if let Ok(Some(date)) = parse_due_date(&first) {
        return Some((date, 1));
    }
    if first.len() == 10
        && first.bytes().enumerate().all(|(index, byte)| {
            if index == 2 || index == 5 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Some((NaiveDate::parse_from_str(&first, "%d-%m-%Y").ok()?, 1));
    }
    if first.len() > 2 || !first.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let day = first.parse::<u32>().ok()?;
    let month = match words.get(1)?.1.to_ascii_lowercase().as_str() {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    };
    let year = words.get(2)?.1;
    if year.len() != 4 || !year.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((NaiveDate::from_ymd_opt(year.parse().ok()?, month, day)?, 3))
}

pub fn remove_date_phrase(text: &str, mut range: Range<usize>) -> String {
    for (left, right) in [('(', ')'), ('[', ']')] {
        if text[..range.start].ends_with(left) && text[range.end..].starts_with(right) {
            range.start -= 1;
            range.end += 1;
            break;
        }
    }
    let before = text[..range.start].trim_end_matches([' ', '\t']);
    let after = text[range.end..].trim_start_matches([' ', '\t']);
    let join = !before.is_empty()
        && !after.is_empty()
        && !before.ends_with(char::is_whitespace)
        && !after.starts_with(char::is_whitespace)
        && !after.starts_with([',', '.', ';', ':', '!', '?', ')', ']']);
    format!("{before}{}{after}", if join { " " } else { "" })
}

pub fn is_overdue(date: NaiveDate, done: bool, today: NaiveDate) -> bool {
    !done && date < today
}

#[cfg(test)]
#[path = "tests/dates.rs"]
mod tests;
