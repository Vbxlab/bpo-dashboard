use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::models::{BpoData, BpoEntry, Character, MaterialEntry};

const ESI_BASE: &str = "https://esi.evetech.net/latest";
const SSO_TOKEN_URL: &str = "https://login.eveonline.com/v2/oauth/token";
const FUZZWORK_URL: &str = "https://www.fuzzwork.co.uk/blueprint/api/blueprint.php";

const HUBS: [(&str, i64); 4] = [
    ("Jita", 10000002),
    ("Amarr", 10000043),
    ("Dodixie", 10000032),
    ("Rens", 10000030),
];

// ─── SSO Token Refresh ────────────────────────────────────────

pub async fn refresh_token(character: &mut Character) -> Result<()> {
    let auth = base64::Engine::encode(&base64::engine::general_purpose::STANDARD,
        format!("{}:{}", character.sso.client_id, character.sso.client_secret));

    let params = format!(
        "grant_type=refresh_token&refresh_token={}",
        urlencoding::encode(&character.tokens.refresh_token)
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(SSO_TOKEN_URL)
        .header("Authorization", format!("Basic {}", auth))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "BPO-Dashboard/1.0")
        .body(params)
        .send()
        .await
        .context("Failed to refresh SSO token")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("SSO refresh failed ({}): {}", status, body);
    }

    let new_tok: serde_json::Value = resp.json().await?;
    character.tokens.access_token = new_tok["access_token"].as_str().unwrap_or("").to_string();
    character.tokens.refresh_token = new_tok["refresh_token"].as_str().unwrap_or("").to_string();
    let expires_in = new_tok["expires_in"].as_i64().unwrap_or(1100) as f64;
    character.tokens.expires_at = Some(chrono::Utc::now().timestamp() as f64 + expires_in);

    Ok(())
}

// ─── ESI Fetch ────────────────────────────────────────────────

async fn esi_get(url: &str, token: Option<&str>) -> Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let mut req = client.get(url)
        .header("User-Agent", "BPO-Dashboard/1.0");
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("ESI error {}: {}", resp.status(), resp.url());
    }
    Ok(resp.json().await?)
}

// ─── Fetch BPOs ───────────────────────────────────────────────

async fn fetch_bpos(char_id: i64, token: &str) -> Result<Vec<serde_json::Value>> {
    let mut all = Vec::new();
    let mut page = 1;
    loop {
        let url = format!("{}/characters/{}/blueprints/?datasource=tranquility&page={}",
            ESI_BASE, char_id, page);
        let data = esi_get(&url, Some(token)).await?;
        let arr = data.as_array().cloned().unwrap_or_default();
        if arr.is_empty() { break; }
        let arr_len = arr.len();
        all.extend(arr);
        if arr_len < 500 { break; }
        page += 1;
    }
    Ok(all.into_iter().filter(|bp| bp.get("quantity").and_then(|v| v.as_i64()) == Some(-1)).collect())
}

// ─── Resolve Type Names ───────────────────────────────────────

async fn resolve_type_names(type_ids: &[i64]) -> Result<HashMap<i64, String>> {
    let mut names = HashMap::new();
    // Batch via universe/names endpoint
    let client = reqwest::Client::new();
    let ids: Vec<i64> = type_ids.to_vec();
    // ESI universe/names accepts up to ~1000 IDs
    for chunk in ids.chunks(500) {
        let resp = client.post(&format!("{}/universe/names/?datasource=tranquility", ESI_BASE))
            .header("User-Agent", "BPO-Dashboard/1.0")
            .json(&chunk)
            .send()
            .await?;
        if resp.status().is_success() {
            let arr: Vec<serde_json::Value> = resp.json().await?;
            for item in &arr {
                if let (Some(id), Some(name)) = (item.get("id").and_then(|v| v.as_i64()),
                    item.get("name").and_then(|v| v.as_str())) {
                    names.insert(id, name.to_string());
                }
            }
        }
    }
    // Fallback for missing
    for &tid in type_ids {
        if !names.contains_key(&tid) {
            if let Ok(data) = esi_get(&format!("{}/universe/types/{}/?datasource=tranquility", ESI_BASE, tid), None).await {
                if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                    names.insert(tid, name.to_string());
                }
            }
        }
    }
    Ok(names)
}

// ─── Fetch Manufacturing Data ─────────────────────────────────

async fn fetch_bp_manufacturing(type_ids: &[i64]) -> Result<HashMap<i64, serde_json::Value>> {
    let client = reqwest::Client::new();
    let mut bp_mfg = HashMap::new();
    for &tid in type_ids {
        let url = format!("{}?typeid={}", FUZZWORK_URL, tid);
        let resp = client.get(&url)
            .header("User-Agent", "BPO-Dashboard/1.0")
            .header("Accept", "application/json")
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                if let Ok(data) = r.json::<serde_json::Value>().await {
                    bp_mfg.insert(tid, data);
                }
            }
            _ => {}
        }
        // Rate limit: small delay
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(bp_mfg)
}

// ─── Fetch Market Prices ──────────────────────────────────────

