use beebotos_launcher::{
    load_env_config, render_env_config, EnvConfig, ALLOW_NETWORK_KEY, IMAGE_GENERATION_KEY,
    JWT_SECRET_KEY, TEXT_MODEL_KEY, VIDEO_GENERATION_KEY,
};

#[test]
fn load_env_config_reads_known_keys_only() {
    let content = "\
# keep this comment
DEEPSEEK_API_KEY=text-key
IMAGE_GENERATION_API_KEY=image-key
VIDEO_GENERATION_API_KEY=video-key
UNKNOWN_KEY=keep-me
";

    let config = load_env_config(content);

    assert_eq!(config.text_model_key, "text-key");
    assert_eq!(config.image_generation_key, "image-key");
    assert_eq!(config.video_generation_key, "video-key");
}

#[test]
fn render_env_config_preserves_comments_and_unknown_keys() {
    let content = "\
# keep this comment
UNKNOWN_KEY=keep-me
DEEPSEEK_API_KEY=old-text
";
    let config = EnvConfig {
        text_model_key: "new-text".to_string(),
        image_generation_key: "new-image".to_string(),
        video_generation_key: "new-video".to_string(),
    };

    let rendered = render_env_config(content, &config);

    assert!(rendered.contains("# keep this comment"));
    assert!(rendered.contains("UNKNOWN_KEY=keep-me"));
    assert!(rendered.contains(&format!("{TEXT_MODEL_KEY}=new-text")));
    assert!(rendered.contains(&format!("{IMAGE_GENERATION_KEY}=new-image")));
    assert!(rendered.contains(&format!("{VIDEO_GENERATION_KEY}=new-video")));
    assert!(rendered.contains(&format!("{ALLOW_NETWORK_KEY}=1")));
    assert!(rendered.contains(&format!("{JWT_SECRET_KEY}=bee-jwt-")));
    assert!(!rendered.contains("old-text"));
}

#[test]
fn render_env_config_keeps_blank_values_out_of_file() {
    let rendered = render_env_config(
        "EXISTING=value\n",
        &EnvConfig {
            text_model_key: String::new(),
            image_generation_key: "image-key".to_string(),
            video_generation_key: String::new(),
        },
    );

    assert!(rendered.contains("EXISTING=value"));
    assert!(!rendered.contains(&format!("{TEXT_MODEL_KEY}=")));
    assert!(rendered.contains(&format!("{IMAGE_GENERATION_KEY}=image-key")));
    assert!(!rendered.contains(&format!("{VIDEO_GENERATION_KEY}=")));
    assert!(rendered.contains(&format!("{ALLOW_NETWORK_KEY}=1")));
    assert!(rendered.contains(&format!("{JWT_SECRET_KEY}=bee-jwt-")));
}

#[test]
fn render_env_config_preserves_existing_valid_jwt_secret() {
    let secret = "existing-local-jwt-secret-at-least-32-characters";
    let rendered = render_env_config(
        &format!("{JWT_SECRET_KEY}={secret}\n"),
        &EnvConfig::default(),
    );

    assert!(rendered.contains(&format!("{JWT_SECRET_KEY}={secret}")));
}

#[test]
fn render_env_config_replaces_short_jwt_secret() {
    let rendered = render_env_config(&format!("{JWT_SECRET_KEY}=short\n"), &EnvConfig::default());

    assert!(!rendered.contains(&format!("{JWT_SECRET_KEY}=short")));
    assert!(rendered.contains(&format!("{JWT_SECRET_KEY}=bee-jwt-")));
}
