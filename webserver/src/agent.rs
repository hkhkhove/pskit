use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{
        Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;

use crate::AppState;
use crate::skills::{
    Skill, allowed_tool_set, default_skill, load_skills, render_skill_prompt, select_skill_with_llm,
};
use crate::tools::{execute_tool, get_tools};

const MAX_AGENT_STEPS: usize = 10;
const MAX_TOOL_CALLS: usize = 12;
const MAX_SAME_TOOL_SIGNATURE: usize = 1;

#[derive(Default, Clone)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments: String,
    extra_content: Option<serde_json::Value>,
}

fn merge_stream_field(current: &mut String, incoming: &str) {
    if incoming.is_empty() {
        return;
    }

    if !current.is_empty() && incoming.starts_with(current.as_str()) {
        *current = incoming.to_string();
        return;
    }

    if incoming.starts_with('{') {
        *current = incoming.to_string();
        return;
    }

    current.push_str(incoming);
}

fn find_or_create_tool_call(
    calls: &mut Vec<ToolCallAcc>,
    id_to_index: &mut HashMap<String, usize>,
    index_hint: Option<usize>,
    id_hint: Option<&str>,
    position_hint: usize,
) -> usize {
    if let Some(id) = id_hint {
        if let Some(idx) = id_to_index.get(id).copied() {
            return idx;
        }

        // If this call has a new ID, always allocate a new slot unless explicit index is provided.
        if index_hint.is_none() {
            calls.push(ToolCallAcc::default());
            let idx = calls.len() - 1;
            id_to_index.insert(id.to_string(), idx);
            return idx;
        }
    }

    if let Some(idx) = index_hint {
        while calls.len() <= idx {
            calls.push(ToolCallAcc::default());
        }
        return idx;
    }

    if position_hint < calls.len() {
        return position_hint;
    }

    calls.push(ToolCallAcc::default());
    calls.len() - 1
}

fn parse_tool_args(args_raw: &str) -> Result<serde_json::Value, String> {
    // 1) Happy path.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(args_raw) {
        return Ok(v);
    }

    // 2) Sometimes arguments are wrapped in a quoted JSON string.
    if let Ok(unwrapped) = serde_json::from_str::<String>(args_raw) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&unwrapped) {
            return Ok(v);
        }
    }

    // 3) Try extracting the largest {...} block.
    if let (Some(start), Some(end)) = (args_raw.find('{'), args_raw.rfind('}')) {
        if start < end {
            let candidate = &args_raw[start..=end];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) {
                return Ok(v);
            }
        }
    }

    Err(format!(
        "Invalid tool arguments (first 300 chars): {}",
        args_raw.chars().take(300).collect::<String>()
    ))
}

fn normalize_tool_signature(name: &str, args_raw: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(args_raw)
        .unwrap_or_else(|_| serde_json::json!({"raw": args_raw}));

    match name {
        "download_pdb_file" => {
            let pdb_id = parsed
                .get("pdb_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let format = parsed
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("cif")
                .to_ascii_lowercase();
            format!("{}::pdb_id={}::format={}", name, pdb_id, format)
        }
        "annotate_binding_pairs" => {
            let pdb_path = parsed
                .get("pdb_path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let canonical_pdb = pdb_path
                .strip_suffix(".cif")
                .or_else(|| pdb_path.strip_suffix(".pdb"))
                .unwrap_or(&pdb_path)
                .to_string();
            let cutoff = parsed.get("cutoff").and_then(|v| v.as_f64()).unwrap_or(3.5);
            format!("{}::pdb={}::cutoff={:.3}", name, canonical_pdb, cutoff)
        }
        _ => format!("{}::{}", name, args_raw),
    }
}

fn sanitize_display_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if !normalized.starts_with('/') {
        return path.to_string();
    }

    let marker = "/tasks/agent_sessions/";
    if let Some(idx) = normalized.find(marker) {
        let tail = &normalized[idx + marker.len()..];
        let parts = tail.split('/').collect::<Vec<_>>();
        if parts.len() >= 3 {
            return parts[2..].join("/");
        }
    }

    let parts = normalized
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if parts.len() >= 2 {
        return format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    }

    normalized
}

fn sanitize_display_value(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => {
            if s.starts_with('/') {
                *s = sanitize_display_path(s);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                sanitize_display_value(item);
            }
        }
        serde_json::Value::Object(map) => {
            for value in map.values_mut() {
                sanitize_display_value(value);
            }
        }
        _ => {}
    }
}

