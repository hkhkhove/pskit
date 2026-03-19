use crate::agent::utils::truncate_text;

pub const SUMMARY_MARKER: &str = "[[SESSION_SUMMARY]]";

pub fn get_role(message: &serde_json::Value) -> Option<&str> {
    message.get("role").and_then(|v| v.as_str())
}

pub fn get_content(message: &serde_json::Value) -> Option<&str> {
    message.get("content").and_then(|v| v.as_str())
}

pub fn extract_latest_summary(history: &[serde_json::Value]) -> Option<String> {
    for msg in history.iter().rev() {
        if get_role(msg) != Some("system") {
            continue;
        }
        if let Some(content) = get_content(msg)
            && let Some(summary) = content.strip_prefix(&format!("{}\n", SUMMARY_MARKER))
        {
            let trimmed = summary.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn count_user_turns(history: &[serde_json::Value]) -> usize {
    history
        .iter()
        .filter(|m| get_role(m) == Some("user"))
        .count()
}

pub fn build_context_history(
    history: &[serde_json::Value],
    recent_user_turns_window: usize,
) -> Vec<serde_json::Value> {
    let user_indices = history
        .iter()
        .enumerate()
        .filter_map(|(idx, msg)| (get_role(msg) == Some("user")).then_some(idx))
        .collect::<Vec<_>>();

    let start_idx = if user_indices.len() > recent_user_turns_window {
        user_indices[user_indices.len() - recent_user_turns_window]
    } else {
        0
    };

    let mut context = history[start_idx..]
        .iter()
        .filter(|m| {
            // Keep a single summary message (latest one inserted below) to avoid duplicates.
            !(get_role(m) == Some("system")
                && get_content(m)
                    .map(|c| c.starts_with(SUMMARY_MARKER))
                    .unwrap_or(false))
        })
        .cloned()
        .collect::<Vec<_>>();

    if let Some(summary) = extract_latest_summary(history) {
        context.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": format!("{}\n{}", SUMMARY_MARKER, summary)
            }),
        );
    }

    context
}

pub fn build_history_for_summary(history: &[serde_json::Value]) -> String {
    let mut lines = Vec::new();
    for msg in history {
        let role = match get_role(msg) {
            Some("user") => "User",
            Some("assistant") => "Assistant",
            Some("tool") => "Tool",
            _ => continue,
        };

        let mut content = get_content(msg).unwrap_or("").trim().to_string();
        if role == "Tool" {
            content = truncate_text(&content, 500);
        }
        if content.is_empty() {
            continue;
        }
        lines.push(format!("{}: {}", role, content));
    }
    lines.join("\n")
}
