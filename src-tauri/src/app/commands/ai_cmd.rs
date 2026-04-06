use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use serde::{Deserialize, Serialize};
use tauri::{command, AppHandle, Emitter, Manager};

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_budget: Option<i32>,
    max_tokens: i32,
    temperature: f32,
    presence_penalty: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    #[serde(flatten)]
    _other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    message: Option<ChatMessage>,
    #[serde(flatten)]
    _other: serde_json::Value,
}

fn clean_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn apply_optional_http_referer(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Ok(referer) = std::env::var("AI_HTTP_REFERER") {
        let trimmed = referer.trim();
        if !trimmed.is_empty() {
            return request.header("HTTP-Referer", trimmed.to_string());
        }
    }
    request
}

fn sanitize_input(content: &str) -> AppResult<String> {
    // Note: We used to have an injection check here, but it's too aggressive for a general purpose clipboard
    // where users might be copying technical text about AI. The limit is increased to 10000 characters.
    Ok(content.chars().take(10000).collect())
}

fn build_system_prompt(action_type: &str) -> String {
    // 1. 社交嘴替 (Mouthpiece) - 纯回复与防御
    let mouth_persona = "你不是聊天机器人，你是用户的社交嘴替与语言防御系统。\n\
        你的任务只有一个：替用户回复对方说的话。不要生成评价、摘要、文案或任何额外任务内容。\n\n\
        【核心定位】\n\
        - 外在：情绪稳定、语气自然、不咄咄逼人\n\
        - 内核：边界清晰、逻辑自洽、不吃亏\n\
        - 风格：简短有力，话少但句句有落点\n\n\
        【判断顺序（必须遵守）】\n\
        1. 是否在索取资源/情绪/责任\n\
        2. 是否在贬低、否定、试探或施压\n\
        3. 是否存在身份或权力不对等\n\
        4. 是否需要反击，还是只需止损封口\n\n\
        【输出策略】\n\
        - 优先一到两句话解决，避免解释和说教\n\
        - 不讨好、不示弱、不留口子\n\
        - 对方语气越重，你的语气越稳\n\
        - 若信息不足，可用一句话澄清关键点\n\n\
        【语言规则】\n\
        - 日常口语但不低级\n\
        - 不使用 Emoji\n\
        - 禁止在输出前后加引号或括号\n\
        - 不输出任何分析、思考、理由或过程\n\n\
        【目标】\n\
        - 面对攻击或压力：让对方自然停止继续进攻。";

    // 2. 任务解决助手 (Task Solver) - 通用任务执行
    let task_persona =
        "你是一个专注于“任务解决”的智能助手。你的核心目标是高效、精准、专业地完成用户的需求。\n\n\
        【你的核心原则】\n\
        1. **结果导向**：直接提供用户需要的内容，不要废话，不要过度寒暄。\n\
        2. **逻辑严密**：确保生成的内容结构清晰、论证有力。\n\
        3. **专业可靠**：保持客观、专业的语气，除非用户指定了特定风格。";

    // 3. 翻译专家 (Translator)
    let translator_persona = "你现在的具体任务是【顶级同传】：请将内容翻译成目标语言，确保译文地道且富有生活化语气，严禁机翻感。只输出译文，不要哪怕一句多余的解释。";

    match action_type {
        "mouthpiece" => mouth_persona.to_string(),
        "task" => task_persona.to_string(),
        "translate" => translator_persona.to_string(),
        _ => mouth_persona.to_string(),
    }
}

fn build_user_prompt(action_type: &str, content: &str, target_lang: &str) -> String {
    let target_lang_name = match target_lang {
        "en" => "English",
        "ja" => "Japanese",
        "de" => "German",
        "fr" => "French",
        _ => "Chinese",
    };

    match action_type {
        "task" => content.to_string(),
        "mouthpiece" => format!("这是对方的内容或消息：\n\n“{}”\n\n请只针对这句话进行回复（嘴替模式），不要做其他任务。", content),
        "translate" => format!("请将以下内容翻译为 {}：\n\n{}", target_lang_name, content),
        _ => content.to_string(),
    }
}

#[command]
pub async fn check_ai_connectivity(
    base_url: String,
    api_key: String,
    model: String,
) -> AppResult<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Client creation failed: {}", e))?;
    let cleaned_base = clean_url(&base_url);
    let api_url = format!("{}/chat/completions", cleaned_base);

    // Perform a real minimal handshake
    let request_body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1
    });

    let request = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("X-Title", "TieZ Clipboard");

    let response = apply_optional_http_referer(request)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Network Error: {}", e))?;

    let status = response.status();
    if status.is_success() {
        Ok("success".to_string())
    } else {
        let err_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("HTTP {} - {}", status, err_text).into())
    }
}

