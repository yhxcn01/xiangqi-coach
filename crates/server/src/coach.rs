//! 服务端 AI 讲解层：Settings 持久化 + OpenAI 兼容 LLM 调用。
//! 文案与上下文构造在 xqcore::coach；未配置 Key 或调用失败时自动降级为模板提示。

use serde::{Deserialize, Serialize};
use xqcore::coach::{self as text, OpponentPieceCtx, PieceCtx, PositionCtx};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_base_url() -> String {
    "https://open.bigmodel.cn/api/paas/v4".to_string()
}
fn default_model() -> String {
    "glm-4.7-flash".to_string()
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string("settings.json")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Settings {
                base_url: default_base_url(),
                api_key: String::new(),
                model: default_model(),
            })
    }
    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write("settings.json", s);
        }
    }
    pub fn has_ai(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

/// 调用 OpenAI 兼容的 chat/completions 接口（智谱 GLM、DeepSeek 等）。
pub async fn call_llm(settings: &Settings, user_prompt: &str) -> Result<String, String> {
    if !settings.has_ai() {
        return Err("未配置 API Key".to_string());
    }
    let url = format!("{}/chat/completions", settings.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            {"role": "system", "content": text::SYSTEM_PROMPT},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 1200
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .bearer_auth(settings.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("网络错误：{}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("AI 服务返回 {}：{}", status, &text[..text.len().min(200)]));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "AI 返回内容为空".to_string())
}

pub async fn explain_position(settings: &Settings, ctx: &PositionCtx) -> (String, &'static str) {
    let prompt = text::build_position_prompt(ctx);
    match call_llm(settings, &prompt).await {
        Ok(t) => (t, "ai"),
        Err(_) => (text::fallback_position(ctx), "fallback"),
    }
}

pub async fn explain_piece(settings: &Settings, ctx: &PieceCtx) -> (String, &'static str) {
    let prompt = text::build_piece_prompt(ctx);
    match call_llm(settings, &prompt).await {
        Ok(t) => (t, "ai"),
        Err(_) => (text::fallback_piece(ctx), "fallback"),
    }
}

pub async fn explain_opponent_piece(settings: &Settings, ctx: &OpponentPieceCtx) -> (String, &'static str) {
    let prompt = text::build_opponent_piece_prompt(ctx);
    match call_llm(settings, &prompt).await {
        Ok(t) => (t, "ai"),
        Err(_) => (text::fallback_opponent_piece(ctx), "fallback"),
    }
}
