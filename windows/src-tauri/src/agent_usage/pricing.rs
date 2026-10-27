// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! API list prices (AgentPricing.swift; spec 03 §6). The list ships with the
//! app and a newer published one replaces it; a list that breaks any rule is
//! refused as a whole and the last good one stays.

use super::model::{Billable, Plan};
use serde_json::Value;
use std::sync::{Arc, RwLock};

/// The list inside the app, the one the Mac ships.
pub const BUNDLED: &str = include_str!("../../../../Resources/agent-prices.json");
pub const MAXIMUM_SIZE: usize = 256 << 10;

#[derive(Clone, Debug, PartialEq)]
pub struct LongContext {
    pub above: i64,
    pub input: f64,
    pub output: f64,
}

/// US dollars per million tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub cache_write_long: f64,
    pub fast_multiplier: f64,
    pub long_context: Option<LongContext>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanEntry {
    pub matcher: String,
    pub name: String,
    pub monthly: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PriceList {
    /// "YYYY-MM-DD"; the newer of two lists wins (ISO days compare as text).
    pub updated: String,
    pub claude: Vec<(String, Price)>,
    pub codex: Vec<(String, Price)>,
    pub claude_plans: Vec<PlanEntry>,
    pub codex_plans: Vec<PlanEntry>,
    pub web_search: f64,
    pub us_only_multiplier: f64,
}

impl PriceList {
    pub fn empty() -> PriceList {
        PriceList { updated: String::new(), claude: vec![], codex: vec![], claude_plans: vec![], codex_plans: vec![], web_search: 0.0, us_only_multiplier: 1.0 }
    }

    pub fn decode(bytes: &[u8]) -> Option<PriceList> {
        if bytes.len() > MAXIMUM_SIZE {
            return None;
        }
        let json: Value = serde_json::from_slice(bytes).ok()?;
        if number(json.get("schema"), 1.0, 1000.0)? != 1.0 {
            return None;
        }
        let updated = json.get("updated")?.as_str().filter(|d| valid_day(d))?.to_string();
        let claude = json.get("claude")?;
        let codex = json.get("codex")?;
        Some(PriceList {
            updated,
            claude: models(claude.get("models"), true)?,
            codex: models(codex.get("models"), false)?,
            claude_plans: plans(claude.get("plans"))?,
            codex_plans: plans(codex.get("plans"))?,
            web_search: number(claude.get("webSearch"), 0.0, 1.0)?,
            us_only_multiplier: number(claude.get("usOnlyMultiplier"), 1.0, 3.0)?,
        })
    }

    /// The newer of the shipped and downloaded lists; on the same day the download wins.
    pub fn newer(shipped: Option<PriceList>, downloaded: Option<PriceList>) -> Option<PriceList> {
        match (shipped, downloaded) {
            (shipped, None) => shipped,
            (None, downloaded) => downloaded,
            (Some(s), Some(d)) => Some(if d.updated >= s.updated { d } else { s }),
        }
    }
}

fn models(value: Option<&Value>, claude: bool) -> Option<Vec<(String, Price)>> {
    let entries = value?.as_array()?;
    if !(1..=500).contains(&entries.len()) {
        return None;
    }
    let mut out: Vec<(String, Price)> = Vec::new();
    for entry in entries {
        let id = entry.get("id")?.as_str()?;
        if !identifier(id, 64) || out.iter().any(|(seen, _)| seen == id) || id.starts_with("claude-") != claude {
            return None;
        }
        let input = number(entry.get("input"), 0.0, 1000.0)?;
        let output = number(entry.get("output"), 0.0, 1000.0)?;
        let cache_read = number(entry.get("cacheRead"), 0.0, 1000.0)?;
        // Fields left out fall back to plain input pricing.
        let cache_write = if entry.get("cacheWrite").is_none() { input } else { number(entry.get("cacheWrite"), 0.0, 1000.0)? };
        let cache_write_long = if entry.get("cacheWriteLong").is_none() { cache_write } else { number(entry.get("cacheWriteLong"), 0.0, 1000.0)? };
        let fast_multiplier = if entry.get("fastMultiplier").is_none() { 1.0 } else { number(entry.get("fastMultiplier"), 1.0, 10.0)? };
        let long_context = match entry.get("longContext") {
            None => None,
            Some(object) => Some(LongContext {
                above: number(object.get("above"), 1000.0, 10_000_000.0)? as i64,
                input: number(object.get("inputMultiplier"), 1.0, 10.0)?,
                output: number(object.get("outputMultiplier"), 1.0, 10.0)?,
            }),
        };
        out.push((id.to_string(), Price { input, output, cache_read, cache_write, cache_write_long, fast_multiplier, long_context }));
    }
    Some(out)
}

fn plans(value: Option<&Value>) -> Option<Vec<PlanEntry>> {
    let entries = value?.as_array()?;
    if entries.len() > 50 {
        return None;
    }
    let mut out = Vec::new();
    for entry in entries {
        let matcher = entry.get("match")?.as_str().filter(|m| identifier(m, 40))?;
        let name = entry.get("name")?.as_str()?.trim();
        if !(1..=24).contains(&name.chars().count()) {
            return None;
        }
        let monthly = match entry.get("monthly") {
            None => None,
            Some(_) => Some(number(entry.get("monthly"), 0.0, 100_000.0)?),
        };
        out.push(PlanEntry { matcher: matcher.into(), name: name.into(), monthly });
    }
    Some(out)
}

/// Lowercase letters, digits, dots, dashes and underscores.
fn identifier(text: &str, length: usize) -> bool {
    (1..=length).contains(&text.chars().count()) && text.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

/// A finite number inside the range; true or false is not one.
fn number(value: Option<&Value>, low: f64, high: f64) -> Option<f64> {
    let n = value?.as_f64()?;
    (n.is_finite() && n >= low && n <= high).then_some(n)
}

fn valid_day(text: &str) -> bool {
    let parts: Vec<_> = text.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) else { return false };
    (2024..=2100).contains(&y) && chrono::NaiveDate::from_ymd_opt(y, m, d).is_some()
}

// MARK: The installed list

fn installed() -> &'static RwLock<Arc<PriceList>> {
    static LIST: std::sync::OnceLock<RwLock<Arc<PriceList>>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| RwLock::new(Arc::new(PriceList::decode(BUNDLED.as_bytes()).unwrap_or_else(PriceList::empty))))
}

