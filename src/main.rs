use chrono::{DateTime, Utc};
use clap::Parser;
use tokio;
use youtube::{create_default_metadata, load_oauth_config, load_video_metadata, YouTubeUploader};
use youtube_scheduler::*;

#[cfg(test)]
mod test;
mod youtube;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let video_files: Vec<String> = args.videos()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let interval_str = args.interval();
    let interval = parse_duration(interval_str)?;

    let oauth_config_path = args.oauth_config();
    let oauth_config = load_oauth_config(oauth_config_path)?;

    let start_time = if let Some(start_str) = args.start_time() {
        Some(DateTime::parse_from_rfc3339(start_str)?.with_timezone(&Utc))
    } else {
        None
    };

    let timestamp_file = args.timestamp_file();

    let dry_run = args.dry_run();

    // Load or create metadata
    let mut metadata = if let Some(metadata_path) = args.timestamp_file() {
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

    //Display schedule
    println!("Upload Schedule:");
    println!("================");
    print_schedule(&video_files, &schedule);

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