#[command]
pub async fn call_ai(
    app_handle: AppHandle,
    id: i64,
    mut content: String,
    action_type: String, // "task", "mouthpiece", "translate"
) -> AppResult<String> {
    let state = app_handle.state::<DbState>();

    // 0. Resolve full content if ID is provided and content is placeholder/truncated
    if id != 0 {
        if id > 0 {
            // Fetch from Database
            if let Ok(Some(full_content)) = state.repo.get_entry_content(id) {
                content = full_content;
            }
        } else {
            // Fetch from Session
            let session = app_handle.state::<crate::app_state::SessionHistory>();
            let session_items = session.0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
            }
        }
    }

    let content = sanitize_input(&content)?;

    let (api_key, base_url, model, target_lang, persistent, enable_thinking, thinking_budget) = {
        // 1. Get functional assignment
        let assigned_profile_key = format!("ai_assigned_profile_{}", action_type);
        let assigned_id = state
            .settings_repo
            .get(&assigned_profile_key)
            .unwrap_or(None)
            .unwrap_or_else(|| "default".to_string());

        // 2. Get Profiles Library
        let profiles_json = state
            .settings_repo
            .get("ai_profiles")
            .unwrap_or(None)
            .unwrap_or_else(|| "[]".to_string());

        let profiles: serde_json::Value =
            serde_json::from_str(&profiles_json).unwrap_or(serde_json::json!([]));

        // 3. Find assigned profile
        let profile = profiles
            .as_array()
            .and_then(|arr| arr.iter().find(|p| p["id"].as_str() == Some(&assigned_id)))
            .cloned();

        let (key, url, mdl, thinking) = if let Some(p) = profile {
            (
                p["apiKey"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                clean_url(
                    p["baseUrl"]
                        .as_str()
                        .unwrap_or("https://api.longcat.chat/openai/v1"),
                ),
                p["model"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "LongCat-Flash-Chat".to_string()),
                p["enableThinking"].as_bool().unwrap_or(false),
            )
        } else {
            // If the assigned profile doesn't exist, try to use the first available profile
            if let Some(p) = profiles.as_array().and_then(|arr| arr.get(0)) {
                (
                    p["apiKey"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    clean_url(
                        p["baseUrl"]
                            .as_str()
                            .unwrap_or("https://api.longcat.chat/openai/v1"),
                    ),
                    p["model"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "LongCat-Flash-Chat".to_string()),
                    p["enableThinking"].as_bool().unwrap_or(false),
                )
            } else {
                (
                    "".to_string(),
                    "https://api.longcat.chat/openai/v1".to_string(),
                    "".to_string(),
                    false,
                )
            }
        };

        let lang = state
            .settings_repo
            .get("ai_target_lang")
            .unwrap_or(None)
            .unwrap_or_else(|| "zh".to_string());
        let persistent = state
            .settings_repo
            .get("app.persistent")
            .unwrap_or(None)
            .unwrap_or_else(|| "true".to_string())
            == "true";
        let thinking_budget = state
            .settings_repo
            .get("ai_thinking_budget")
            .unwrap_or(None)
            .unwrap_or_else(|| "1024".to_string())
            .parse::<i32>()
            .map(|v| std::cmp::max(v, 1024)) // API requires minimum 1024
            .unwrap_or(1024);

        (key, url, mdl, lang, persistent, thinking, thinking_budget)
    };

    if api_key.is_empty() {
        return Err(AppError::Validation(
            "AI API Key is not set in settings.".to_string(),
        ));
    }

    // Respect User Choice: Remove the forced override for thinking.
    // If user enabled it in the profile, we send it to the API.
    let effective_thinking = enable_thinking;

    // Handle Auto Detect for Translation
    let effective_target_lang = if action_type == "translate" && target_lang == "auto_zh_en" {
        // Simple heuristic: check if content contains any CJK unified ideographs
        let has_chinese = content.chars().any(
            |c| {
                (c >= '\u{4E00}' && c <= '\u{9FFF}') || // CJK Unified Ideographs
            (c >= '\u{3400}' && c <= '\u{4DBF}')
            }, // CJK Unified Ideographs Extension A
        );
        if has_chinese {
            "en".to_string()
        } else {
            "zh".to_string()
        }
    } else {
        target_lang
    };

    let system_prompt = build_system_prompt(&action_type);
    let user_prompt = build_user_prompt(&action_type, &content, &effective_target_lang);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Client creation failed: {}", e))?;
    let api_url = format!("{}/chat/completions", base_url);

    // Ensure max_tokens is greater than thinking_budget when thinking is enabled
    // This prevents the thinking process from consuming all tokens, leaving no room for actual output
    // API requires: max_tokens > thinking_budget
    let base_max_tokens = 2000;
    let final_max_tokens = if effective_thinking {
        // Add large buffer: max_tokens should be at least thinking_budget + 1500 (for actual response)
        std::cmp::max(base_max_tokens, thinking_budget + 1500)
    } else {
        base_max_tokens
    };

    // Dynamic presence penalty
    let presence_penalty = if action_type == "mouthpiece" {
        0.6
    } else {
        0.0 // Task/Translate usually needs accuracy, not variety
    };

    let request_body = ChatCompletionRequest {
        model,
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt),
                reasoning_content: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(user_prompt),
                reasoning_content: None,
            },
        ],
        enable_thinking: if effective_thinking { Some(true) } else { None },
        thinking_budget: if effective_thinking {
            Some(thinking_budget)
        } else {
            None
        },
        max_tokens: final_max_tokens,
        temperature: 0.7,
        presence_penalty: presence_penalty,
    };

    let request = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("X-Title", "TieZ Clipboard");

    let response = apply_optional_http_referer(request)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let err_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("AI API error: {} - {}", api_url, err_text).into());
    }

    let response_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let completion: ChatCompletionResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse response: {} | Body: {}", e, response_text))?;

    let choice = completion.choices.first();

    // Log reasoning content if available for debugging
    if let Some(c) = choice {
        if let Some(msg) = &c.message {
            if let Some(reasoning) = &msg.reasoning_content {
                println!("[AI Reasoning] {}", reasoning);
            }
        }
    }

    let raw_response = choice
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.content.clone())
        .unwrap_or_default();

    let mut ai_response = raw_response.trim().to_string();

    // Safety check for empty results early
    if ai_response.is_empty() {
        return Err(format!("AI returned an empty response. Raw Body: {}", response_text).into());
    }

    // UTF-8 Safe Quote Stripping - ONLY for Mouthpiece
    if action_type == "mouthpiece" {
        let has_quotes = {
            let chars: Vec<char> = ai_response.chars().collect();
            if chars.len() >= 2 {
                let first = chars[0];
                let last = chars[chars.len() - 1];
                (first == '"' && last == '"')
                    || (first == '\'' && last == '\'')
                    || (first == '“' && last == '”')
            } else {
                false
            }
        };

        if has_quotes {
            let mut chars = ai_response.chars();
            chars.next(); // remove first
            chars.next_back(); // remove last
            ai_response = chars.collect::<String>().trim().to_string();
        }
    }

    if !ai_response.is_empty() {
        let preview = if ai_response.chars().count() > 100 {
            let p: String = ai_response.chars().take(100).collect();
            format!("{}...", p.replace('\n', " "))
        } else {
            ai_response.replace('\n', " ")
        };

        // Update database only if persistence is enabled
        if persistent {
            state
                .repo
                .update_entry_content(id, &ai_response, &preview)
                .map_err(|e| format!("Database update failed: {}", e))?;
        }

        // Sync Session History and Emit Update
        use crate::app_state::SessionHistory;
        if let Some(session) = app_handle.try_state::<SessionHistory>() {
            let mut history = session.0.lock().unwrap();
            if let Some(item) = history.iter_mut().find(|i| i.id == id) {
                item.content = ai_response.clone();
                item.preview = preview.clone();
                // Clear rich text so AI result is shown
                if item.content_type == "rich_text" {
                    item.content_type = "text".to_string();
                    item.html_content = None;
                }
                // Emit update event from session item
                let _ = app_handle.emit("clipboard-updated", item.clone());
            } else if persistent {
                // If persistent and not in session, fetch from DB to emit
                if let Ok(Some(updated_entry)) = state.repo.get_entry_by_id(id) {
                    let _ = app_handle.emit("clipboard-updated", updated_entry);
                }
            }
        }

        // Return response directly. Frontend will update local state to avoid race conditions.
        Ok(ai_response)
    } else {
        Err(format!(
            "AI response became empty after cleaning. Original: {}",
            raw_response
        )
        .into())
    }
}