pub fn list() -> Arc<PriceList> {
    installed().read().unwrap().clone()
}

/// Makes `list` the one prices come from; false when nothing changes.
pub fn install(list: PriceList) -> bool {
    let mut current = installed().write().unwrap();
    if **current == list {
        return false;
    }
    *current = Arc::new(list);
    true
}

// MARK: Matching (§6.3)

/// Provider prefixes, dates and context tags vary by where the model runs;
/// the family name in the middle does not.
pub fn normalized(model: &str) -> String {
    let mut id = model.trim().to_lowercase();
    if let Some(at) = id.find("claude-") {
        id = id[at..].to_string();
    }
    // Copilot names Claude point releases with a dot.
    if id.starts_with("claude-") {
        id = id.replace('.', "-");
    }
    if let Some(slash) = id.rfind('/') {
        id = id[slash + 1..].to_string();
    }
    for marker in ['@', '['] {
        if let Some(at) = id.find(marker) {
            id.truncate(at);
        }
    }
    id
}

const SIBLINGS: [&str; 12] = ["mini", "nano", "pro", "lite", "research", "search", "audio", "realtime", "transcribe", "tts", "cyber", "image"];

/// A family matches at a word boundary, and a number after it must be a date.
fn matches(id: &str, family: &str) -> bool {
    let Some(rest) = id.strip_prefix(family) else { return false };
    let mut chars = rest.chars();
    let Some(next) = chars.next() else { return true };
    if !matches!(next, '-' | '_' | ':') {
        return false;
    }
    let digits = chars.take_while(|c| c.is_ascii_digit()).count();
    digits == 0 || digits >= 4
}