fn sanitize_tool_args_for_display(args_raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(args_raw) {
        Ok(mut v) => {
            sanitize_display_value(&mut v);
            serde_json::to_string(&v).unwrap_or_else(|_| args_raw.to_string())
        }
        Err(_) => args_raw.to_string(),
    }
}

fn debug_enabled() -> bool {
    matches!(
        std::env::var("PSKIT_AGENT_DEBUG").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("on") | Some("ON")
    )
}

fn debug_log(msg: &str) {
    if debug_enabled() {
        eprintln!("[PSKIT_AGENT_DEBUG] {}", msg);
    }
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Text { content: String },
    ToolCall { name: String, args: String },
    ToolResult { content: String },
    Error { message: String },
    Done,
}

#[derive(Serialize)]
struct AgentSessionFile {
    path: String,
    size: u64,
    download_url: String,
}

#[derive(Serialize)]
struct AgentSessionFilesResponse {
    session_id: String,
    total_files: usize,
    files: Vec<AgentSessionFile>,
}

pub fn agent_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(chat_handler))
        .route("/sessions/{session_id}/files", get(list_session_files))
        .route(
            "/sessions/{session_id}/download/{*relative_path}",
            get(download_session_file),
        )
}

fn content_type_for_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("txt") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("tsv") => "text/tab-separated-values",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        Some("npy") => "application/octet-stream",
        Some("dssp") => "text/plain; charset=utf-8",
        Some("log") => "text/plain; charset=utf-8",
        Some("pdb") => "chemical/x-pdb",
        Some("cif") => "chemical/x-cif",
        _ => "application/octet-stream",
    }
}

async fn collect_session_files(
    session_dir: &std::path::Path,
) -> Result<Vec<(PathBuf, u64)>, String> {
    let mut files = Vec::new();
    let mut stack = vec![session_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| format!("Failed to read dir {}: {}", dir.display(), e))?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let path = entry.path();
            let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                files.push((path, metadata.len()));
            }
        }
    }

    Ok(files)
}

async fn list_session_files(
    Path(session_id): Path<String>,
) -> Result<Json<AgentSessionFilesResponse>, (StatusCode, String)> {
    let home = crate::config::Config::home();
    let session_dir = home.join("tasks").join("agent_sessions").join(&session_id);

    if !session_dir.exists() {
        return Ok(Json(AgentSessionFilesResponse {
            session_id,
            total_files: 0,
            files: Vec::new(),
        }));
    }

    let mut files = collect_session_files(&session_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .into_iter()
        .filter_map(|(path, size)| {
            let rel = path.strip_prefix(&session_dir).ok()?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            Some(AgentSessionFile {
                path: rel_str.clone(),
                size,
                download_url: format!("/api/agent/sessions/{}/download/{}", session_id, rel_str),
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Json(AgentSessionFilesResponse {
        session_id,
        total_files: files.len(),
        files,
    }))
}

async fn download_session_file(
    Path((session_id, relative_path)): Path<(String, String)>,
) -> Result<Response, (StatusCode, String)> {
    let home = crate::config::Config::home();
    let session_dir = home.join("tasks").join("agent_sessions").join(&session_id);
    let file_path = session_dir.join(&relative_path);

    if !file_path.exists() {
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    let session_dir_canonical = tokio::fs::canonicalize(&session_dir)
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid session path".to_string()))?;
    let file_path_canonical = tokio::fs::canonicalize(&file_path)
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid file path".to_string()))?;

    if !file_path_canonical.starts_with(&session_dir_canonical) {
        return Err((StatusCode::BAD_REQUEST, "Invalid file path".to_string()));
    }

    let file_content = tokio::fs::read(&file_path_canonical).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read file".to_string(),
        )
    })?;

    let filename = file_path_canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download.bin")
        .to_string();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type_for_path(&file_path_canonical))
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(file_content.into())
        .unwrap())
}

