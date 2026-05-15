mod config;
mod token;

#[cfg(test)]
mod config_tests;

use std::fs;
use std::path::PathBuf;
use std::process;

use aws_sdk_sso as sso;
use clap::{Parser, ValueEnum};

use config::{
    get_start_url_from_config, merge_sso_profiles, parse_config_file, render_config, AccountResult,
};
use token::get_sso_access_token;

#[derive(Parser)]
#[command(about = "List all AWS SSO accounts and their available roles")]
struct Cli {
    /// SSO start URL to match cached token (auto-detected from --sso-session if omitted)
    #[arg(long)]
    start_url: Option<String>,

    /// SSO session name (used to look up start URL and required for config output)
    #[arg(long)]
    sso_session: Option<String>,

    /// AWS region for SSO API calls
    #[arg(long, default_value = "eu-central-1")]
    region: String,

    /// Output format: json or config (AWS CLI named profiles)
    #[arg(long, value_enum, default_value = "config")]
    output: OutputFormat,

    /// Path to AWS config file (used with config output)
    #[arg(long, default_value_t = default_config_path())]
    config_file: String,

    /// Write config output directly to the config file instead of stdout
    #[arg(long, default_value_t = false)]
    write_config: bool,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Json,
    Config,
}

fn default_config_path() -> String {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".aws")
        .join("config")
        .to_string_lossy()
        .to_string()
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Resolve start URL: explicit flag > lookup from sso-session in config
    let start_url = match &cli.start_url {
        Some(url) => url.clone(),
        None => match &cli.sso_session {
            Some(session) => match get_start_url_from_config(session, &cli.config_file) {
                Some(url) => {
                    eprintln!("Resolved start URL from sso-session '{session}': {url}");
                    url
                }
                None => {
                    eprintln!(
                        "Error: Could not find sso_start_url for sso-session '{session}' in {}",
                        cli.config_file
                    );
                    eprintln!(
                        "Provide --start-url explicitly or ensure the sso-session is configured."
                    );
                    process::exit(1);
                }
            },
            None => {
                eprintln!("Error: Either --start-url or --sso-session is required");
                process::exit(1);
            }
        },
    };

    let access_token = match get_sso_access_token(&start_url) {
        Some(t) => t,
        None => {
            eprintln!("Error: No valid SSO token found for {}", start_url);
            eprintln!("Please log in: aws sso login --sso-session <your-sso-session>");
            process::exit(1);
        }
    };

    if matches!(cli.output, OutputFormat::Config) && cli.sso_session.is_none() {
        eprintln!("Error: --sso-session is required for config output");
        process::exit(1);
    }

    let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(cli.region.clone()))
        .no_credentials()
        .load()
        .await;

    let client = sso::Client::new(&aws_config);

    // List all accounts
    let mut accounts = Vec::new();
    let mut paginator = client
        .list_accounts()
        .access_token(&access_token)
        .into_paginator()
        .send();

    while let Some(page) = paginator.next().await {
        match page {
            Ok(output) => {
                accounts.extend(output.account_list().to_vec());
            }
            Err(e) => {
                eprintln!("Error listing accounts: {e}");
                process::exit(1);
            }
        }
    }

    if accounts.is_empty() {
        println!("No accounts found.");
        return;
    }

    // List roles for each account
    let mut results: Vec<AccountResult> = Vec::new();
    for account in &accounts {
        let aid = account.account_id().unwrap_or_default();
        let aname = account.account_name().unwrap_or_default();

        let mut roles = Vec::new();
        let mut role_paginator = client
            .list_account_roles()
            .account_id(aid)
            .access_token(&access_token)
            .into_paginator()
            .send();

        let mut had_error = false;
        while let Some(page) = role_paginator.next().await {
            match page {
                Ok(output) => {
                    for role in output.role_list() {
                        if let Some(name) = role.role_name() {
                            roles.push(name.to_string());
                        }
                    }
                }
                Err(e) => {
                    results.push(AccountResult {
                        account_id: aid.to_string(),
                        account_name: aname.to_string(),
                        roles: None,
                        error: Some(e.to_string()),
                    });
                    had_error = true;
                    break;
                }
            }
        }

        if !had_error {
            results.push(AccountResult {
                account_id: aid.to_string(),
                account_name: aname.to_string(),
                roles: Some(roles),
                error: None,
            });
        }
    }

    // Output
    match cli.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
        OutputFormat::Config => {
            let sso_session = cli.sso_session.unwrap();
            let config_path = PathBuf::from(&cli.config_file);
            let (preamble, mut sections) = parse_config_file(&config_path);

            let (updated, added) =
                merge_sso_profiles(&mut sections, &results, &sso_session, &cli.region);

            if !updated.is_empty() {
                eprintln!(
                    "Updated {} existing profile(s): {}",
                    updated.len(),
                    updated.join(", ")
                );
            }
            if !added.is_empty() {
                eprintln!("Added {} new profile(s): {}", added.len(), added.join(", "));
            }

            let output = render_config(&preamble, &sections);

            if cli.write_config {
                if let Some(parent) = config_path.parent() {
                    fs::create_dir_all(parent).unwrap_or_else(|e| {
                        eprintln!("Error creating directory {}: {e}", parent.display());
                        process::exit(1);
                    });
                }
                fs::write(&config_path, format!("{output}\n")).unwrap_or_else(|e| {
                    eprintln!("Error writing {}: {e}", config_path.display());
                    process::exit(1);
                });
                eprintln!("Written to {}", config_path.display());
            } else {
                println!("{output}");
            }
        }
    }
}
