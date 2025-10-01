use chrono::{DateTime, Duration, Utc};
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
use youtube_scheduler::expand_tilde;

#[derive(Debug, Serialize, Deserialize)]
pub struct OAuthConfig {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoMetadata {
    title: String,
    description: String,
    tags: Vec<String>,
    category_id: String,
    pub privacy_status: String,
    pub scheduled_start_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub id: String,
    snippet: serde_json::Value,
    status: serde_json::Value,
}

pub struct YouTubeUploader {
    client: Client,
    access_token: String,
    oauth_client: BasicClient,
    client_id: String,
    client_secret: String,
}

impl YouTubeUploader {
    pub fn new(oauth_config: &OAuthConfig) -> Result<Self, Box<dyn std::error::Error>> {
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

    pub async fn authenticate(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

    pub async fn upload_video(
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

pub fn create_default_metadata(
    video_files: &[String],
    description_file: &str,
) -> Vec<VideoMetadata> {
    let expanded_path = expand_tilde(description_file);
    video_files
        .iter()
        .map(|file_path| {
            let filename = Path::new(file_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            VideoMetadata {
                title: filename.to_string(),
                description: get_random_line(&expanded_path).unwrap_or_default(),
                tags: vec!["gaming".to_string()],
                category_id: "20".to_string(), // GAMING
                privacy_status: "private".to_string(),
                scheduled_start_time: None,
            }
        })
        .collect()
}
fn get_random_line(path: &str) -> io::Result<String> {
    let expanded_path = expand_tilde(path);
    let file = File::open(expanded_path)?;
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
pub fn load_video_metadata(
    metadata_path: &str,
) -> Result<Vec<VideoMetadata>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(metadata_path)?;
    let metadata: Vec<VideoMetadata> = serde_json::from_str(&content)?;
    Ok(metadata)
}

pub fn load_oauth_config(config_path: &str) -> Result<OAuthConfig, Box<dyn std::error::Error>> {
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