async fn chat_handler(
    State(_state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel();
    let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    tokio::spawn(async move {
        if let Err(e) = run_agent_loop(req.message, session_id, tx.clone()).await {
            let _ = tx.send(Ok(Event::default()
                .json_data(AgentEvent::Error {
                    message: e.to_string(),
                })
                .unwrap()));
        }
        let _ = tx.send(Ok(Event::default().json_data(AgentEvent::Done).unwrap()));
    });

    let stream = UnboundedReceiverStream::new(rx);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}

async fn run_agent_loop(
    user_message: String,
    session_id: String,
    tx: mpsc::UnboundedSender<Result<Event, Infallible>>,
) -> anyhow::Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_else(|_| "".to_string());
    if api_key.is_empty() {
        return Err(anyhow::anyhow!(
            "GEMINI_API_KEY environment variable is not set."
        ));
    }

    let client = reqwest::Client::new();
    let api_url = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";

    let skills = load_skills()?;
    let selected_skill_id =
        select_skill_with_llm(&client, api_url, &api_key, &user_message, &skills)
            .await
            .ok()
            .flatten();
    let selected_skill: Skill = if let Some(skill_id) = selected_skill_id {
        skills
            .iter()
            .find(|s| s.id == skill_id)
            .cloned()
            .or_else(|| default_skill(&skills))
            .ok_or_else(|| anyhow::anyhow!("No skills configured"))?
    } else {
        default_skill(&skills).ok_or_else(|| anyhow::anyhow!("No skills configured"))?
    };
    let allowed_tools = allowed_tool_set(&selected_skill);

    // Create session dir
    let home = crate::config::Config::home();
    let session_dir = home.join("tasks").join("agent_sessions").join(&session_id);
    tokio::fs::create_dir_all(&session_dir).await?;

    let mut messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "You are an expert structural bioinformatics agent for PSKit. You have tools to search PDB, fetch info, split complexes, extract features, and predict interactions/binding sites. Use tools to answer the user's queries. You MUST use the tools provided. Before analyzing structure, always use tools to fetch and process the PDB first. Keep tool arguments simple and exactly as defined."
        }),
        serde_json::json!({
            "role": "system",
            "content": render_skill_prompt(&selected_skill),
        }),
        serde_json::json!({
            "role": "user",
            "content": user_message,
        }),
    ];

    let tools = serde_json::to_value(get_tools())?;
    let filtered_tools: Vec<serde_json::Value> = tools
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            let name = tool
                .get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            allowed_tools.contains(name)
        })
        .collect();
    let mut agent_steps = 0usize;
    let mut total_tool_calls = 0usize;
    let mut tool_signature_counts: HashMap<String, usize> = HashMap::new();
    let mut tool_name_counts: HashMap<String, usize> = HashMap::new();
    let mut tools_enabled = true;

    loop {
        agent_steps += 1;
        if agent_steps > MAX_AGENT_STEPS {
            let msg = format!(
                "Stopped after {} planning rounds to prevent infinite loop.",
                MAX_AGENT_STEPS
            );
            let _ = tx.send(Ok(Event::default()
                .json_data(AgentEvent::Error {
                    message: msg.clone(),
                })
                .unwrap()));
            break;
        }

        let mut request_body = serde_json::json!({
            "model": "gemini-3-flash-preview",
            "messages": messages,
            "stream": true,
        });
        if tools_enabled {
            request_body["tools"] = serde_json::Value::Array(filtered_tools.clone());
        }

        if debug_enabled() {
            let msg_count = request_body
                .get("messages")
                .and_then(|v| v.as_array())
                .map(|v| v.len())
                .unwrap_or(0);
            let tool_count = request_body
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|v| v.len())
                .unwrap_or(0);
            debug_log(&format!(
                "Sending request: model=gemini-3-flash-preview, stream=true, skill={}, messages={}, tools={}, tools_enabled={}",
                selected_skill.id, msg_count, tool_count, tools_enabled
            ));
        }

        let response = client
            .post(api_url)
            .bearer_auth(&api_key)
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let raw_text = response.text().await?;
            return Err(anyhow::anyhow!("Gemini API error {}: {}", status, raw_text));
        }

        let mut assistant_content = String::new();
        let mut calls: Vec<ToolCallAcc> = Vec::new();
        let mut id_to_index: HashMap<String, usize> = HashMap::new();

        let mut buffer = String::new();
        let mut stream_done = false;
        let mut response = response;
        while let Some(bytes) = response.chunk().await? {
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(event_end) = buffer.find("\n\n") {
                let event_text = buffer[..event_end].to_string();
                buffer.drain(..event_end + 2);

                let mut data_lines = Vec::new();
                for raw_line in event_text.lines() {
                    let line = raw_line.trim_end_matches('\r');
                    if let Some(rest) = line.strip_prefix("data:") {
                        data_lines.push(rest.trim_start());
                    }
                }

                if data_lines.is_empty() {
                    continue;
                }

                let data_payload = data_lines.join("\n");
                if data_payload == "[DONE]" {
                    debug_log("Received [DONE] from Gemini stream");
                    stream_done = true;
                    break;
                }

                let chunk: serde_json::Value = match serde_json::from_str(&data_payload) {
                    Ok(v) => v,
                    Err(err) => {
                        debug_log(&format!(
                            "Skip non-JSON chunk ({}), first 120 chars: {}",
                            err,
                            data_payload.chars().take(120).collect::<String>()
                        ));
                        continue;
                    }
                };

                if debug_enabled() {
                    let choice_count = chunk
                        .get("choices")
                        .and_then(|v| v.as_array())
                        .map(|v| v.len())
                        .unwrap_or(0);
                    debug_log(&format!("Chunk parsed: choices={}", choice_count));
                }

                if let Some(choices) = chunk.get("choices").and_then(|v| v.as_array()) {
                    for choice in choices {
                        let delta = choice.get("delta").cloned().unwrap_or_default();

                        if let Some(content_part) = delta.get("content").and_then(|v| v.as_str()) {
                            assistant_content.push_str(content_part);
                            let _ = tx.send(Ok(Event::default()
                                .json_data(AgentEvent::Text {
                                    content: content_part.to_string(),
                                })
                                .unwrap()));
                        }

                        if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array())
                        {
                            for (pos, tc) in tool_calls.iter().enumerate() {
                                let index_hint =
                                    tc.get("index").and_then(|v| v.as_u64()).map(|n| n as usize);
                                let id_hint = tc.get("id").and_then(|v| v.as_str());
                                let idx = find_or_create_tool_call(
                                    &mut calls,
                                    &mut id_to_index,
                                    index_hint,
                                    id_hint,
                                    pos,
                                );

                                if let Some(id) = id_hint {
                                    calls[idx].id = id.to_string();
                                    id_to_index.insert(id.to_string(), idx);
                                }

                                if let Some(func) = tc.get("function") {
                                    if let Some(name_part) =
                                        func.get("name").and_then(|v| v.as_str())
                                    {
                                        merge_stream_field(&mut calls[idx].name, name_part);
                                    }
                                    if let Some(args_part) =
                                        func.get("arguments").and_then(|v| v.as_str())
                                    {
                                        merge_stream_field(&mut calls[idx].arguments, args_part);
                                    }
                                }

                                if let Some(extra) = tc.get("extra_content") {
                                    calls[idx].extra_content = Some(extra.clone());
                                    if debug_enabled() {
                                        let has_sig = extra
                                            .get("google")
                                            .and_then(|v| v.get("thought_signature"))
                                            .is_some();
                                        debug_log(&format!(
                                            "Tool-call chunk idx={} got extra_content, thought_signature_present={}",
                                            idx, has_sig
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if stream_done {
                break;
            }
        }

        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        for (i, call) in calls.iter().enumerate() {
            if call.name.is_empty() {
                continue;
            }
            let id = if call.id.is_empty() {
                format!("function-call-{}", i)
            } else {
                call.id.clone()
            };

            let mut tc = serde_json::json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": if call.arguments.is_empty() { "{}" } else { &call.arguments }
                }
            });
            if let Some(extra) = &call.extra_content {
                tc["extra_content"] = extra.clone();
            }
            if debug_enabled() {
                let has_sig = call
                    .extra_content
                    .as_ref()
                    .and_then(|v| v.get("google"))
                    .and_then(|v| v.get("thought_signature"))
                    .is_some();
                debug_log(&format!(
                    "Rebuilt tool_call {}: id={}, name={}, args_len={}, thought_signature_present={}",
                    i,
                    id,
                    call.name,
                    call.arguments.len(),
                    has_sig
                ));
            }
            tool_calls.push(tc);
        }

        let mut message = serde_json::json!({ "role": "assistant" });
        if !assistant_content.is_empty() {
            message["content"] = serde_json::Value::String(assistant_content);
        }
        if !tool_calls.is_empty() {
            message["tool_calls"] = serde_json::Value::Array(tool_calls.clone());
        }

        if !tool_calls.is_empty() {
            if !tools_enabled {
                // Some models may still emit tool_calls even when no tools are supplied.
                // Treat this as a finalization step and ask model to answer without tools.
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": "Tool calls are disabled now. Provide final answer directly without calling tools."
                }));
                continue;
            }

            // IMPORTANT: preserve full assistant message exactly, including
            // tool_calls.extra_content.google.thought_signature.
            debug_log(&format!(
                "Assistant requested {} tool call(s). Preserving full assistant message for replay.",
                tool_calls.len()
            ));
            messages.push(message.clone());

            let mut terminal_reached_in_batch = false;
            for tool_call in tool_calls {
                if terminal_reached_in_batch {
                    break;
                }

                if total_tool_calls >= MAX_TOOL_CALLS {
                    let msg = format!(
                        "Stopped after {} tool calls to prevent runaway execution.",
                        MAX_TOOL_CALLS
                    );
                    let _ = tx.send(Ok(Event::default()
                        .json_data(AgentEvent::Error {
                            message: msg.clone(),
                        })
                        .unwrap()));
                    return Ok(());
                }

                let tool_id = tool_call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let name = tool_call
                    .get("function")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let args_str = tool_call
                    .get("function")
                    .and_then(|v| v.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string();

                if let Some(max_calls) = selected_skill.max_calls_per_tool.get(&name) {
                    let used = tool_name_counts.get(&name).copied().unwrap_or(0);
                    if used >= *max_calls {
                        let skip_msg = format!(
                            "Tool '{}' skipped: reached skill limit {}/{} for skill '{}'.",
                            name, used, max_calls, selected_skill.id
                        );
                        let _ = tx.send(Ok(Event::default()
                            .json_data(AgentEvent::ToolResult {
                                content: skip_msg.clone(),
                            })
                            .unwrap()));
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "name": name,
                            "tool_call_id": tool_id,
                            "content": skip_msg,
                        }));
                        continue;
                    }
                }

                if !allowed_tools.contains(name.as_str()) {
                    let skip_msg = format!(
                        "Tool '{}' is not allowed for active skill '{}'",
                        name, selected_skill.id
                    );
                    let _ = tx.send(Ok(Event::default()
                        .json_data(AgentEvent::ToolResult {
                            content: skip_msg.clone(),
                        })
                        .unwrap()));
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "name": name,
                        "tool_call_id": tool_id,
                        "content": skip_msg,
                    }));
                    continue;
                }

                total_tool_calls += 1;
                tool_name_counts
                    .entry(name.clone())
                    .and_modify(|n| *n += 1)
                    .or_insert(1);

                let signature = normalize_tool_signature(&name, &args_str);
                let seen = tool_signature_counts
                    .entry(signature.clone())
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
                if *seen > MAX_SAME_TOOL_SIGNATURE {
                    let skip_msg = format!(
                        "Skipped repeated tool call {} with same args to avoid loop.",
                        name
                    );
                    let _ = tx.send(Ok(Event::default()
                        .json_data(AgentEvent::ToolResult {
                            content: skip_msg.clone(),
                        })
                        .unwrap()));
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "name": name,
                        "tool_call_id": tool_id,
                        "content": skip_msg,
                    }));
                    continue;
                }

                let _ = tx.send(Ok(Event::default()
                    .json_data(AgentEvent::ToolCall {
                        name: name.clone(),
                        args: sanitize_tool_args_for_display(&args_str),
                    })
                    .unwrap()));

                debug_log(&format!(
                    "Executing tool: name={}, tool_call_id={}, args_len={}",
                    name,
                    tool_id,
                    args_str.len()
                ));

                let args_json = match parse_tool_args(&args_str) {
                    Ok(v) => v,
                    Err(err) => {
                        let err_msg = format!("Tool arguments parse failed: {}", err);
                        let _ = tx.send(Ok(Event::default()
                            .json_data(AgentEvent::ToolResult {
                                content: err_msg.clone(),
                            })
                            .unwrap()));
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "name": name,
                            "tool_call_id": tool_id,
                            "content": err_msg,
                        }));
                        continue;
                    }
                };

                let result = match execute_tool(&name, args_json, &session_dir).await {
                    Ok(res) => res,
                    Err(err) => format!("Error executing tool: {}", err),
                };

                if let Some(terminal_tool) = &selected_skill.terminal_tool {
                    if &name == terminal_tool && !result.starts_with("Error executing tool:") {
                        tools_enabled = false;
                        terminal_reached_in_batch = true;
                        messages.push(serde_json::json!({
                            "role": "system",
                            "content": format!(
                                "Skill '{}' stop condition reached by tool '{}'. Do not call more tools and produce final answer.",
                                selected_skill.id,
                                terminal_tool
                            )
                        }));
                    }
                }

                debug_log(&format!(
                    "Tool finished: name={}, result_len={}",
                    name,
                    result.len()
                ));

                let _ = tx.send(Ok(Event::default()
                    .json_data(AgentEvent::ToolResult {
                        content: result.clone(),
                    })
                    .unwrap()));

                messages.push(serde_json::json!({
                    "role": "tool",
                    "name": name,
                    "tool_call_id": tool_id,
                    "content": result,
                }));
            }
        } else {
            debug_log("No tool calls in this step; finishing loop.");
            messages.push(message);
            break;
        }
    }

    Ok(())
}