pub fn price<'a>(model: &str, list: &'a PriceList) -> Option<&'a Price> {
    let id = normalized(model);
    let table = if id.starts_with("claude-") { &list.claude } else { &list.codex };
    let (family, price) = table.iter().filter(|(family, _)| matches(&id, family)).max_by_key(|(family, _)| family.len())?;
    // A sibling the list does not name is priced very differently: no figure.
    let rest = &id[family.len()..];
    if SIBLINGS.iter().any(|s| rest.contains(s)) {
        return None;
    }
    Some(price)
}

/// (cost, savings) at API list prices (§6.4).
pub fn cost_in(billable: &Billable, model: &str, list: &PriceList) -> (Option<f64>, f64) {
    if billable.aggregate && billable.tokens.total() == 0 {
        return (Some(0.0), 0.0);
    }
    let Some(price) = price(model, list) else { return (None, 0.0) };
    let t = &billable.tokens;
    let long = billable.long_cache_write.max(0).min(t.cache_write);
    let mut multiplier = if billable.fast { price.fast_multiplier } else { 1.0 };
    if billable.domestic {
        multiplier *= list.us_only_multiplier;
    }
    let (mut input_rate, mut output_rate) = (multiplier, multiplier);
    if let Some(lc) = price.long_context.as_ref().filter(|lc| !billable.aggregate && t.prompt() > lc.above) {
        input_rate *= lc.input;
        output_rate *= lc.output;
    }
    let cost = (t.input as f64 * price.input * input_rate
        + (t.cache_write - long) as f64 * price.cache_write * input_rate
        + long as f64 * price.cache_write_long * input_rate
        + t.cache_read as f64 * price.cache_read * input_rate
        + t.output as f64 * price.output * output_rate)
        / 1_000_000.0
        + billable.web_searches.max(0) as f64 * list.web_search;
    let savings = t.cache_read as f64 * (price.input - price.cache_read).max(0.0) * input_rate / 1_000_000.0;
    (Some(cost), savings)
}

pub fn cost(billable: &Billable, model: &str) -> (Option<f64>, f64) {
    cost_in(billable, model, &list())
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// "claude-opus-5-5" → "Opus 5.5", "gpt-6-astra" → "GPT-6 Astra" (§6.5).
pub fn display_name(model: &str) -> String {
    let id = normalized(model);
    if id.is_empty() {
        return String::new();
    }
    let numeric = |p: &str| p.chars().all(|c| c.is_ascii_digit() || c == '.');
    let date = |p: &str| p.len() >= 6 && p.chars().all(|c| c.is_ascii_digit());
    if let Some(rest) = id.strip_prefix("claude-") {
        let family = rest.split(':').next().unwrap_or(rest);
        let parts: Vec<&str> = family
            .split('-')
            .filter(|p| !p.is_empty() && !date(p) && *p != "latest")
            .filter(|p| !(p.starts_with('v') && p[1..].chars().next().is_some_and(|c| c.is_ascii_digit())))
            .collect();
        let words: Vec<&str> = parts.iter().copied().filter(|p| !numeric(p)).collect();
        let version = parts.iter().copied().filter(|p| numeric(p)).collect::<Vec<_>>().join(".");
        let mut name = vec![words.first().map(|w| capitalized(w)).unwrap_or_default(), version];
        name.extend(words.iter().skip(1).map(|w| capitalized(w)));
        return name.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    }
    let Some(rest) = id.strip_prefix("gpt-") else { return id };
    let mut parts: Vec<&str> = rest.split('-').filter(|p| !p.is_empty() && *p != "latest" && !date(p)).collect();
    if parts.len() >= 4 {
        let n = parts.len();
        let year = parts[n - 3].parse::<i32>().is_ok_and(|y| (2000..=2100).contains(&y));
        if year && parts[n - 2..].iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_digit())) {
            parts.truncate(n - 3);
        }
    }
    let Some(first) = parts.first() else { return "GPT".into() };
    let head = if first.starts_with(|c: char| c.is_ascii_digit()) { format!("GPT-{first}") } else { format!("GPT {}", capitalized(first)) };
    std::iter::once(head).chain(parts.iter().skip(1).map(|p| capitalized(p))).collect::<Vec<_>>().join(" ")
}

