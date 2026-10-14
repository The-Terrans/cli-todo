use super::*;

fn extract_date_phrase(text: &str, today: NaiveDate) -> Option<(String, NaiveDate)> {
    let phrase = date_phrases(text, today).into_iter().next()?;
    Some((remove_date_phrase(text, phrase.range), phrase.date))
}

#[test]
fn date_phrases_are_extracted_and_removed_with_calendar_safe_relative_dates() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
    for (text, cleaned, date) in [
        ("do chores today", "do chores", "2026-08-15"),
        ("do chores tod", "do chores", "2026-08-15"),
        ("do chores tomorrow", "do chores", "2026-08-16"),
        ("do chores tom", "do chores", "2026-08-16"),
        ("Do TOM chores", "Do chores", "2026-08-16"),
        ("today do chores", "do chores", "2026-08-15"),
        ("do chores next week", "do chores", "2026-08-22"),
        ("do chores next month", "do chores", "2026-09-15"),
        ("do chores 12-12-2000", "do chores", "2000-12-12"),
        ("do chores 23-04-2026", "do chores", "2026-04-23"),
        ("do chores 29-02-2028.", "do chores.", "2028-02-29"),
        ("do chores next Monday", "do chores", "2026-08-17"),
        ("do chores next sat", "do chores", "2026-08-22"),
        ("do chores in 3 days", "do chores", "2026-08-18"),
        ("do chores in 2 weeks", "do chores", "2026-08-29"),
        ("do chores 12 oct 2026", "do chores", "2026-10-12"),
        ("do chores 12 October 2026.", "do chores.", "2026-10-12"),
        ("do chores 2028-02-29", "do chores", "2028-02-29"),
        ("做家务 (tomorrow)!", "做家务!", "2026-08-16"),
        (
            "first line\nfinish tomorrow\nlast line",
            "first line\nfinish\nlast line",
            "2026-08-16",
        ),
    ] {
        assert_eq!(
            extract_date_phrase(text, today),
            Some((cleaned.into(), parse_due_date(date).unwrap().unwrap())),
            "{text}"
        );
    }
    let year_end = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
    assert_eq!(
        extract_date_phrase("finish tom", year_end).unwrap().1,
        NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
    );
}

#[test]
fn next_month_uses_calendar_months_and_clamps_month_end() {
    for (today, expected) in [
        ("2026-01-31", "2026-02-28"),
        ("2028-01-31", "2028-02-29"),
        ("2026-12-31", "2027-01-31"),
    ] {
        let today = parse_due_date(today).unwrap().unwrap();
        assert_eq!(
            extract_date_phrase("finish next month", today),
            Some(("finish".into(), parse_due_date(expected).unwrap().unwrap()))
        );
    }
    assert!(date_phrases("next month", parse_due_date("9999-12-31").unwrap().unwrap()).is_empty());
}

#[test]
fn date_matching_ignores_invalid_dates_partial_words_urls_and_overflow() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
    for text in [
        "tomatoes",
        "todayish",
        "tomorrow's plan",
        "31 feb 2026",
        "2026-02-30",
        "31-04-2026",
        "29-02-2026",
        "12-12-0000",
        "1-12-2000",
        "12 oct 0000",
        "12 oct 26",
        "next, week",
        "in -1 days",
        "in 18446744073709551615 weeks",
        "https://example.com/today",
        "tom@example.com",
        "",
        "   ",
    ] {
        assert_eq!(extract_date_phrase(text, today), None, "{text}");
    }
    assert!(extract_date_phrase(
        "finish tomorrow",
        NaiveDate::from_ymd_opt(9999, 12, 31).unwrap()
    )
    .is_none());
    assert_eq!(
        extract_date_phrase("today then tomorrow", today).unwrap(),
        ("then tomorrow".into(), today)
    );
}

#[test]
fn dates_are_optional_strict_and_calendar_valid() -> Result<()> {
    assert_eq!(parse_due_date(" \t ")?, None);
    assert_eq!(
        parse_due_date(" 2028-02-29 ")?,
        NaiveDate::from_ymd_opt(2028, 2, 29)
    );
    for input in [
        "2026-02-29",
        "2026-02-30",
        "2026-13-01",
        "2026-04-31",
        "2026-1-01",
        "0000-01-01",
        "tomorrow",
        "2026-01-01 12:00",
        "２０２６-01-01",
    ] {
        assert!(parse_due_date(input).is_err(), "{input}");
    }
    Ok(())
}

#[test]
fn only_unfinished_tasks_before_today_are_overdue() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
    assert!(is_overdue(today.pred_opt().unwrap(), false, today));
    assert!(!is_overdue(today, false, today));
    assert!(!is_overdue(today.succ_opt().unwrap(), false, today));
    assert!(!is_overdue(today.pred_opt().unwrap(), true, today));
}
