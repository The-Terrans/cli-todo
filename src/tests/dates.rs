use super::*;

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