async fn fetch_hub_prices(type_ids: &[i64]) -> Result<HashMap<String, HashMap<i64, f64>>> {
    let mut hub_prices: HashMap<String, HashMap<i64, f64>> = HashMap::new();
    for (hub_name, region_id) in &HUBS {
        let mut prices = HashMap::new();
        for &tid in type_ids {
            let url = format!("{}/markets/{}/orders/?datasource=tranquility&type_id={}&order_type=sell",
                ESI_BASE, region_id, tid);
            match esi_get(&url, None).await {
                Ok(data) => {
                    let sell_prices: Vec<f64> = data.as_array()
                        .map(|arr| arr.iter()
                            .filter(|o| !o.get("is_buy_order").and_then(|v| v.as_bool()).unwrap_or(false))
                            .filter_map(|o| o.get("price").and_then(|v| v.as_f64()))
                            .collect())
                        .unwrap_or_default();
                    prices.insert(tid, sell_prices.iter().cloned().fold(f64::INFINITY, f64::min));
                }
                Err(_) => {
                    prices.insert(tid, 0.0);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        hub_prices.insert(hub_name.to_string(), prices);
    }
    Ok(hub_prices)
}

// ─── Apply ME ─────────────────────────────────────────────────

fn apply_me(base_qty: i64, me: i32) -> i64 {
    if me > 0 {
        ((base_qty as f64 * (1.0 - me as f64 / 100.0)).ceil() as i64).max(1)
    } else {
        base_qty
    }
}

// ─── Build BPO List ───────────────────────────────────────────

pub async fn fetch_and_build(character: &mut Character) -> Result<BpoData> {
    // 1. Refresh token
    refresh_token(character).await?;
    let token = character.tokens.access_token.clone();
    let char_id = character.id;

    // 2. Fetch BPOs
    let bpos_raw = fetch_bpos(char_id, &token).await?;
    let bp_type_ids: Vec<i64> = bpos_raw.iter()
        .filter_map(|bp| bp.get("type_id").and_then(|v| v.as_i64()))
        .collect();

    // 3. Resolve names
    let type_names = resolve_type_names(&bp_type_ids).await?;

    // 4. Fetch manufacturing data
    let bp_mfg = fetch_bp_manufacturing(&bp_type_ids).await?;

    // 5. Collect all type IDs for pricing
    let mut all_type_ids: Vec<i64> = bp_type_ids.clone();
    for (_tid, mfg) in &bp_mfg {
        if let Some(details) = mfg.get("blueprintDetails") {
            if let Some(pid) = details.get("productTypeID").and_then(|v| v.as_i64()) {
                if pid != 0 { all_type_ids.push(pid); }
            }
        }
        if let Some(materials) = mfg.get("activityMaterials").and_then(|v| v.get("1")) {
            if let Some(arr) = materials.as_array() {
                for mat in arr {
                    if let Some(mid) = mat.get("typeid").and_then(|v| v.as_i64()) {
                        all_type_ids.push(mid);
                    }
                }
            }
        }
    }
    all_type_ids.sort();
    all_type_ids.dedup();

    // 6. Fetch prices
    let hub_prices = fetch_hub_prices(&all_type_ids).await?;

    // 7. Build BPO list
    let mut bpo_list = Vec::new();
    for bp in &bpos_raw {
        let tid = bp.get("type_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let name = type_names.get(&tid).cloned().unwrap_or_else(|| format!("Type {}", tid));
        let mfg = bp_mfg.get(&tid);
        let details = mfg.and_then(|m| m.get("blueprintDetails"));
        let materials_raw = mfg.and_then(|m| m.get("activityMaterials").and_then(|v| v.get("1")));

        let product_id = details.and_then(|d| d.get("productTypeID").and_then(|v| v.as_i64())).unwrap_or(0);
        let product_name = details.and_then(|d| d.get("productTypeName").and_then(|v| v.as_str()))
            .unwrap_or("N/A").to_string();
        let product_qty = details.and_then(|d| d.get("productQuantity").and_then(|v| v.as_i64())).unwrap_or(0);
        let me = bp.get("material_efficiency").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let te = bp.get("time_efficiency").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        let mut materials = Vec::new();
        let mut mat_costs: HashMap<String, f64> = HashMap::new();
        for hub in &["Jita", "Amarr", "Dodixie", "Rens"] {
            mat_costs.insert(hub.to_string(), 0.0);
        }

        if let Some(arr) = materials_raw.and_then(|v| v.as_array()) {
            for mat in arr {
                let mid = mat.get("typeid").and_then(|v| v.as_i64()).unwrap_or(0);
                let mname = type_names.get(&mid).cloned().unwrap_or_else(|| format!("Mat {}", mid));
                let mqty = mat.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0);
                let adjusted_qty = apply_me(mqty, me);

                materials.push(MaterialEntry {
                    typeid: mid,
                    name: mname,
                    quantity: adjusted_qty,
                });

                for hub in &["Jita", "Amarr", "Dodixie", "Rens"] {
                    let price = hub_prices.get(*hub).and_then(|h| h.get(&mid)).copied().unwrap_or(0.0);
                    let cost = mat_costs.get_mut(*hub).unwrap();
                    *cost += price * adjusted_qty as f64;
                }
            }
        }

        let mut product_prices: HashMap<String, f64> = HashMap::new();
        for hub in &["Jita", "Amarr", "Dodixie", "Rens"] {
            let price = hub_prices.get(*hub).and_then(|h| h.get(&product_id)).copied().unwrap_or(0.0);
            product_prices.insert(hub.to_string(), price);
        }

        bpo_list.push(BpoEntry {
            bp_name: name,
            product_name,
            product_id,
            product_qty,
            me,
            te,
            materials,
            mat_costs,
            product_prices,
        });
    }

    bpo_list.sort_by(|a, b| a.bp_name.cmp(&b.bp_name));

    Ok(BpoData {
        generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        character_name: character.name.clone(),
        bpo_count: bpo_list.len(),
        bpos: bpo_list,
    })
}