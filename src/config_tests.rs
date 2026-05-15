use crate::config::*;

// --- INI Parser Tests ---

#[test]
fn test_parse_empty_content() {
    let (preamble, sections) = parse_config_content("");
    assert!(preamble.is_empty());
    assert!(sections.is_empty());
}

#[test]
fn test_parse_single_section() {
    let content = "[default]\nregion = us-east-1\noutput = json";
    let (preamble, sections) = parse_config_content(content);
    assert!(preamble.is_empty());
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].header, "[default]");
    assert_eq!(sections[0].entries.len(), 2);
    assert_eq!(
        sections[0].entries[0],
        ("region".to_string(), "us-east-1".to_string())
    );
    assert_eq!(
        sections[0].entries[1],
        ("output".to_string(), "json".to_string())
    );
}

#[test]
fn test_parse_multiple_sections() {
    let content = "\
[default]
region = us-east-1

[profile dev]
sso_session = my-sso
sso_account_id = 111111111111
sso_role_name = AdminAccess
region = eu-west-1

[sso-session my-sso]
sso_start_url = https://my-org.awsapps.com/start
sso_region = eu-central-1
sso_registration_scopes = sso:account:access";

    let (preamble, sections) = parse_config_content(content);
    assert!(preamble.is_empty());
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].header, "[default]");
    assert_eq!(sections[1].header, "[profile dev]");
    assert_eq!(sections[2].header, "[sso-session my-sso]");
}

#[test]
fn test_parse_preserves_preamble_comments() {
    let content = "\
# This is a comment
# Another comment

[default]
region = us-east-1";

    let (preamble, sections) = parse_config_content(content);
    assert_eq!(preamble.len(), 3);
    assert_eq!(preamble[0], "# This is a comment");
    assert_eq!(preamble[1], "# Another comment");
    assert_eq!(sections.len(), 1);
}

#[test]
fn test_parse_preserves_inline_comments() {
    let content = "\
[profile dev]
# This is a comment inside a section
sso_session = my-sso
sso_account_id = 111111111111";

    let (_, sections) = parse_config_content(content);
    assert_eq!(sections[0].entries.len(), 3);
    assert_eq!(
        sections[0].entries[0],
        (
            "__comment__".to_string(),
            "# This is a comment inside a section".to_string()
        )
    );
}

#[test]
fn test_parse_handles_spaces_around_equals() {
    let content = "[profile test]\n  sso_session  =  my-sso  \nregion=us-east-1";
    let (_, sections) = parse_config_content(content);
    assert_eq!(
        sections[0].entries[0],
        ("sso_session".to_string(), "my-sso".to_string())
    );
    assert_eq!(
        sections[0].entries[1],
        ("region".to_string(), "us-east-1".to_string())
    );
}

// --- Render Tests ---

#[test]
fn test_render_roundtrip_simple() {
    let content = "\
[default]
region = us-east-1
output = json";

    let (preamble, sections) = parse_config_content(content);
    let rendered = render_config(&preamble, &sections);
    assert_eq!(rendered, content);
}

#[test]
fn test_render_roundtrip_multiple_sections() {
    let content = "\
[default]
region = us-east-1

[profile dev]
sso_session = my-sso
sso_account_id = 111111111111";

    let (preamble, sections) = parse_config_content(content);
    let rendered = render_config(&preamble, &sections);
    assert_eq!(rendered, content);
}

#[test]
fn test_render_with_preamble() {
    let content = "\
# My AWS config

[default]
region = us-east-1";

    let (preamble, sections) = parse_config_content(content);
    let rendered = render_config(&preamble, &sections);
    assert_eq!(rendered, content);
}

// --- Sanitize Profile Name Tests ---

#[test]
fn test_sanitize_simple() {
    assert_eq!(
        sanitize_profile_name("my-account-AdminAccess"),
        "my-account-adminaccess"
    );
}

#[test]
fn test_sanitize_spaces_and_special_chars() {
    assert_eq!(
        sanitize_profile_name("My Corp Production-AdminAccess"),
        "my_corp_production-adminaccess"
    );
}

#[test]
fn test_sanitize_strips_leading_trailing_underscores() {
    assert_eq!(sanitize_profile_name("__test__"), "test");
}

#[test]
fn test_sanitize_dots_and_slashes() {
    assert_eq!(
        sanitize_profile_name("org.team/account-Role"),
        "org_team_account-role"
    );
}

// --- Merge Tests ---

#[test]
fn test_merge_adds_new_profile() {
    let content = "\
[default]
region = us-east-1";

    let (_, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "111111111111".to_string(),
        account_name: "dev-account".to_string(),
        roles: Some(vec!["AdminAccess".to_string()]),
        error: None,
    }];

    let (updated, added) = merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");

    assert!(updated.is_empty());
    assert_eq!(added, vec!["dev-account-adminaccess"]);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[1].header, "[profile dev-account-adminaccess]");
}

#[test]
fn test_merge_preserves_existing_non_sso_profiles() {
    let content = "\
[default]
region = us-east-1
output = json

[profile legacy-iam]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
region = us-west-2";

    let (preamble, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "222222222222".to_string(),
        account_name: "prod".to_string(),
        roles: Some(vec!["ReadOnly".to_string()]),
        error: None,
    }];

    merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");
    let rendered = render_config(&preamble, &sections);

    assert!(rendered.contains("[default]"));
    assert!(rendered.contains("output = json"));
    assert!(rendered.contains("[profile legacy-iam]"));
    assert!(rendered.contains("aws_access_key_id = AKIAIOSFODNN7EXAMPLE"));
    assert!(rendered.contains("aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"));
    assert!(rendered.contains("region = us-west-2"));
    assert!(rendered.contains("[profile prod-readonly]"));
}

