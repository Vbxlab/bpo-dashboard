use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, Redirect, Json},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::{AppConfig, BpoData, DashboardSummary, SsoConfig};

pub type AppState = Arc<RwLock<AppStateInner>>;

pub struct AppStateInner {
    pub config: AppConfig,
    pub data: HashMap<String, BpoData>,
    pub refreshing: bool,
}

// ─── Query / Body Params ──────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub hub: Option<String>,
    pub me_min: Option<i32>,
    pub me_max: Option<i32>,
    pub te_min: Option<i32>,
    pub te_max: Option<i32>,
    pub profit_only: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub char_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct AddCharacterParams {
    pub client_id: String,
    pub client_secret: String,
    pub callback_url: String,
}

#[derive(Deserialize)]
pub struct DeleteCharacterParams {
    pub char_id: i64,
}

#[derive(Deserialize)]
pub struct RefreshParams {
    pub char_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct SsoCallbackQuery {
    pub code: String,
    pub state: String,
}

// ─── API: BPOs ─────────────────────────────────────────────────

pub async fn api_bpos(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Json<Vec<serde_json::Value>> {
    let state = state.read().await;
    let mut results: Vec<serde_json::Value> = Vec::new();

    let data_iter: Vec<&BpoData> = if let Some(cid) = params.char_id {
        state.config.characters.iter()
            .filter(|c| c.id == cid)
            .filter_map(|c| state.data.get(&c.name))
            .collect()
    } else {
        state.data.values().collect()
    };

    for bpo_data in data_iter {
        for bpo in &bpo_data.bpos {
            let query = params.q.as_deref().unwrap_or("").to_lowercase();
            if !query.is_empty() {
                let name_match = bpo.bp_name.to_lowercase().contains(&query);
                let product_match = bpo.product_name.to_lowercase().contains(&query);
                if !name_match && !product_match { continue; }
            }

            if let Some(me_min) = params.me_min { if bpo.me < me_min { continue; } }
            if let Some(me_max) = params.me_max { if bpo.me > me_max { continue; } }
            if let Some(te_min) = params.te_min { if bpo.te < te_min { continue; } }
            if let Some(te_max) = params.te_max { if bpo.te > te_max { continue; } }

            if params.profit_only.unwrap_or(false) {
                if let Some(ref hub) = params.hub {
                    if bpo.profit(hub) <= 0.0 { continue; }
                } else {
                    let hubs = &["Jita", "Amarr", "Dodixie", "Rens"];
                    let any_profit = hubs.iter().any(|h| bpo.profit(h) > 0.0);
                    if !any_profit { continue; }
                }
            }

            let (best_hub, best_profit) = bpo.best_hub();

            let entry = serde_json::json!({
                "bp_name": bpo.bp_name,
                "product_name": bpo.product_name,
                "product_qty": bpo.product_qty,
                "me": bpo.me,
                "te": bpo.te,
                "character": bpo_data.character_name,
                "profit_jita": bpo.profit("Jita"),
                "profit_amarr": bpo.profit("Amarr"),
                "profit_dodixie": bpo.profit("Dodixie"),
                "profit_rens": bpo.profit("Rens"),
                "margin_jita": bpo.margin_percent("Jita"),
                "margin_amarr": bpo.margin_percent("Amarr"),
                "margin_dodixie": bpo.margin_percent("Dodixie"),
                "margin_rens": bpo.margin_percent("Rens"),
                "best_hub": best_hub,
                "best_profit": best_profit,
                "mat_cost_jita": bpo.mat_costs.get("Jita").copied().unwrap_or(0.0),
                "mat_cost_amarr": bpo.mat_costs.get("Amarr").copied().unwrap_or(0.0),
                "mat_cost_dodixie": bpo.mat_costs.get("Dodixie").copied().unwrap_or(0.0),
                "mat_cost_rens": bpo.mat_costs.get("Rens").copied().unwrap_or(0.0),
                "price_jita": bpo.product_prices.get("Jita").copied().unwrap_or(0.0),
                "price_amarr": bpo.product_prices.get("Amarr").copied().unwrap_or(0.0),
                "price_dodixie": bpo.product_prices.get("Dodixie").copied().unwrap_or(0.0),
                "price_rens": bpo.product_prices.get("Rens").copied().unwrap_or(0.0),
            });

            results.push(entry);
        }
    }

    let sort_key = params.sort_by.as_deref().unwrap_or("bp_name");
    let sort_desc = params.sort_desc.unwrap_or(false);
    results.sort_by(|a, b| {
        let cmp = match sort_key {
            "profit" => a.get("best_profit").and_then(|v| v.as_f64()).unwrap_or(0.0)
                .partial_cmp(&b.get("best_profit").and_then(|v| v.as_f64()).unwrap_or(0.0)),
            "price" => {
                let hub = params.hub.as_deref().unwrap_or("Jita").to_lowercase();
                let key = format!("price_{}", hub);
                a.get(&key).and_then(|v| v.as_f64()).unwrap_or(0.0)
                    .partial_cmp(&b.get(&key).and_then(|v| v.as_f64()).unwrap_or(0.0))
            }
            "margin" => a.get("margin_dodixie").and_then(|v| v.as_f64()).unwrap_or(0.0)
                .partial_cmp(&b.get("margin_dodixie").and_then(|v| v.as_f64()).unwrap_or(0.0)),
            "me" => Some(a.get("me").and_then(|v| v.as_i64()).unwrap_or(0)
                .cmp(&b.get("me").and_then(|v| v.as_i64()).unwrap_or(0))),
            "te" => Some(a.get("te").and_then(|v| v.as_i64()).unwrap_or(0)
                .cmp(&b.get("te").and_then(|v| v.as_i64()).unwrap_or(0))),
            _ => a.get("bp_name").and_then(|v| v.as_str()).unwrap_or("")
                .partial_cmp(b.get("bp_name").and_then(|v| v.as_str()).unwrap_or("")),
        };
        let ord = cmp.unwrap_or(std::cmp::Ordering::Equal);
        if sort_desc { ord.reverse() } else { ord }
    });

    Json(results)
}

// ─── API: Summary ─────────────────────────────────────────────

pub async fn api_summary(State(state): State<AppState>) -> Json<serde_json::Value> {
    let state = state.read().await;
    let mut all_bpos: Vec<crate::models::BpoEntry> = Vec::new();
    for data in state.data.values() {
        all_bpos.extend(data.bpos.clone());
    }
    let summary = DashboardSummary::from_bpos(&all_bpos);
    // Convert to JSON and add computed profit fields for each hub
    let mut json = serde_json::to_value(&summary).unwrap_or_default();
    // Enrich top_profits and worst_losses with computed profit values
    let hubs = ["Jita", "Amarr", "Dodixie", "Rens"];
    for key in ["top_profits", "worst_losses"] {
        if let Some(arr) = json.get_mut(key).and_then(|v| v.as_array_mut()) {
            for item in arr.iter_mut() {
                if let Some(obj) = item.as_object_mut() {
                    for hub in &hubs {
                        let hub_lower = hub.to_lowercase();
                        let revenue = obj.get("product_prices").and_then(|p| p.get(hub)).and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let cost = obj.get("mat_costs").and_then(|p| p.get(hub)).and_then(|v| v.as_f64()).unwrap_or(0.0);
                        obj.insert(format!("profit_{}", &hub_lower), serde_json::Value::from(revenue - cost));
                    }
                }
            }
        }
    }
    Json(json)
}

// ─── API: Materials ────────────────────────────────────────────

pub async fn api_materials(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    let state = state.read().await;
    let mut all_bpos: Vec<crate::models::BpoEntry> = Vec::new();
    for data in state.data.values() {
        all_bpos.extend(data.bpos.clone());
    }
    // Build materials with BPO parent info
    let mut mat_map: std::collections::HashMap<i64, (String, i64, Vec<(String, i64)>)> = std::collections::HashMap::new();
    for bpo in &all_bpos {
        for mat in &bpo.materials {
            let entry = mat_map.entry(mat.typeid).or_insert((mat.name.clone(), 0, Vec::new()));
            entry.1 += mat.quantity * bpo.product_qty;
            entry.2.push((bpo.bp_name.clone(), mat.quantity));
        }
    }
    let mut result: Vec<serde_json::Value> = mat_map.iter().map(|(tid, (name, total_qty, bpo_list))| {
        let bpos_using: Vec<serde_json::Value> = bpo_list.iter().map(|(bp_name, qty)| {
            serde_json::json!({"bp_name": bp_name, "quantity": qty})
        }).collect();
        serde_json::json!({
            "name": name,
            "type_id": tid,
            "total_quantity": total_qty,
            "used_by": bpos_using,
        })
    }).collect();
    result.sort_by(|a, b| {
        b.get("total_quantity").and_then(|v| v.as_i64()).unwrap_or(0)
            .cmp(&a.get("total_quantity").and_then(|v| v.as_i64()).unwrap_or(0))
    });
    Json(result)
}

// ─── API: Improvements ─────────────────────────────────────────

pub async fn api_improvements(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    let state = state.read().await;
    let mut results = Vec::new();
    for data in state.data.values() {
        for bpo in &data.bpos {
            if bpo.me < 10 || bpo.te < 20 {
                let current_cost = bpo.mat_costs.get("Jita").copied().unwrap_or(0.0);
                let me_saving_pct = if bpo.me < 10 { (10 - bpo.me) as f64 / 100.0 } else { 0.0 };
                let estimated_saving = current_cost * me_saving_pct;

                results.push(serde_json::json!({
                    "bp_name": bpo.bp_name,
                    "product_name": bpo.product_name,
                    "character": data.character_name,
                    "me": bpo.me,
                    "te": bpo.te,
                    "me_needs": bpo.me < 10,
                    "te_needs": bpo.te < 20,
                    "current_mat_cost_jita": current_cost,
                    "estimated_me_saving_jita": estimated_saving,
                    "priority": estimated_saving,
                }));
            }
        }
    }
    results.sort_by(|a, b| {
        let pa = a.get("priority").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let pb = b.get("priority").and_then(|v| v.as_f64()).unwrap_or(0.0);
        pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
    });
    Json(results)
}

// ─── API: Characters ────────────────────────────────────────────

pub async fn api_characters(State(state): State<AppState>) -> Json<serde_json::Value> {
    let state = state.read().await;
    let has_default_sso = state.config.default_sso.is_some();
    let chars: Vec<serde_json::Value> = state.config.characters.iter().map(|c| {
        serde_json::json!({
            "id": c.id,
            "name": c.name,
            "has_data": state.data.contains_key(&c.name),
        })
    }).collect();
    // Include default_sso flag so frontend knows if one-click is available
    Json(serde_json::json!({
        "default_sso_configured": has_default_sso,
        "characters": chars,
    }))
}

pub async fn api_delete_character(
    State(state): State<AppState>,
    Json(params): Json<DeleteCharacterParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut state = state.write().await;
    let name = state.config.characters.iter()
        .find(|c| c.id == params.char_id)
        .map(|c| c.name.clone());

    if let Some(name) = name {
        state.config.characters.retain(|c| c.id != params.char_id);
        state.data.remove(&name);

        // Delete data file
        let data_path = format!("{}/bpo-data-{}.json", state.config.data_dir, name.replace(' ', "_"));
        let _ = std::fs::remove_file(&data_path);

        // Save config
        let config_path = AppConfig::default_path();
        let _ = state.config.save(&config_path);

        Ok(Json(serde_json::json!({"status": "deleted", "name": name})))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// ─── API: Refresh ────────────────────────────────────────────────

pub async fn api_refresh(
    State(state): State<AppState>,
    Json(params): Json<RefreshParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    {
        let s = state.read().await;
        if s.refreshing {
            return Ok(Json(serde_json::json!({"status": "already_refreshing"})));
        }
    }

    {
        let mut s = state.write().await;
        s.refreshing = true;
    }

    let result = {
        let mut s = state.write().await;
        let chars_to_refresh: Vec<usize> = if let Some(cid) = params.char_id {
            s.config.characters.iter().enumerate()
                .filter(|(_, c)| c.id == cid)
                .map(|(i, _)| i)
                .collect()
        } else {
            s.config.characters.iter().enumerate().map(|(i, _)| i).collect()
        };

        for idx in chars_to_refresh {
            match crate::esi::fetch_and_build(&mut s.config.characters[idx]).await {
                Ok(bpo_data) => {
                    let name = s.config.characters[idx].name.clone();
                    s.data.insert(name, bpo_data);
                }
                Err(e) => {
                    eprintln!("Error refreshing character {}: {}", s.config.characters[idx].name, e);
                }
            }
        }

        let config_path = AppConfig::default_path();
        let _ = s.config.save(&config_path);

        for (name, data) in &s.data {
            let path = format!("{}/bpo-data-{}.json", s.config.data_dir, name.replace(' ', "_"));
            if let Ok(content) = serde_json::to_string_pretty(data) {
                let _ = std::fs::write(&path, content);
            }
        }

        s.refreshing = false;
        Ok(Json(serde_json::json!({"status": "ok"})))
    };

    result
}

// ─── SSO OAuth Flow ────────────────────────────────────────────
// Two modes:
// 1. One-click: if default_sso is configured in config.json, /api/sso/start
//    generates the EVE SSO URL automatically — user just clicks "Se connecter".
// 2. Manual: /api/sso/authorize lets user provide their own Client ID/Secret
//    (for people running their own EVE developer app).

/// One-click SSO start — uses default_sso from config
pub async fn api_sso_start(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let sso = {
        let s = state.read().await;
        s.config.default_sso.clone()
    };

    let sso = sso.ok_or((StatusCode::PRECONDITION_FAILED,
        "No default SSO configured. Add default_sso to config.json or use /api/sso/authorize.".to_string()))?;

    // Generate state token
    let state_token = format!("{:016x}", rand::random::<u64>());

    // Store pending SSO state
    {
        let s = state.read().await;
        let pending = serde_json::json!({
            "client_id": sso.client_id,
            "client_secret": sso.client_secret,
            "callback_url": sso.callback_url,
            "state": state_token,
        });
        let path = format!("{}/pending-sso-{}.json", s.config.data_dir, state_token);
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&pending).unwrap_or_default());
    }

    let scopes = "esi-characters.read_blueprints.v1";
    let sso_url = format!(
        "https://login.eveonline.com/v2/oauth/authorize/?response_type=code&redirect_uri={}&client_id={}&scope={}&state={}",
        urlencoding::encode(&sso.callback_url),
        urlencoding::encode(&sso.client_id),
        urlencoding::encode(scopes),
        state_token,
    );

    Ok(Json(serde_json::json!({"url": sso_url, "state": state_token})))
}

/// Manual SSO — user provides their own Client ID/Secret
pub async fn api_sso_authorize(
    State(state): State<AppState>,
    Json(params): Json<AddCharacterParams>,
) -> Json<serde_json::Value> {
    let state_token = format!("{:016x}", rand::random::<u64>());

    {
        let s = state.read().await;
        let pending = serde_json::json!({
            "client_id": params.client_id,
            "client_secret": params.client_secret,
            "callback_url": params.callback_url,
            "state": state_token,
        });
        let path = format!("{}/pending-sso-{}.json", s.config.data_dir, state_token);
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&pending).unwrap_or_default());
    }

