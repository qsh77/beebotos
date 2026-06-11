use std::path::Path;

use uuid::Uuid;

pub const TEXT_MODEL_KEY: &str = "DEEPSEEK_API_KEY";
pub const IMAGE_GENERATION_KEY: &str = "IMAGE_GENERATION_API_KEY";
pub const VIDEO_GENERATION_KEY: &str = "VIDEO_GENERATION_API_KEY";
pub const ALLOW_NETWORK_KEY: &str = "BEE_ALLOW_NETWORK";
pub const JWT_SECRET_KEY: &str = "BEE__JWT__SECRET";
pub const WEB_CONSOLE_URL: &str = "http://localhost:8090";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherCommand {
    Ui,
    Start,
    Stop,
    Restart,
    Status,
    OpenWeb,
    OpenLogs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvConfig {
    pub text_model_key: String,
    pub image_generation_key: String,
    pub video_generation_key: String,
}

pub fn parse_launcher_command<I, S>(args: I) -> LauncherCommand
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    let Some(arg) = args.next() else {
        return LauncherCommand::Ui;
    };
    match arg.as_ref() {
        "--start" | "start" => LauncherCommand::Start,
        "--stop" | "stop" => LauncherCommand::Stop,
        "--restart" | "restart" => LauncherCommand::Restart,
        "--status" | "status" => LauncherCommand::Status,
        "--open" | "open" | "web" => LauncherCommand::OpenWeb,
        "--logs" | "logs" => LauncherCommand::OpenLogs,
        _ => LauncherCommand::Ui,
    }
}

pub fn load_env_config(content: &str) -> EnvConfig {
    let mut config = EnvConfig::default();
    for line in content.lines() {
        let Some((key, value)) = parse_env_line(line) else {
            continue;
        };
        match key {
            TEXT_MODEL_KEY => config.text_model_key = value.to_string(),
            IMAGE_GENERATION_KEY => config.image_generation_key = value.to_string(),
            VIDEO_GENERATION_KEY => config.video_generation_key = value.to_string(),
            _ => {}
        }
    }
    config
}

pub fn render_env_config(existing: &str, config: &EnvConfig) -> String {
    let jwt_secret = find_env_value(existing, JWT_SECRET_KEY)
        .filter(|value| value.len() >= 32)
        .map(str::to_owned)
        .unwrap_or_else(generate_jwt_secret);

    let mut lines = Vec::new();
    for line in existing.lines() {
        if parse_env_line(line)
            .map(|(key, _)| is_launcher_managed_key(key))
            .unwrap_or(false)
        {
            continue;
        }
        lines.push(line.to_string());
    }

    push_key_value(&mut lines, TEXT_MODEL_KEY, &config.text_model_key);
    push_key_value(
        &mut lines,
        IMAGE_GENERATION_KEY,
        &config.image_generation_key,
    );
    push_key_value(
        &mut lines,
        VIDEO_GENERATION_KEY,
        &config.video_generation_key,
    );
    lines.push(format!("{ALLOW_NETWORK_KEY}=1"));
    lines.push(format!("{JWT_SECRET_KEY}={jwt_secret}"));

    let mut rendered = lines.join("\n");
    rendered.push('\n');
    rendered
}

pub fn read_env_file(path: &Path) -> anyhow::Result<EnvConfig> {
    if !path.exists() {
        return Ok(EnvConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    Ok(load_env_config(&content))
}

pub fn write_env_file(path: &Path, config: &EnvConfig) -> anyhow::Result<()> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    std::fs::write(path, render_env_config(&existing, config))?;
    Ok(())
}

fn is_launcher_managed_key(key: &str) -> bool {
    matches!(
        key,
        TEXT_MODEL_KEY
            | IMAGE_GENERATION_KEY
            | VIDEO_GENERATION_KEY
            | ALLOW_NETWORK_KEY
            | JWT_SECRET_KEY
    )
}

fn find_env_value<'a>(content: &'a str, target_key: &str) -> Option<&'a str> {
    content.lines().find_map(|line| {
        let (key, value) = parse_env_line(line)?;
        (key == target_key).then_some(value)
    })
}

fn generate_jwt_secret() -> String {
    format!(
        "bee-jwt-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn push_key_value(lines: &mut Vec<String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        lines.push(format!("{key}={}", format_env_value(value)));
    }
}

fn parse_env_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, unquote_env_value(value.trim())))
}

fn unquote_env_value(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn format_env_value(value: &str) -> String {
    if value
        .chars()
        .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '#'))
    {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "windows")]
pub mod windows;