#[test]
fn test_merge_updates_existing_sso_profile_preserves_extra_keys() {
    let content = "\
[profile dev-account-adminaccess]
sso_session = old-session
sso_account_id = 111111111111
sso_role_name = AdminAccess
region = us-east-1
output = json
cli_pager = ";

    let (preamble, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "111111111111".to_string(),
        account_name: "dev-account".to_string(),
        roles: Some(vec!["AdminAccess".to_string()]),
        error: None,
    }];

    let (updated, added) =
        merge_sso_profiles(&mut sections, &results, "new-session", "eu-central-1");
    let rendered = render_config(&preamble, &sections);

    assert_eq!(updated, vec!["dev-account-adminaccess"]);
    assert!(added.is_empty());

    assert!(rendered.contains("sso_session = new-session"));
    assert!(rendered.contains("region = eu-central-1"));
    assert!(rendered.contains("output = json"));
    assert!(rendered.contains("cli_pager = "));
}

#[test]
fn test_merge_preserves_sso_session_section() {
    let content = "\
[sso-session my-sso]
sso_start_url = https://my-org.awsapps.com/start
sso_region = eu-central-1
sso_registration_scopes = sso:account:access

[profile existing]
sso_session = my-sso
sso_account_id = 111111111111
sso_role_name = AdminAccess
region = eu-central-1";

    let (preamble, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "222222222222".to_string(),
        account_name: "new-account".to_string(),
        roles: Some(vec!["ViewOnly".to_string()]),
        error: None,
    }];

    merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");
    let rendered = render_config(&preamble, &sections);

    assert!(rendered.contains("[sso-session my-sso]"));
    assert!(rendered.contains("sso_start_url = https://my-org.awsapps.com/start"));
    assert!(rendered.contains("sso_registration_scopes = sso:account:access"));
}

#[test]
fn test_merge_does_not_duplicate_keys_on_repeated_runs() {
    let content = "\
[profile dev-account-adminaccess]
sso_session = my-sso
sso_account_id = 111111111111
sso_role_name = AdminAccess
region = eu-central-1";

    let (_, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "111111111111".to_string(),
        account_name: "dev-account".to_string(),
        roles: Some(vec!["AdminAccess".to_string()]),
        error: None,
    }];

    merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");
    merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");

    assert_eq!(sections[0].entries.len(), 4);
}

#[test]
fn test_merge_multiple_roles_same_account() {
    let content = "[default]\nregion = us-east-1";

    let (_, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "111111111111".to_string(),
        account_name: "my-account".to_string(),
        roles: Some(vec!["AdminAccess".to_string(), "ReadOnly".to_string()]),
        error: None,
    }];

    let (_, added) = merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");

    assert_eq!(added.len(), 2);
    assert!(added.contains(&"my-account-adminaccess".to_string()));
    assert!(added.contains(&"my-account-readonly".to_string()));
    assert_eq!(sections.len(), 3);
}

#[test]
fn test_merge_skips_entries_with_errors() {
    let content = "[default]\nregion = us-east-1";

    let (_, mut sections) = parse_config_content(content);
    let results = vec![AccountResult {
        account_id: "111111111111".to_string(),
        account_name: "broken-account".to_string(),
        roles: None,
        error: Some("AccessDenied".to_string()),
    }];

    let (updated, added) = merge_sso_profiles(&mut sections, &results, "my-sso", "eu-central-1");

    assert!(updated.is_empty());
    assert!(added.is_empty());
    assert_eq!(sections.len(), 1);
}

#[test]
fn test_full_roundtrip_preserves_all_content() {
    let content = "\
# Main config file

[sso-session my-sso]
sso_start_url = https://my-org.awsapps.com/start
sso_region = eu-central-1
sso_registration_scopes = sso:account:access

[default]
region = eu-central-1
output = json

[profile legacy]
aws_access_key_id = AKIAEXAMPLE
aws_secret_access_key = SECRET
region = us-west-2
# keep this comment
cli_pager = 

[profile dev-adminaccess]
sso_session = my-sso
sso_account_id = 111111111111
sso_role_name = AdminAccess
region = eu-central-1
output = table";

    let (preamble, mut sections) = parse_config_content(content);
    let results = vec![
        AccountResult {
            account_id: "111111111111".to_string(),
            account_name: "dev".to_string(),
            roles: Some(vec!["AdminAccess".to_string()]),
            error: None,
        },
        AccountResult {
            account_id: "222222222222".to_string(),
            account_name: "prod".to_string(),
            roles: Some(vec!["ReadOnly".to_string()]),
            error: None,
        },
    ];

    merge_sso_profiles(&mut sections, &results, "my-sso", "eu-west-1");
    let rendered = render_config(&preamble, &sections);

    // Preamble preserved
    assert!(rendered.starts_with("# Main config file"));

    // sso-session untouched
    assert!(rendered.contains("sso_start_url = https://my-org.awsapps.com/start"));

    // default untouched
    assert!(rendered.contains("[default]\nregion = eu-central-1\noutput = json"));

    // legacy profile fully preserved including comment and all keys
    assert!(rendered.contains("aws_access_key_id = AKIAEXAMPLE"));
    assert!(rendered.contains("aws_secret_access_key = SECRET"));
    assert!(rendered.contains("# keep this comment"));
    assert!(rendered.contains("cli_pager = "));

    // dev profile updated (region changed, extra key preserved)
    assert!(rendered.contains("sso_session = my-sso"));
    assert!(rendered.contains("region = eu-west-1"));
    assert!(rendered.contains("output = table"));

    // new prod profile added
    assert!(rendered.contains("[profile prod-readonly]"));
    assert!(rendered.contains("sso_account_id = 222222222222"));
}
