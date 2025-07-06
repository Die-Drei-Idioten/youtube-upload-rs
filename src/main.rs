use chrono::{DateTime, Duration, Utc};
use clap::{Arg, Command};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use rand::seq::SliceRandom;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use tokio;

#[cfg(test)]
mod test;

#[derive(Debug, Serialize, Deserialize)]
struct OAuthConfig {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VideoMetadata {
    title: String,
    description: String,
    tags: Vec<String>,
    category_id: String,
    privacy_status: String,
    scheduled_start_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UploadResponse {
    id: String,
    snippet: serde_json::Value,
    status: serde_json::Value,
}

struct YouTubeUploader {
    client: Client,
    access_token: String,
    oauth_client: BasicClient,
    client_id: String,
    client_secret: String,
}

impl YouTubeUploader {
    fn new(oauth_config: &OAuthConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let oauth_client = BasicClient::new(
            ClientId::new(oauth_config.client_id.clone()),
            Some(ClientSecret::new(oauth_config.client_secret.clone())),
            AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?,
            Some(TokenUrl::new(
                "https://oauth2.googleapis.com/token".to_string(),
            )?),
        )
        .set_redirect_uri(RedirectUrl::new(oauth_config.redirect_uri.clone())?);

        Ok(Self {
            client: Client::new(),
            access_token: String::new(),
            oauth_client,
            client_id: oauth_config.client_id.clone(),
            client_secret: oauth_config.client_secret.clone(),
        })
    }

    async fn authenticate(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Try to load existing tokens
        if let Ok(tokens) = self.load_tokens() {
            if let Some(expires_at) = tokens.expires_at {
                if expires_at > Utc::now() + Duration::minutes(5) {
                    // Token is still valid
                    self.access_token = tokens.access_token;
                    println!("Using existing valid token");
                    return Ok(());
                }
            }

            // Try to refresh the token
            if let Some(refresh_token) = tokens.refresh_token {
                if let Ok(new_tokens) = self.refresh_token(&refresh_token).await {
                    self.access_token = new_tokens.access_token.clone();
                    self.store_tokens(&new_tokens)?;
                    println!("Refreshed access token");
                    return Ok(());
                }
            }
        }

        // Perform full OAuth flow
        self.perform_oauth_flow().await?;
        Ok(())
    }

    async fn perform_oauth_flow(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Generate PKCE challenge
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Generate authorization URL
        let (auth_url, _csrf_token) = self
            .oauth_client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(
                "https://www.googleapis.com/auth/youtube.upload".to_string(),
            ))
            .set_pkce_challenge(pkce_challenge)
            .url();

        println!("Open this URL in your browser to authenticate:");
        println!("{}", auth_url);
        println!("\nAfter authorization, you'll be redirected to your redirect URI.");
        println!("Copy the 'code' parameter from the redirect URL and paste it here:");

        // Get authorization code from user
        let mut auth_code = String::new();
        std::io::stdin().read_line(&mut auth_code)?;
        let auth_code = auth_code.trim();

        // Exchange authorization code for access token
        let token_result = self
            .oauth_client
            .exchange_code(AuthorizationCode::new(auth_code.to_string()))
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await?;

        // Store tokens
        let expires_at = token_result
            .expires_in()
            .map(|duration| Utc::now() + Duration::seconds(duration.as_secs() as i64));

        let tokens = StoredTokens {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
            expires_at,
        };

        self.access_token = tokens.access_token.clone();
        self.store_tokens(&tokens)?;

        println!("Authentication successful!");
        Ok(())
    }

    async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<StoredTokens, Box<dyn std::error::Error>> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ];

        let response = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await?;

        let token_data: serde_json::Value = response.json().await?;

        let access_token = token_data["access_token"]
            .as_str()
            .ok_or("No access token in response")?
            .to_string();

        let expires_in = token_data["expires_in"].as_u64().unwrap_or(3600);
        let expires_at = Some(Utc::now() + Duration::seconds(expires_in as i64));