// MARK: Plans

/// Matched by substring of "<rate limit tier> <organization type>", in list order.
pub fn claude_plan(organization_type: Option<&str>, rate_limit_tier: Option<&str>, list: &PriceList) -> Option<Plan> {
    let kind = organization_type.unwrap_or("").to_lowercase();
    let recorded = format!("{} {}", rate_limit_tier.unwrap_or("").to_lowercase(), kind);
    if let Some(plan) = list.claude_plans.iter().find(|p| recorded.contains(&p.matcher)) {
        return Some(Plan { name: plan.name.clone(), monthly_price: plan.monthly });
    }
    let name = kind.replace("claude_", "").replace('_', " ");
    let name = name.trim();
    (!name.is_empty()).then(|| Plan { name: name.split(' ').map(capitalized).collect::<Vec<_>>().join(" "), monthly_price: None })
}

/// Exact match on the plan type, else the type capitalized.
pub fn codex_plan(plan_type: Option<&str>, list: &PriceList) -> Option<Plan> {
    let raw = plan_type?.trim().to_lowercase();
    if raw.is_empty() {
        return None;
    }
    if let Some(plan) = list.codex_plans.iter().find(|p| p.matcher == raw) {
        return Some(Plan { name: plan.name.clone(), monthly_price: plan.monthly });
    }
    let mut chars = raw.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    Some(Plan { name: first + chars.as_str(), monthly_price: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_usage::model::Tokens;

    fn bundled() -> PriceList {
        PriceList::decode(BUNDLED.as_bytes()).expect("the shipped list is valid")
    }

    #[test]
    fn the_shipped_list_decodes_and_a_broken_one_is_refused_whole() {
        let list = bundled();
        assert_eq!(list.updated, "2026-10-09");
        assert!(list.claude.len() >= 20 && list.codex.len() >= 50);
        let mut json: Value = serde_json::from_str(BUNDLED).unwrap();
        json["claude"]["models"][0]["input"] = Value::from(5000);
        assert!(PriceList::decode(json.to_string().as_bytes()).is_none());
        let mut json: Value = serde_json::from_str(BUNDLED).unwrap();
        json["codex"]["models"][0]["id"] = Value::from("claude-x");
        assert!(PriceList::decode(json.to_string().as_bytes()).is_none());
        let mut json: Value = serde_json::from_str(BUNDLED).unwrap();
        json["schema"] = Value::from(2);
        assert!(PriceList::decode(json.to_string().as_bytes()).is_none());
        let mut json: Value = serde_json::from_str(BUNDLED).unwrap();
        json["updated"] = Value::from("2026-02-30");
        assert!(PriceList::decode(json.to_string().as_bytes()).is_none());
    }

    #[test]
    fn the_newer_list_wins_and_a_download_wins_a_tie() {
        let mut shipped = bundled();
        let mut downloaded = bundled();
        downloaded.web_search = 0.5;
        assert_eq!(PriceList::newer(Some(shipped.clone()), Some(downloaded.clone())).unwrap().web_search, 0.5);
        shipped.updated = "2026-12-01".into();
        assert_eq!(PriceList::newer(Some(shipped), Some(downloaded)).unwrap().updated, "2026-12-01");
    }

    #[test]
    fn models_match_their_family_at_a_word_boundary() {
        let list = bundled();
        assert_eq!(normalized(" anthropic/Claude-Sonnet-4.5@20250929 "), "claude-sonnet-4-5");
        assert_eq!(normalized("openai/gpt-5.6[1m]"), "gpt-5.6");
        assert!(price("claude-opus-5-5", &list).is_some());
        assert!(price("claude-opus-5-5-20261001", &list).is_some(), "a dated snapshot is the same model");
        assert!(price("claude-sonnet-4.5", &list).is_some(), "Copilot's dotted name");
        assert!(price("claude-opus-5-9", &list).is_none(), "a later version is not its family");
        assert!(price("gpt-5.6-mini", &list).is_none() || list.codex.iter().any(|(id, _)| id == "gpt-5.6-mini"));
        assert!(price("gemini-3-pro", &list).is_none());
    }

    #[test]
    fn cost_follows_the_formula() {
        let mut list = PriceList::empty();
        list.web_search = 0.01;
        list.us_only_multiplier = 1.1;
        list.claude.push(("claude-test-1".into(), Price { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75, cache_write_long: 6.0, fast_multiplier: 2.0, long_context: Some(LongContext { above: 200_000, input: 2.0, output: 1.5 }) }));
        let tokens = Tokens { input: 1_000_000, cache_write: 1_000_000, cache_read: 1_000_000, output: 1_000_000, reasoning: 0 };
        let b = Billable { tokens, long_cache_write: 500_000, ..Default::default() };
        let (cost, savings) = cost_in(&b, "claude-test-1", &list);
        // Long context applies: 3M prompt > 200K.
        let expected = 3.0 * 2.0 + 0.5 * 3.75 * 2.0 + 0.5 * 6.0 * 2.0 + 0.3 * 2.0 + 15.0 * 1.5;
        assert!((cost.unwrap() - expected).abs() < 1e-9);
        assert!((savings - 2.7 * 2.0).abs() < 1e-9);
        let small = Billable { tokens: Tokens { input: 1000, output: 1000, ..Default::default() }, fast: true, domestic: true, web_searches: 2, ..Default::default() };
        let (cost, _) = cost_in(&small, "claude-test-1", &list);
        let expected = (1000.0 * 3.0 + 1000.0 * 15.0) * 2.0 * 1.1 / 1e6 + 0.02;
        assert!((cost.unwrap() - expected).abs() < 1e-12);
        // Aggregates never take the long-context premium; empty ones cost nothing.
        let aggregate = Billable { tokens, aggregate: true, ..Default::default() };
        let (cost, _) = cost_in(&aggregate, "claude-test-1", &list);
        assert!((cost.unwrap() - (3.0 + 3.75 + 0.3 + 15.0)).abs() < 1e-9);
        assert_eq!(cost_in(&Billable { aggregate: true, ..Default::default() }, "unknown", &list).0, Some(0.0));
        assert_eq!(cost_in(&b, "unknown-model", &list).0, None);
    }

    #[test]
    fn display_names_read_well() {
        assert_eq!(display_name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(display_name("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
        assert_eq!(display_name("claude-3-5-haiku-latest"), "Haiku 3.5");
        assert_eq!(display_name("gpt-6-astra"), "GPT-6 Astra");
        assert_eq!(display_name("gpt-5.6-2026-10-01"), "GPT-5.6");
        assert_eq!(display_name("gpt-oss-120b"), "GPT Oss 120b");
        assert_eq!(display_name("gemini-3-pro"), "gemini-3-pro");
    }

    #[test]
    fn plans_match_by_substring_or_read_their_own_name() {
        let list = bundled();
        let max = claude_plan(Some("claude_max"), Some("default_claude_max_20x"), &list).unwrap();
        assert_eq!(max.monthly_price, Some(200.0));
        assert_eq!(claude_plan(Some("claude_galaxy_tier"), None, &list).unwrap().name, "Galaxy Tier");
        assert_eq!(claude_plan(None, None, &list), None);
        assert_eq!(codex_plan(Some("plus"), &list).unwrap().monthly_price, Some(20.0));
        assert_eq!(codex_plan(Some("ultra"), &list).unwrap().name, "Ultra");
    }
}
