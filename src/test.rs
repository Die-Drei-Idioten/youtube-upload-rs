use super::*;

#[test]
fn test_parse_duration() {
    assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
    assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
    assert_eq!(parse_duration("1d").unwrap(), Duration::days(1));
    assert_eq!(parse_duration("3").unwrap(), Duration::hours(3));
}

#[test]
fn test_generate_schedule() {
    let interval = Duration::hours(2);
    let start_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let schedule = generate_schedule(3, interval, Some(start_time), None).unwrap();

    assert_eq!(schedule.len(), 3);
    assert_eq!(schedule[0], start_time);
    assert_eq!(schedule[1], start_time + Duration::hours(2));
    assert_eq!(schedule[2], start_time + Duration::hours(4));
}
