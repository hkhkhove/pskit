use crate::agent::context::build_history_for_summary;

fn sanitize_session_title(raw: &str) -> String {
    let mut title = raw.replace('\n', " ").replace('\r', " ");
    title = title.trim().trim_matches('"').trim_matches('`').to_string();
    let collapsed = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() > 40 {
        collapsed.chars().take(40).collect::<String>()
    } else {
        collapsed
    }
}

pub async fn generate_session_title(
    client: &reqwest::Client,
    model: &str,
    api_url: &str,
    api_key: &str,
    first_user_message: &str,
) -> Option<String> {
    let prompt = "Generate a very short chat title (max 12 words) for the user's first message. Return only the title text, no quotes, no punctuation suffix.";
    let body = serde_json::json!({
        "model": model,
        "stream": false,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": first_user_message}
        ]
    });

    let resp = client
        .post(api_url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let value: serde_json::Value = resp.json().await.ok()?;
    let content = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("message"))
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())?;

    let title = sanitize_session_title(content);
    if title.is_empty() { None } else { Some(title) }
}

pub async fn generate_history_summary(
    client: &reqwest::Client,
    model: &str,
    api_url: &str,
    api_key: &str,
    history: &[serde_json::Value],
) -> Option<String> {
    let transcript = build_history_for_summary(history);
    if transcript.is_empty() {
        return None;
    }

    let prompt = "Summarize the conversation for long-context memory. Keep it concise and factual. Include: user goals, key decisions, important tool findings, unresolved questions, and constraints. Output plain text under 180 words.";
    let body = serde_json::json!({
        "model": model,
        "stream": false,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": transcript}
        ]
    });

    let response = client
        .post(api_url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }

    let value: serde_json::Value = response.json().await.ok()?;
    let summary = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("message"))
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())?;

    if summary.is_empty() {
        None
    } else {
        Some(summary)
    }
}