        Ok(StoredTokens {
            access_token,
            refresh_token: Some(refresh_token.to_string()),
            expires_at,
        })
    }

    fn store_tokens(&self, tokens: &StoredTokens) -> Result<(), Box<dyn std::error::Error>> {
        let tokens_path = expand_tilde("~/.youtube_tokens.json");
        let tokens_json = serde_json::to_string_pretty(tokens)?;
        fs::write(&tokens_path, tokens_json)?;
        Ok(())
    }

    fn load_tokens(&self) -> Result<StoredTokens, Box<dyn std::error::Error>> {
        let tokens_path = expand_tilde("~/.youtube_tokens.json");
        let tokens_json = fs::read_to_string(&tokens_path)?;
        let tokens: StoredTokens = serde_json::from_str(&tokens_json)?;
        Ok(tokens)
    }

    async fn upload_video(
        &self,
        video_path: &str,
        metadata: &VideoMetadata,
    ) -> Result<UploadResponse, Box<dyn std::error::Error>> {
        // Read video file
        let video_data = fs::read(video_path)?;

        // Combine snippet and status into a single JSON object
        let metadata_json = json!({
            "snippet": {
                "title": metadata.title,
                "description": metadata.description,
                "tags": metadata.tags,
                "categoryId": metadata.category_id
            },
            "status": {
                "privacyStatus": metadata.privacy_status,
                "publishAt": metadata.scheduled_start_time
            }
        });

        // Create multipart form with only 2 parts: metadata and media
        let form = reqwest::multipart::Form::new()
            .part(
                "snippet",
                reqwest::multipart::Part::text(metadata_json.to_string())
                    .mime_str("application/json")?,
            )
            .part(
                "media",
                reqwest::multipart::Part::bytes(video_data)
                    .file_name("video.mp4")
                    .mime_str("video/mp4")?,
            );

        let response = self
            .client
            .post("https://www.googleapis.com/upload/youtube/v3/videos")
            .query(&[("part", "snippet,status")])
            .header("Authorization", format!("Bearer {}", self.access_token))
            .multipart(form)
            .send()
            .await?;

        if response.status().is_success() {
            let upload_response: UploadResponse = response.json().await?;
            Ok(upload_response)
        } else {
            let error_text = response.text().await?;
            Err(format!("Upload failed: {}", error_text).into())
        }
    }
}

fn parse_duration(duration_str: &str) -> Result<Duration, Box<dyn std::error::Error>> {
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

fn generate_schedule(
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

fn load_video_metadata(
    metadata_path: &str,
) -> Result<Vec<VideoMetadata>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(metadata_path)?;
    let metadata: Vec<VideoMetadata> = serde_json::from_str(&content)?;
    Ok(metadata)
}

fn expand_tilde(path: &str) -> String {
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

fn load_oauth_config(config_path: &str) -> Result<OAuthConfig, Box<dyn std::error::Error>> {
    let expanded_path = expand_tilde(config_path);
    let content = fs::read_to_string(&expanded_path).map_err(|e| {
        format!(
            "Failed to read OAuth config from '{}': {}",
            expanded_path, e
        )
    })?;
    let config: OAuthConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse OAuth config: {}", e))?;
    Ok(config)
}

fn create_default_metadata(video_files: &[String]) -> Vec<VideoMetadata> {
    video_files
        .iter()
        .enumerate()
        .map(|(_i, file_path)| {
            let filename = Path::new(file_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            VideoMetadata {
                title: format!("{}", filename),
                description: get_random_line("/home/linly/org/quotes.org").expect("WHAT DA HAIL"),
                tags: vec!["gaming".to_string()],
                category_id: "20".to_string(), // GAMING
                privacy_status: "private".to_string(),
                scheduled_start_time: None,
            }
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("youtube-scheduler")
        .version("1.0")
        .author("Your Name")
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
fn get_random_line<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<io::Result<Vec<String>>>()?;

    if lines.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "No lines found in the file.",
        ));
    }

    let mut rng = rand::thread_rng();
    let random_line = lines.choose(&mut rng).unwrap().to_string();

    Ok(random_line)
}
