use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::Serialize;

/// Result of listing accounts and roles from AWS SSO.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResult {
    pub account_id: String,
    pub account_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A parsed section from an INI-style AWS config file.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigSection {
    /// The full header line, e.g. "[profile my-profile]" or "[default]"
    pub header: String,
    /// Key-value pairs in insertion order
    pub entries: Vec<(String, String)>,
}

/// Parse an AWS config file into ordered sections, preserving comments and blanks.
pub fn parse_config_file(path: &PathBuf) -> (Vec<String>, Vec<ConfigSection>) {
    if !path.exists() {
        return (Vec::new(), Vec::new());
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    parse_config_content(&content)
}

/// Parse INI-style config content into ordered sections, preserving comments and blanks.
pub fn parse_config_content(content: &str) -> (Vec<String>, Vec<ConfigSection>) {
    let mut preamble: Vec<String> = Vec::new();
    let mut sections: Vec<ConfigSection> = Vec::new();

    let section_re = Regex::new(r"^\[(.+)\]").unwrap();
    let mut current_section: Option<ConfigSection> = None;

    for line in content.lines() {
        if section_re.is_match(line) {
            if let Some(sec) = current_section.take() {
                sections.push(sec);
            }
            current_section = Some(ConfigSection {
                header: line.to_string(),
                entries: Vec::new(),
            });
        } else if let Some(ref mut sec) = current_section {
            if let Some((key, value)) = line.split_once('=') {
                sec.entries
                    .push((key.trim().to_string(), value.trim().to_string()));
            } else if !line.trim().is_empty() {
                sec.entries
                    .push(("__comment__".to_string(), line.to_string()));
            }
        } else {
            preamble.push(line.to_string());
        }
    }
    if let Some(sec) = current_section {
        sections.push(sec);
    }

    (preamble, sections)
}

/// Render sections back to INI format.
pub fn render_config(preamble: &[String], sections: &[ConfigSection]) -> String {
    let mut output = String::new();

    let trimmed_preamble: Vec<&String> = preamble
        .iter()
        .rev()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    for line in &trimmed_preamble {
        output.push_str(line);
        output.push('\n');
    }
    if !trimmed_preamble.is_empty() {
        output.push('\n');
    }

    for (i, section) in sections.iter().enumerate() {
        if i > 0 || !trimmed_preamble.is_empty() {
            if i > 0 {
                output.push('\n');
            }
        }
        output.push_str(&section.header);
        output.push('\n');
        for (key, value) in &section.entries {
            if key == "__comment__" {
                output.push_str(value);
            } else {
                output.push_str(&format!("{key} = {value}"));
            }
            output.push('\n');
        }
    }

    output.trim_end_matches('\n').to_string()
}

/// Sanitize a string into a valid AWS CLI profile name.
pub fn sanitize_profile_name(name: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9_-]").unwrap();
    re.replace_all(name, "_").trim_matches('_').to_lowercase()
}

/// Look up the sso_start_url for a given sso-session name from the AWS config file.
pub fn get_start_url_from_config(session_name: &str, config_file: &str) -> Option<String> {
    let config_path = PathBuf::from(config_file);
    let (_, sections) = parse_config_file(&config_path);

    let target_header = format!("[sso-session {session_name}]");
    for section in &sections {
        if section.header == target_header {
            for (key, value) in &section.entries {
                if key == "sso_start_url" {
                    return Some(value.clone());
                }
            }
        }
    }
    None
}

/// Merge SSO profile entries into parsed config sections.
/// Updates existing profiles in place (preserving extra keys), adds new ones at the end.
/// Returns (updated_profiles, added_profiles).
pub fn merge_sso_profiles(
    sections: &mut Vec<ConfigSection>,
    results: &[AccountResult],
    sso_session: &str,
    region: &str,
) -> (Vec<String>, Vec<String>) {
    let mut section_index: HashMap<String, usize> = HashMap::new();
    let re = Regex::new(r"^\[(.+)\]$").unwrap();
    for (i, sec) in sections.iter().enumerate() {
        if let Some(cap) = re.captures(&sec.header) {
            section_index.insert(cap[1].to_string(), i);
        }
    }

    let mut updated: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();

    for entry in results {
        if entry.error.is_some() {
            continue;
        }
        if let Some(roles) = &entry.roles {
            for role in roles {
                let profile = sanitize_profile_name(&format!("{}-{}", entry.account_name, role));
                let section_name = format!("profile {profile}");

                let new_entries = vec![
                    ("sso_session".to_string(), sso_session.to_string()),
                    ("sso_account_id".to_string(), entry.account_id.clone()),
                    ("sso_role_name".to_string(), role.clone()),
                    ("region".to_string(), region.to_string()),
                ];

                if let Some(&idx) = section_index.get(&section_name) {
                    let existing_sec = &mut sections[idx];
                    let new_keys: HashMap<&str, &str> = new_entries
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();

                    let mut seen_keys: HashSet<String> = HashSet::new();
                    for (key, value) in existing_sec.entries.iter_mut() {
                        if key == "__comment__" {
                            continue;
                        }
                        if let Some(new_val) = new_keys.get(key.as_str()) {
                            *value = new_val.to_string();
                            seen_keys.insert(key.clone());
                        }
                    }
                    for (k, v) in &new_entries {
                        if !seen_keys.contains(k.as_str()) {
                            existing_sec.entries.push((k.clone(), v.clone()));
                        }
                    }
                    updated.push(profile);
                } else {
                    sections.push(ConfigSection {
                        header: format!("[{section_name}]"),
                        entries: new_entries,
                    });
                    added.push(profile);
                }
            }
        }
    }

    (updated, added)
}
