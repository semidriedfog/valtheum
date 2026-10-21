use super::*;
#[derive(Deserialize)]
pub struct Input {
    language: String,
}
pub async fn preference() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        format!(
            "window.__valtheumLanguage={};",
            serde_json::to_string(language::code()).unwrap()
        ),
    )
}
fn save_at(path: &std::path::Path, value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(language::valid(value), "Unsupported language");
    let mut config = load_config_at(path)?;
    config.language = value.into();
    save_config_at(path, &config)
}
pub async fn update(Json(input): Json<Input>) -> Result<Json<serde_json::Value>, StatusCode> {
    if !language::valid(&input.language) {
        return Err(StatusCode::BAD_REQUEST);
    }
    tokio::task::spawn_blocking(move || {
        save_at(&get_config_path(), &input.language)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        language::set(&input.language);
        Ok(Json(serde_json::json!({"language":input.language})))
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_persists_and_preserves_public_settings_without_private_paths() -> anyhow::Result<()>
    {
        let root = std::env::temp_dir().join(format!(
            "vault-language-{}",
            crate::archive_crypto::hex(&crate::archive_crypto::random::<16>()?)
        ));
        std::fs::create_dir(&root)?;
        let path = root.join("vault_config.json");
        assert_eq!(load_config_at(&path)?.language, "en");
        std::fs::write(&path,br#"{"gallery_folder":"D:/photos","gallery_frame_seconds":27,"active_folder":"D:/private","folders":[],"language":"invalid"}"#)?;
        assert_eq!(load_config_at(&path)?.language, "en");
        for lang in ["en", "ja", "ko"] {
            save_at(&path, lang)?;
            let c = load_config_at(&path)?;
            assert_eq!(c.language, lang);
            assert_eq!(c.gallery_frame_seconds, 27);
            assert_eq!(c.gallery_folder, Some(PathBuf::from("D:/photos")));
            let saved = std::fs::read_to_string(&path)?;
            assert!(!saved.contains("private") && !saved.contains("active_folder"));
        }
        let before = std::fs::read(&path)?;
        assert!(save_at(&path, "zh").is_err());
        assert_eq!(std::fs::read(&path)?, before);
        std::fs::remove_file(&path)?;
        assert_eq!(load_config_at(&path)?.language, "ja");
        std::fs::remove_file(path.with_extension("json.bak"))?;
        std::fs::remove_dir(&root)?;
        Ok(())
    }
    #[tokio::test]
    async fn language_setting_requires_auth_and_current_generation_even_while_concealed() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let app = AppState::new(Library::unconfigured()).unwrap();
        app.concealed
            .store(true, std::sync::atomic::Ordering::Release);
        let router = build_router(app.clone());
        for (token, generation, expected) in [
            ("bad", 1, StatusCode::UNAUTHORIZED),
            (app.token.as_str(), 0, StatusCode::CONFLICT),
            (app.token.as_str(), 1, StatusCode::BAD_REQUEST),
        ] {
            let request = Request::builder()
                .uri("/api/settings/language")
                .method("POST")
                .header("host", "127.0.0.1:8080")
                .header("x-vault-token", token)
                .header("x-vault-library", generation.to_string())
                .header("content-type", "application/json")
                .body(Body::from(r#"{"language":"unsupported"}"#))
                .unwrap();
            assert_eq!(
                router.clone().oneshot(request).await.unwrap().status(),
                expected
            );
        }
    }
}
