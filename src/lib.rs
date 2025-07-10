use chrono::{DateTime, Duration, Utc};
use clap::{command, Parser};
use std::fs::{self};

#[derive(Parser, Debug)]
#[command(author= "LinlyBoi",
          version = "0.9",
          about = "Upload and schedule YouTube videos",
          long_about = None)]
pub struct Args {
    #[arg(
        short = 'v',
        long,
        value_name = "VIDEO_FILES",
        help = "Comma-separated list of video file paths",
        required = true
    )]
    videos: String,

    #[arg(
        short = 'i',
        long,
        value_name = "DURATION",
        help = "Time interval between uploads (e.g., 2h, 30m, 1d)",
        required = true
    )]
    interval: String,

    #[arg(
        short = 'c',
        long = "oauth-config",
        value_name = "CONFIG_FILE",
        help = "OAuth configuration file (JSON)",
        default_value = "~/.client_secrets.json",
        required = true
    )]
    oauth_config: String,

    #[arg(
        short = 'm',
        long = "metadata",
        value_name = "METADATA_FILE",
        help = "JSON file containing video metadata"
    )]
    metadata: Option<String>,

    #[arg(
        short = 's',
        long = "start-time",
        value_name = "START_TIME",
        help = "Start time for first upload (ISO 8601 format)"
    )]
    start_time: Option<String>,

    #[arg(
        long = "timestamp-file",
        value_name = "FILE",
        help = "File containing unix timestamp for start time"
    )]
    timestamp_file: Option<String>,

    #[arg(
    long = "dry-run",
    help = "Show schedule without uploading",
    action = clap::ArgAction::SetTrue
)]
    dry_run: bool,
}

impl Args {
    pub fn timestamp_file(&self) -> Option<&String> {
        self.timestamp_file.as_ref()
    }

    pub fn videos(&self) -> &str {
        &self.videos
    }

    pub fn interval(&self) -> &str {
        &self.interval
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn start_time(&self) -> Option<&String> {
        self.start_time.as_ref()
    }

    pub fn oauth_config(&self) -> &str {
        &self.oauth_config
    }
}

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

pub fn print_schedule(video_files: &Vec<String>, schedule: &Vec<DateTime<Utc>>) {
    for (i, (video_file, scheduled_time)) in video_files.iter().zip(schedule.iter()).enumerate() {
        println!(
            "{}. {} -> {}",
            i + 1,
            video_file,
            scheduled_time.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
}