    let scopes = "esi-characters.read_blueprints.v1";
    let sso_url = format!(
        "https://login.eveonline.com/v2/oauth/authorize/?response_type=code&redirect_uri={}&client_id={}&scope={}&state={}",
        urlencoding::encode(&params.callback_url),
        urlencoding::encode(&params.client_id),
        urlencoding::encode(scopes),
        state_token,
    );

    Json(serde_json::json!({"url": sso_url, "state": state_token}))
}

/// SSO Callback — handles the redirect from EVE Online
pub async fn api_sso_callback(
    State(state): State<AppState>,
    Query(params): Query<SsoCallbackQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    // Load pending SSO config
    let pending_path = {
        let s = state.read().await;
        format!("{}/pending-sso-{}.json", s.config.data_dir, params.state)
    };
    let pending_content = std::fs::read_to_string(&pending_path)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid state token: {}", e)))?;
    let pending: serde_json::Value = serde_json::from_str(&pending_content)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid pending config: {}", e)))?;

    let client_id = pending.get("client_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let client_secret = pending.get("client_secret").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let callback_url = pending.get("callback_url").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Exchange code for token
    let auth = base64::Engine::encode(&base64::engine::general_purpose::STANDARD,
        format!("{}:{}", client_id, client_secret));
    let token_body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}",
        urlencoding::encode(&params.code),
        urlencoding::encode(&callback_url),
    );

    let client = reqwest::Client::new();
    let resp = client.post("https://login.eveonline.com/v2/oauth/token")
        .header("Authorization", format!("Basic {}", auth))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "BPO-Dashboard/1.0")
        .body(token_body)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("SSO token exchange failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err((StatusCode::BAD_GATEWAY, format!("SSO error {}: {}", status, body)));
    }

    let token_data: serde_json::Value = resp.json().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Token parse error: {}", e)))?;

    let access_token = token_data.get("access_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let refresh_token = token_data.get("refresh_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let expires_in = token_data.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(1100);

    // Verify token and get character info
    let verify_resp = client.get("https://login.eveonline.com/v2/oauth/verify")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "BPO-Dashboard/1.0")
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Token verify failed: {}", e)))?;

    let verify: serde_json::Value = verify_resp.json().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Verify parse error: {}", e)))?;

    // Extract character ID — EVE SSO v2 uses "CharacterID"
    let char_id: i64 = verify.get("CharacterID").and_then(|v| v.as_i64())
        .unwrap_or(0);
    let char_name = verify.get("CharacterName").and_then(|v| v.as_str())
        .unwrap_or("Unknown").to_string();

    // Save character to config
    {
        let mut s = state.write().await;

        // Determine SSO config: use pending SSO (from default or manual)
        let sso_config = SsoConfig {
            client_id,
            client_secret,
            callback_url,
        };

        if let Some(char) = s.config.characters.iter_mut().find(|c| c.id == char_id) {
            // Update existing character's tokens
            char.tokens.access_token = access_token;
            char.tokens.refresh_token = refresh_token;
            char.tokens.expires_at = Some(chrono::Utc::now().timestamp() as f64 + expires_in as f64);
        } else {
            // Add new character
            s.config.characters.push(crate::models::Character {
                id: char_id,
                name: char_name.clone(),
                sso: sso_config,
                tokens: crate::models::TokenData {
                    access_token,
                    refresh_token,
                    expires_at: Some(chrono::Utc::now().timestamp() as f64 + expires_in as f64),
                    character_id: char_id,
                    character_name: char_name.clone(),
                },
            });
        }

        let config_path = AppConfig::default_path();
        let _ = s.config.save(&config_path);

        // Clean up pending file
        let _ = std::fs::remove_file(&pending_path);
    }

    Ok(Redirect::to("/"))
}

// ─── Frontend ─────────────────────────────────────────────────

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../frontend/index.html"))
}