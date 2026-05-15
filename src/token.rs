use std::fs;

use chrono::Utc;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedToken {
    start_url: Option<String>,
    access_token: Option<String>,
    expires_at: Option<String>,
}

/// Read a valid cached SSO access token matching the given start URL.
pub fn get_sso_access_token(start_url: &str) -> Option<String> {
    let cache_dir = dirs::home_dir()?.join(".aws").join("sso").join("cache");
    if !cache_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(&cache_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let data: CachedToken = match serde_json::from_str(&content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if data.start_url.as_deref() != Some(start_url) {
            continue;
        }
        let access_token = match &data.access_token {
            Some(t) => t.clone(),
            None => continue,
        };
        let expires_at = match &data.expires_at {
            Some(e) => e.clone(),
            None => continue,
        };

        // Parse ISO 8601 timestamp
        let expires = match chrono::DateTime::parse_from_rfc3339(&expires_at)
            .or_else(|_| chrono::DateTime::parse_from_rfc3339(&expires_at.replace("Z", "+00:00")))
        {
            Ok(dt) => dt,
            Err(_) => continue,
        };

        if expires > Utc::now() {
            return Some(access_token);
        }
    }
    None
}
