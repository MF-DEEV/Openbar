// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The few calls of Telegram's Bot API that phone approvals make (#214),
//! over Windows' own TLS without cookies or redirects. The bot's token is
//! part of every address, so neither the address nor the token is logged.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::bridge::{Api, Failure, Transport};

const DEFAULT_BASE: &str = "https://api.telegram.org";

/// A stand-in Bot API for tests, or Telegram's own.
fn base() -> String {
    let base = std::env::var("OPENBAR_TELEGRAM_API").unwrap_or_default();
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() { DEFAULT_BASE.to_owned() } else { base.to_owned() }
}

pub struct Telegram;

impl Api for Telegram {
    fn connect(&self, token: &str) -> Arc<dyn Transport> {
        Arc::new(Client {
            token: token.to_owned(),
            http: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("Openbar/", env!("CARGO_PKG_VERSION")))
                .timeout(None)
                .build()
                .ok(),
        })
    }
}

struct Client {
    token: String,
    http: Option<reqwest::blocking::Client>,
}

impl Transport for Client {
    fn call(&self, method: &str, body: &Value, timeout: Duration) -> Result<Value, Failure> {
        let http = self.http.as_ref().ok_or(Failure::Network)?;
        let response = http
            .post(format!("{}/bot{}/{method}", base(), self.token))
            .timeout(timeout)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .map_err(|_| Failure::Network)?;
        let status = response.status().as_u16();
        let text = response.text().map_err(|_| Failure::Network)?;
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if json["ok"].as_bool() == Some(true) {
            return Ok(json["result"].clone());
        }
        let code = json["error_code"].as_u64().map_or(status, |c| c as u16);
        Err(match code {
            // A malformed token answers Not Found.
            401 | 404 => Failure::Unauthorized,
            409 => Failure::Conflict,
            429 => Failure::TooMany(json["parameters"]["retry_after"].as_u64().unwrap_or(5).clamp(1, 600)),
            400..=499 => Failure::Refused(json["description"].as_str().unwrap_or("").to_owned()),
            _ => Failure::Network,
        })
    }
}
