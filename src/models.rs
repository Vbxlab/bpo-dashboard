use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── EVE SSO ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoConfig {
    pub client_id: String,
    pub client_secret: String,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<f64>,
    pub character_id: i64,
    pub character_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: i64,
    pub name: String,
    pub sso: SsoConfig,
    pub tokens: TokenData,
}

// No built-in SSO credentials — all configured via config file or environment variables.

// ─── App Config ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub data_dir: String,
    /// SSO credentials for new character sign-ups.
    /// When present, the "Add character" button works in one click.
    /// Priority: config file > built-in (BUILTIN_SSO_CLIENT_ID + BPO_SSO_SECRET env) > None
    #[serde(default)]
    pub default_sso: Option<SsoConfig>,
    #[serde(default)]
    pub characters: Vec<Character>,
}

impl AppConfig {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut config: AppConfig = serde_json::from_str(&content)?;
        config.resolve_default_sso();
        Ok(config)
    }

    /// Resolve default_sso from config file only.
    fn resolve_default_sso(&mut self) {
        // No environment variables, no built-in credentials.
        // Users must configure default_sso in config.json with their own EVE app credentials.
    }

    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn default_path() -> String {
        // Use ./data/config.json relative to current directory (portable)
        "./data/config.json".to_string()
    }

    pub fn default_config() -> Self {
        Self {
            port: 8090,
            data_dir: "./data".to_string(),
            default_sso: None,
            characters: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn sso_for_character(&self, char_id: i64) -> Option<SsoConfig> {
        self.characters.iter()
            .find(|c| c.id == char_id)
            .map(|c| c.sso.clone())
            .or_else(|| self.default_sso.clone())
    }
}

// ─── BPO Data ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BpoEntry {
    pub bp_name: String,
    pub product_name: String,
    pub product_id: i64,
    pub product_qty: i64,
    pub me: i32,
    pub te: i32,
    pub materials: Vec<MaterialEntry>,
    pub mat_costs: HashMap<String, f64>,
    pub product_prices: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialEntry {
    pub typeid: i64,
    pub name: String,
    pub quantity: i64,
}

impl BpoEntry {
    pub fn profit(&self, hub: &str) -> f64 {
        let revenue = self.product_prices.get(hub).copied().unwrap_or(0.0) * self.product_qty as f64;
        let cost = self.mat_costs.get(hub).copied().unwrap_or(0.0);
        revenue - cost
    }

    pub fn margin_percent(&self, hub: &str) -> f64 {
        let revenue = self.product_prices.get(hub).copied().unwrap_or(0.0) * self.product_qty as f64;
        if revenue == 0.0 { return 0.0; }
        let cost = self.mat_costs.get(hub).copied().unwrap_or(0.0);
        (revenue - cost) / revenue * 100.0
    }

    pub fn best_hub(&self) -> (&str, f64) {
        let hubs = ["Jita", "Amarr", "Dodixie", "Rens"];
        let mut best = ("Jita", self.profit("Jita"));
        for hub in &hubs[1..] {
            let p = self.profit(hub);
            if p > best.1 { best = (hub, p); }
        }
        best
    }

    pub fn needs_me_improvement(&self) -> bool { self.me < 10 }
    pub fn needs_te_improvement(&self) -> bool { self.te < 20 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BpoData {
    pub generated_at: String,
    pub character_name: String,
    pub bpo_count: usize,
    pub bpos: Vec<BpoEntry>,
}

// ─── Summary ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DashboardSummary {
    pub total_bpos: usize,
    pub profitable_per_hub: HashMap<String, usize>,
    pub top_profits: Vec<BpoEntry>,
    pub worst_losses: Vec<BpoEntry>,
    pub total_investment: HashMap<String, f64>,
    pub total_revenue: HashMap<String, f64>,
    pub needs_me_improvement: usize,
    pub needs_te_improvement: usize,
    pub needs_both: usize,
}

impl DashboardSummary {
    pub fn from_bpos(bpos: &[BpoEntry]) -> Self {
        let hubs = ["Jita", "Amarr", "Dodixie", "Rens"];
        let mut profitable_per_hub = HashMap::new();
        let mut total_investment = HashMap::new();
        let mut total_revenue = HashMap::new();

        for hub in &hubs {
            let profitable = bpos.iter().filter(|b| b.profit(hub) > 0.0).count();
            profitable_per_hub.insert(hub.to_string(), profitable);
            let invest: f64 = bpos.iter().map(|b| b.mat_costs.get(*hub).copied().unwrap_or(0.0)).sum();
            let revenue: f64 = bpos.iter()
                .map(|b| b.product_prices.get(*hub).copied().unwrap_or(0.0) * b.product_qty as f64)
                .sum();
            total_investment.insert(hub.to_string(), invest);
            total_revenue.insert(hub.to_string(), revenue);
        }

        let mut sorted: Vec<_> = bpos.to_vec();
        sorted.sort_by(|a, b| b.profit("Dodixie").partial_cmp(&a.profit("Dodixie")).unwrap_or(std::cmp::Ordering::Equal));

        let top_profits: Vec<BpoEntry> = sorted.iter().filter(|b| b.profit("Dodixie") > 0.0).take(5).cloned().collect();
        let worst_losses: Vec<BpoEntry> = sorted.iter().filter(|b| b.profit("Dodixie") < 0.0).rev().take(5).cloned().collect();

        let needs_me = bpos.iter().filter(|b| b.needs_me_improvement()).count();
        let needs_te = bpos.iter().filter(|b| b.needs_te_improvement()).count();
        let needs_both = bpos.iter().filter(|b| b.needs_me_improvement() && b.needs_te_improvement()).count();

        DashboardSummary {
            total_bpos: bpos.len(),
            profitable_per_hub,
            top_profits,
            worst_losses,
            total_investment,
            total_revenue,
            needs_me_improvement: needs_me,
            needs_te_improvement: needs_te,
            needs_both,
        }
    }
}

// ─── Material Summary ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MaterialSummary {
    pub name: String,
    pub type_id: i64,
    pub total_quantity: i64,
    pub unit_price_jita: f64,
    pub total_cost_jita: f64,
}

impl MaterialSummary {
    pub fn from_bpos(bpos: &[BpoEntry]) -> Vec<Self> {
        let mut mats: HashMap<i64, (String, i64)> = HashMap::new();
        for bpo in bpos {
            for mat in &bpo.materials {
                let entry = mats.entry(mat.typeid).or_insert((mat.name.clone(), 0));
                entry.1 += mat.quantity * bpo.product_qty;
            }
        }

        let mut result: Vec<Self> = mats.into_iter().map(|(tid, (name, qty))| {
            Self {
                name,
                type_id: tid,
                total_quantity: qty,
                unit_price_jita: 0.0,
                total_cost_jita: 0.0,
            }
        }).collect();

        result.sort_by(|a, b| b.total_cost_jita.partial_cmp(&a.total_cost_jita).unwrap_or(std::cmp::Ordering::Equal));
        result
    }
}