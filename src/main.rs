mod api;
mod esi;
mod models;

use axum::Router;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use api::AppStateInner;
use models::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("BPO_CONFIG")
        .unwrap_or_else(|_| AppConfig::default_path());

    // Ensure config directory exists before loading/creating config
    if let Some(parent) = std::path::Path::new(&config_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let config = AppConfig::load(&config_path)
        .unwrap_or_else(|_| {
            eprintln!("⚠️  No config found at {}", config_path);
            eprintln!("   Creating default config with placeholders...");
            eprintln!("   Edit the file and add your EVE developer app credentials.");
            let mut c = AppConfig::default_config();
            c.data_dir = std::path::Path::new(&config_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| c.data_dir.clone());
            // Save placeholder config
            let _ = c.save(&config_path);
            c
        });

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    let port = config.port;

    // Load cached data for each character
    let mut data = HashMap::new();
    for char in &config.characters {
        let path = format!("{}/bpo-data-{}.json", config.data_dir, char.name.replace(' ', "_"));
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(bpo_data) = serde_json::from_str::<models::BpoData>(&content) {
                data.insert(char.name.clone(), bpo_data);
            }
        }
    }

    // Save config to ensure it exists
    let _ = config.save(&config_path);

    let state = Arc::new(RwLock::new(AppStateInner {
        config,
        data,
        refreshing: false,
    }));

    let app = Router::new()
        .route("/", axum::routing::get(api::index))
        .route("/api/bpos", axum::routing::get(api::api_bpos))
        .route("/api/summary", axum::routing::get(api::api_summary))
        .route("/api/materials", axum::routing::get(api::api_materials))
        .route("/api/improvements", axum::routing::get(api::api_improvements))
        .route("/api/characters", axum::routing::get(api::api_characters))
        .route("/api/refresh", axum::routing::post(api::api_refresh))
        // SSO: one-click (uses default_sso from config)
        .route("/api/sso/start", axum::routing::get(api::api_sso_start))
        // SSO: manual (user provides Client ID/Secret)
        .route("/api/sso/authorize", axum::routing::post(api::api_sso_authorize))
        // SSO: callback (EVE Online redirects here after login)
        .route("/api/sso/callback", axum::routing::get(api::api_sso_callback))
        .route("/api/characters/delete", axum::routing::post(api::api_delete_character))
        .with_state(state.clone());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 BPO Dashboard running on http://localhost:{}", port);
    if state.read().await.config.default_sso.is_some() {
        println!("   ✅ SSO configured — add characters with one click");
    } else {
        println!("   ⚠️  Configure default_sso in config.json to enable one-click SSO");
        println!("   See README.md for setup instructions");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}