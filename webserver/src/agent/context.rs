use crate::agent::utils::truncate_text;
use std::collections::HashSet;

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

    let start_idx = if recent_user_turns_window == 0 {
        history.len()
    } else if user_indices.len() > recent_user_turns_window {
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

pub fn validate_message_sequence(messages: &[serde_json::Value]) -> Result<(), String> {
    let mut expected_tool_ids: Option<HashSet<String>> = None;

    for (idx, msg) in messages.iter().enumerate() {
        let role = get_role(msg).unwrap_or_default();

        if role == "tool" {
            let tool_call_id = msg
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let Some(expected) = expected_tool_ids.as_mut() else {
                return Err(format!(
                    "tool message at index {} has no preceding tool_calls",
                    idx
                ));
            };
            if !expected.remove(tool_call_id) {
                return Err(format!(
                    "tool message at index {} has unexpected tool_call_id '{}'",
                    idx, tool_call_id
                ));
            }
            continue;
        }

        if let Some(expected) = &expected_tool_ids
            && !expected.is_empty()
        {
            return Err(format!(
                "assistant tool_calls before index {} are missing tool results: {:?}",
                idx, expected
            ));
        }
        expected_tool_ids = None;

        if role == "assistant"
            && let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array())
            && !tool_calls.is_empty()
        {
            let ids = tool_calls
                .iter()
                .filter_map(|tc| tc.get("id").and_then(|v| v.as_str()))
                .filter(|id| !id.is_empty())
                .map(|id| id.to_string())
                .collect::<HashSet<_>>();
            if ids.len() != tool_calls.len() {
                return Err(format!(
                    "assistant tool_calls at index {} contain empty or duplicate ids",
                    idx
                ));
            }
            expected_tool_ids = Some(ids);
        }
    }

    if let Some(expected) = expected_tool_ids
        && !expected.is_empty()
    {
        return Err(format!(
            "assistant tool_calls at end of messages are missing tool results: {:?}",
            expected
        ));
    }

    Ok(())
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
