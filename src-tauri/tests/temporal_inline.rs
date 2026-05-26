//! Moved from inline tests in src/chat_manager/temporal.rs.

use chrono::{Local, TimeZone};
use lettuceai_lib::chat_manager::temporal::detect_temporal_query_range;

fn local_ms(year: i32, month: u32, day: u32, hour: u32) -> u64 {
    Local
        .with_ymd_and_hms(year, month, day, hour, 0, 0)
        .earliest()
        .expect("valid local datetime")
        .timestamp_millis() as u64
}

#[test]
fn parses_last_week() {
    let reference = local_ms(2026, 5, 10, 12);
    let range =
        detect_temporal_query_range("what place did we go to last week", reference).expect("range");
    assert!(range.start_ms < range.end_ms);
}

#[test]
fn parses_days_ago() {
    let reference = local_ms(2026, 5, 10, 12);
    let range =
        detect_temporal_query_range("where did we eat 2 days ago", reference).expect("range");
    assert!(range.start_ms < range.end_ms);
}

#[test]
fn parses_last_saturday() {
    let reference = local_ms(2026, 5, 10, 12);
    let range = detect_temporal_query_range("what did we do after coffee last saturday", reference)
        .expect("range");
    assert!(range.start_ms < range.end_ms);
}

#[test]
fn parses_five_weeks_ago_today() {
    let reference = local_ms(2026, 5, 10, 12);
    let range =
        detect_temporal_query_range("what did we do 5 week ago today", reference).expect("range");
    assert!(range.end_ms - range.start_ms <= 86_400_000);
}

#[test]
fn parses_word_number_weekday_ago() {
    let reference = local_ms(2026, 5, 10, 12);
    let range =
        detect_temporal_query_range("what did we do two fridays ago", reference).expect("range");
    assert!(range.start_ms < range.end_ms);
}
