use clap::{Arg, Command};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use reqwest::Client;
use serde_json::json;
use youtube::{create_default_metadata, load_oauth_config, load_video_metadata, YouTubeUploader};
use youtube_scheduler::*;
use std::path::Path;
use tokio;
use chrono::{DateTime, Duration, Utc};
use std::fs::{self, File};
use serde::{Deserialize, Serialize};
use std::io::{self};


mod youtube;
#[cfg(test)]
mod test;



#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("youtube-scheduler")
        .version("1.0")
        .author("LinlyBoi")
        .about("Upload and schedule YouTube videos")
        .arg(
            Arg::new("videos")
                .short('v')
                .long("videos")
                .value_name("VIDEO_FILES")
                .help("Comma-separated list of video file paths")
                .required(true),
        )
        .arg(
            Arg::new("interval")
                .short('i')
                .long("interval")
                .value_name("DURATION")
                .help("Time interval between uploads (e.g., 2h, 30m, 1d)")
                .required(true),
        )
        .arg(
            Arg::new("oauth-config")
                .short('c')
                .long("oauth-config")
                .value_name("CONFIG_FILE")
                .help("OAuth configuration file (JSON)")
                .default_value("~/.client_secrets.json"),
        )
        .arg(
            Arg::new("metadata")
                .short('m')
                .long("metadata")
                .value_name("METADATA_FILE")
                .help("JSON file containing video metadata"),
        )
        .arg(
            Arg::new("start-time")
                .short('s')
                .long("start-time")
                .value_name("START_TIME")
                .help("Start time for first upload (ISO 8601 format)"),
        )
        .arg(
            Arg::new("timestamp-file")
                .long("timestamp-file")
                .value_name("FILE")
                .help("File containing unix timestamp for start time"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Show schedule without uploading")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let video_files: Vec<String> = matches
        .get_one::<String>("videos")
        .unwrap()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let interval_str = matches.get_one::<String>("interval").unwrap();
    let interval = parse_duration(interval_str)?;

    let oauth_config_path = matches.get_one::<String>("oauth-config").unwrap();
    let oauth_config = load_oauth_config(oauth_config_path)?;

    let start_time = if let Some(start_str) = matches.get_one::<String>("start-time") {
        Some(DateTime::parse_from_rfc3339(start_str)?.with_timezone(&Utc))
    } else {
        None
    };

    let timestamp_file = matches.get_one::<String>("timestamp-file");

    let dry_run = matches.get_flag("dry-run");

    // Load or create metadata
    let mut metadata = if let Some(metadata_path) = matches.get_one::<String>("metadata") {
        load_video_metadata(metadata_path)?
    } else {
        create_default_metadata(&video_files)
    };

    // Generate schedule
    let schedule = generate_schedule(
        video_files.len(),
        interval,
        start_time,
        timestamp_file.map(|s| s.as_str()),
    )?;

    // Apply schedule to metadata
    for (i, scheduled_time) in schedule.iter().enumerate() {
        if i < metadata.len() {
            metadata[i].scheduled_start_time = Some(scheduled_time.to_rfc3339());
            metadata[i].privacy_status = "private".to_string(); // Set to private for scheduling
        }
    }

    // Display schedule
    println!("Upload Schedule:");
    println!("================");
    for (i, (video_file, scheduled_time)) in video_files.iter().zip(schedule.iter()).enumerate() {
        println!(
            "{}. {} -> {}",
            i + 1,
            video_file,
            scheduled_time.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }

    if dry_run {
        println!("\nDry run complete. No videos were uploaded.");
        return Ok(());
    }

    // Confirm before proceeding
    println!("\nProceed with upload? (y/N): ");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if !input.trim().to_lowercase().starts_with('y') {
        println!("Upload cancelled.");
        return Ok(());
    }

    // Create uploader and authenticate
    let mut uploader = YouTubeUploader::new(&oauth_config)?;

    if !dry_run {
        println!("Authenticating with YouTube...");
        uploader.authenticate().await?;
    }
    // Upload videos
    println!("\nUploading videos...");
    for (i, (video_file, video_metadata)) in video_files.iter().zip(metadata.iter()).enumerate() {
        println!("Uploading {} ({}/{})", video_file, i + 1, video_files.len());

        match uploader.upload_video(video_file, video_metadata).await {
            Ok(response) => {
                println!(
                    "✓ Successfully uploaded: {} (ID: {})",
                    video_file, response.id
                );
            }
            Err(e) => {
                eprintln!("✗ Failed to upload {}: {}", video_file, e);
            }
        }
    }

    println!("\nUpload process completed!");
    Ok(())
}
//For the random descriptions LOL
