use chrono::{DateTime, Duration, Utc};
use std::{fs::{self, File}};
use std::path::Path;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader};
use rand::seq::SliceRandom;





pub fn parse_duration(duration_str: &str) -> Result<Duration, Box<dyn std::error::Error>> {
    let duration_str = duration_str.to_lowercase();

    if duration_str.ends_with("h") {
        let hours: i64 = duration_str.trim_end_matches("h").parse()?;
        Ok(Duration::hours(hours))
    } else if duration_str.ends_with("m") {
        let minutes: i64 = duration_str.trim_end_matches("m").parse()?;
        Ok(Duration::minutes(minutes))
    } else if duration_str.ends_with("d") {
        let days: i64 = duration_str.trim_end_matches("d").parse()?;
        Ok(Duration::days(days))
    } else {
        // Default to hours if no unit specified
        let hours: i64 = duration_str.parse()?;
        Ok(Duration::hours(hours))
    }
}

pub fn generate_schedule(
    video_count: usize,
    interval: Duration,
    start_time: Option<DateTime<Utc>>,
    timestamp_file: Option<&str>,
) -> Result<Vec<DateTime<Utc>>, Box<dyn std::error::Error>> {
    let mut schedule = Vec::new();

    let start = if let Some(file_path) = timestamp_file {
        // Read timestamp from file
        let expanded_path = expand_tilde(file_path);
        let timestamp_str = fs::read_to_string(&expanded_path)
            .map_err(|e| format!("Failed to read timestamp from '{}': {}", expanded_path, e))?;

        let timestamp: i64 = timestamp_str
            .trim()
            .parse()
            .map_err(|e| format!("Invalid timestamp in file '{}': {}", expanded_path, e))?;

        DateTime::from_timestamp(timestamp, 0)
            .ok_or_else(|| format!("Invalid unix timestamp: {}", timestamp))?
    } else if let Some(start_time) = start_time {
        start_time
    } else {
        Utc::now() + Duration::hours(1)
    };

    for i in 0..video_count {
        let scheduled_time = start + interval * i as i32;
        schedule.push(scheduled_time);
    }

    Ok(schedule)
}

pub fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            path.replacen("~", &home, 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    }
}
