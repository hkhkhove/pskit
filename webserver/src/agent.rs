use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, Sse},
    routing::{delete, get, post},
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::TryAcquireError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;

use crate::AppState;

mod context;
mod store;
mod summarizer;
mod tools;
mod utils;

use self::tools::{execute_tool, get_tools};

use self::context::{
    SUMMARY_MARKER, build_context_history, count_user_turns, get_content, get_role,
    validate_message_sequence,
};
use self::store::{
    build_session_dir, delete_session, download_session_tool_file, get_session_history,
    history_file, is_valid_session_id, list_session_files, read_history, ui_history_file,
    upload_session_files, write_history,
};
use self::summarizer::{generate_history_summary, generate_session_title};
use self::utils::{parse_tool_args, truncate_text};

#[derive(Debug, Deserialize)]
struct ModelConfig {
    model: String,
    api: String,
}

#[derive(Debug, Deserialize)]
struct AgentConfig {
    chat: ModelConfig,
    summary: ModelConfig,

    max_agent_steps: usize,
    max_tool_calls: usize,
    recent_user_turns_window: usize,
    summary_every_user_turns: usize,
    max_tool_result_chars: usize,
}

#[derive(Default, Clone)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments: String,
    extra_content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub display_message: Option<String>,
    pub uploaded_files: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionTitle {
        session_id: String,
        title: String,
    },
    Text {
        content: String,
    },
    ToolCall {
        tool_call_id: String,
        name: String,
        args: String,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
    },
    ToolStatus {
        tool_call_id: String,
        status: String,
        message: Option<String>,
    },
    Error {
        message: String,
    },
    Done,
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

fn needs_cpu_semaphore(tool_name: &str) -> bool {
    matches!(tool_name, "predict_binding_sites" | "predict_interaction")
}

fn tool_name_set(tools: &[serde_json::Value]) -> HashSet<String> {
    tools
        .iter()
        .filter_map(|tool| {
            tool.get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

fn make_ui_tool_call(id: &str, name: &str, args: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "args": args,
        "status": "running",
        "waitMessage": "",
        "result": "",
        "files": [],
        "expanded": false,
    })
}

fn history_without_system_prompt(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    if messages.len() > 1 {
        messages[1..].to_vec()
    } else {
        messages.to_vec()
    }
}

async fn persist_agent_history_snapshot(history_file: &Path, messages: &[serde_json::Value]) {
    let history_to_save = history_without_system_prompt(messages);
    if let Err(err) = write_history(history_file, &history_to_save).await {
        debug_log(&format!("Failed to write agent history snapshot: {}", err));
    }
}

async fn persist_agent_ui_snapshot(
    ui_history_file: &Path,
    base_ui_history: &[serde_json::Value],
    display_message: &str,
    uploaded_files: &[serde_json::Value],
    ui_assistant_content: &str,
    ui_tool_calls: &[serde_json::Value],
) {
    let mut ui_history = base_ui_history.to_vec();
    let mut ui_user_message = serde_json::json!({
        "role": "user",
        "content": display_message,
    });
    if !uploaded_files.is_empty() {
        ui_user_message["files"] = serde_json::Value::Array(uploaded_files.to_vec());
    }
    ui_history.push(ui_user_message);

    if !ui_assistant_content.is_empty() || !ui_tool_calls.is_empty() {
        ui_history.push(serde_json::json!({
            "role": "assistant",
            "content": ui_assistant_content,
            "toolCalls": ui_tool_calls,
        }));
    }

    if let Err(err) = write_history(ui_history_file, &ui_history).await {
        debug_log(&format!(
            "Failed to write agent UI history snapshot: {}",
            err
        ));
    }
}

fn load_agent_config() -> anyhow::Result<AgentConfig> {
    let config_path = crate::config::Config::home()
        .join("agent_config")
        .join("config.toml");
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("Failed to read agent config: {}", e))?;
    let config: AgentConfig = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse agent config: {}", e))?;
    Ok(config)
}

fn load_agent_prompt(filename: &str, label: &str) -> anyhow::Result<String> {
    let config_path = crate::config::Config::home()
        .join("agent_config")
        .join(filename);
    let prompt = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", label, e))?;
    Ok(prompt)
}

pub fn agent_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(chat_handler))
        .route("/sessions/{session_id}", delete(delete_session))
        .route(
            "/sessions/{session_id}/files",
            get(list_session_files).post(upload_session_files),
        )
        .route("/sessions/{session_id}/history", get(get_session_history))
        .route(
            "/sessions/{session_id}/{tool_call_id}/{filename}",
            get(download_session_tool_file),
        )
}

async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel();
    let session_id = req.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if !is_valid_session_id(&session_id) {
        let _ = tx.send(Ok(Event::default()
            .json_data(AgentEvent::Error {
                message: "Invalid session_id".to_string(),
            })
            .unwrap()));
        let _ = tx.send(Ok(Event::default().json_data(AgentEvent::Done).unwrap()));
        let stream = UnboundedReceiverStream::new(rx);
        return Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new());
    }

    let session_lock = {
        let mut locks = state.agent_session_locks.lock().await;
        locks
            .entry(session_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    //只是引用计数加一，指向的还是同一信号量
    let cpu_task_semaphore = state.cpu_task_semaphore.clone();

    let worker_tx = tx.clone();
    let worker = tokio::spawn(async move {
        let _session_guard = session_lock.lock().await;
        let display_message = req
            .display_message
            .clone()
            .unwrap_or_else(|| req.message.clone());
        let uploaded_files = req.uploaded_files.unwrap_or_default();
        if let Err(e) = run_agent_loop(
            req.message,
            display_message,
            uploaded_files,
            session_id,
            cpu_task_semaphore,
            worker_tx.clone(),
        )
        .await
        {
            let _ = worker_tx.send(Ok(Event::default()
                .json_data(AgentEvent::Error {
                    message: e.to_string(),
                })
                .unwrap()));
        }
        let _ = worker_tx.send(Ok(Event::default().json_data(AgentEvent::Done).unwrap()));
    });

    let close_watch_tx = tx.clone();
    let abort_handle = worker.abort_handle();
    tokio::spawn(async move {
        close_watch_tx.closed().await;
        abort_handle.abort();
    });

    let stream = UnboundedReceiverStream::new(rx);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}

async fn run_agent_loop(
    user_message: String,
    display_message: String,
    uploaded_files: Vec<serde_json::Value>,
    session_id: String,
    cpu_task_semaphore: Arc<tokio::sync::Semaphore>,
    tx: mpsc::UnboundedSender<Result<Event, Infallible>>,
) -> anyhow::Result<()> {
    let api_key = std::env::var("API_KEY").unwrap_or_else(|_| "".to_string());
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("API_KEY environment variable is not set."));
    }

    let agent_config = load_agent_config()?;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()?;

    let home = crate::config::Config::home();
    let session_dir = build_session_dir(&home, &session_id);
    tokio::fs::create_dir_all(&session_dir).await?;

    let history_file = history_file(&session_dir);
    let ui_history_file = ui_history_file(&session_dir);

    let mut has_prior_history = false;
    let mut stored_history: Vec<serde_json::Value> = Vec::new();
    if let Ok(mut history) = read_history(&history_file).await {
        has_prior_history = !history.is_empty();
        stored_history.append(&mut history);
    }

    let system_prompt = load_agent_prompt("system_prompt.txt", "system prompt")?;
    let title_prompt = load_agent_prompt("title_prompt.txt", "title prompt")?;
    let summary_prompt = load_agent_prompt("summary_prompt.txt", "summary prompt")?;

    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt,
    })];

    let mut context_history =
        build_context_history(&stored_history, agent_config.recent_user_turns_window);
    messages.append(&mut context_history);

    messages.push(serde_json::json!({
        "role": "user",
        "content": user_message,
    }));

    let base_ui_history = read_history(&ui_history_file).await.unwrap_or_default();

    // 新对话异步生成标题，不阻塞主回答生成
    let title_task = if !has_prior_history {
        let title_client = client.clone();
        let title_model = agent_config.summary.model.clone();
        let title_api = agent_config.summary.api.clone();
        let title_api_key = api_key.clone();
        let title_prompt = title_prompt.clone();
        let title_user_message = user_message.clone();
        let title_session_id = session_id.clone();
        let title_tx = tx.clone();

        Some(tokio::spawn(async move {
            if let Some(title) = generate_session_title(
                &title_client,
                &title_model,
                &title_api,
                &title_api_key,
                &title_prompt,
                &title_user_message,
            )
            .await
            {
                let _ = title_tx.send(Ok(Event::default()
                    .json_data(AgentEvent::SessionTitle {
                        session_id: title_session_id,
                        title,
                    })
                    .unwrap()));
            } else {
                debug_log("Failed to generate session title.");
            }
        }))
    } else {
        None
    };

    let tools = get_tools()?;
    let allowed_tool_names = tool_name_set(&tools);
    let mut agent_steps = 0usize;
    let mut total_tool_calls = 0usize;
    let tools_enabled = true;
    let mut ui_assistant_content = String::new();
    let mut ui_tool_calls: Vec<serde_json::Value> = Vec::new();
    let mut unsaved_ui_chars = 0usize;

    persist_agent_history_snapshot(&history_file, &messages).await;
    persist_agent_ui_snapshot(
        &ui_history_file,
        &base_ui_history,
        &display_message,
        &uploaded_files,
        &ui_assistant_content,
        &ui_tool_calls,
    )
    .await;

    loop {
        agent_steps += 1;
        if agent_steps > agent_config.max_agent_steps {
            let msg = format!(
                "Stopped after {} planning rounds to prevent infinite loop.",
                agent_config.max_agent_steps
            );

            let _ = tx.send(Ok(Event::default()
                .json_data(AgentEvent::Error {
                    message: msg.clone(),
                })
                .unwrap()));
            break;
        }
        //启用Stream
        validate_message_sequence(&messages)
            .map_err(|e| anyhow::anyhow!("Invalid message sequence before LLM request: {}", e))?;

        let mut request_body = serde_json::json!({
            "model": &agent_config.chat.model,
            "messages": messages,
            "stream": true,
        });
        if tools_enabled && !tools.is_empty() {
            request_body["tools"] = serde_json::Value::Array(tools.clone());
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
                "Sending request: model={}, stream=true, messages={}, tools={}, tools_enabled={}",
                agent_config.chat.model, msg_count, tool_count, tools_enabled
            ));
        }
        //LLM响应头
        let response = client
            .post(&agent_config.chat.api)
            .bearer_auth(&api_key)
            .json(&request_body)
            .send()
            .await?;

        debug_log(&format!("Request body: {}", request_body));

        let status = response.status();
        if !status.is_success() {
            let raw_text = response.text().await?;
            return Err(anyhow::anyhow!("LLM API error {}: {}", status, raw_text));
        }

        let mut assistant_content = String::new();
        let mut reasoning_content = String::new();
        let mut calls: Vec<ToolCallAcc> = Vec::new();

        let mut buffer = String::new();
        let mut response = response;
        //流式传输LLM生成内容
        while let Some(bytes) = response.chunk().await? {
            //bytes是不断到达的LLM输出内容，可能包含多条SSE事件，或者部分事件
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            //SSE规定每个事件以双换行分割
            //一个事件可能有一行或多行以data:开头的内容？
            while let Some(event_end) = buffer.find("\n\n") {
                let event_text = buffer[..event_end].to_string();
                //移除已处理文本
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
                    continue;
                }

                //把一个事件中多行data:内容合并成一个完整的JSON块，进行解析
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

                //LLM可能会生成多个不同的回答（choices）
                if let Some(choices) = chunk.get("choices").and_then(|v| v.as_array()) {
                    //流式传输中，delta(增量)替代message，需要不断合并增量内容构建完整message
                    let choice = &choices[0];
                    let delta = choice.get("delta").cloned().unwrap_or_default();

                    if let Some(content_part) = delta.get("content").and_then(|v| v.as_str()) {
                        assistant_content.push_str(content_part);
                        ui_assistant_content.push_str(content_part);
                        unsaved_ui_chars += content_part.len();
                        let _ = tx.send(Ok(Event::default()
                            .json_data(AgentEvent::Text {
                                content: content_part.to_string(),
                            })
                            .unwrap()));
                        if unsaved_ui_chars >= 1024 {
                            persist_agent_ui_snapshot(
                                &ui_history_file,
                                &base_ui_history,
                                &display_message,
                                &uploaded_files,
                                &ui_assistant_content,
                                &ui_tool_calls,
                            )
                            .await;
                            unsaved_ui_chars = 0;
                        }
                    }
                    if let Some(reasoning_part) =
                        delta.get("reasoning_content").and_then(|v| v.as_str())
                    {
                        reasoning_content.push_str(reasoning_part);
                    }
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tool_calls.iter() {
                            let idx = tc
                                .get("index")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize)
                                .unwrap_or(calls.len());

                            while calls.len() <= idx {
                                calls.push(ToolCallAcc::default());
                            }

                            if let Some(id) = tc.get("id") {
                                let id_part = id.as_str().unwrap_or_default();
                                if !id_part.is_empty() {
                                    calls[idx].id = id_part.to_string();
                                }
                            }

                            if let Some(func) = tc.get("function") {
                                if let Some(name_part) = func.get("name") {
                                    calls[idx]
                                        .name
                                        .push_str(name_part.as_str().unwrap_or_default());
                                }
                                if let Some(args_part) = func.get("arguments") {
                                    calls[idx]
                                        .arguments
                                        .push_str(args_part.as_str().unwrap_or_default());
                                }
                            }

                            if let Some(extra) = tc.get("extra_content") {
                                calls[idx].extra_content = Some(extra.clone());
                            }
                        }
                    }
                }
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

            tool_calls.push(tc);
        }

        let mut message = serde_json::json!({
            "role": "assistant",
            "content": assistant_content,
        });
        if !reasoning_content.is_empty() {
            message["reasoning_content"] = serde_json::Value::String(reasoning_content);
        }
        if unsaved_ui_chars > 0 {
            persist_agent_ui_snapshot(
                &ui_history_file,
                &base_ui_history,
                &display_message,
                &uploaded_files,
                &ui_assistant_content,
                &ui_tool_calls,
            )
            .await;
            unsaved_ui_chars = 0;
        }

        if !tool_calls.is_empty() {
            message["tool_calls"] = serde_json::Value::Array(tool_calls.clone());

            if !tools_enabled {
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": "Tool calls are disabled now. Provide final answer directly without calling tools."
                }));
                continue;
            }

            messages.push(message.clone());
            persist_agent_history_snapshot(&history_file, &messages).await;

            for tool_call in tool_calls {
                if total_tool_calls >= agent_config.max_tool_calls {
                    let msg = format!(
                        "Stopped after {} tool calls to prevent runaway execution.",
                        agent_config.max_tool_calls
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
                let ui_tool_idx = ui_tool_calls.len();
                ui_tool_calls.push(make_ui_tool_call(&tool_id, &name, &args_str));
                persist_agent_ui_snapshot(
                    &ui_history_file,
                    &base_ui_history,
                    &display_message,
                    &uploaded_files,
                    &ui_assistant_content,
                    &ui_tool_calls,
                )
                .await;

                if !allowed_tool_names.contains(name.as_str()) {
                    let skip_msg = format!("Tool '{}' is not defined in the tool catalog", name);
                    let _ = tx.send(Ok(Event::default()
                        .json_data(AgentEvent::ToolResult {
                            tool_call_id: tool_id.clone(),
                            content: skip_msg.clone(),
                        })
                        .unwrap()));
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "name": name,
                        "tool_call_id": tool_id,
                        "content": skip_msg,
                    }));
                    ui_tool_calls[ui_tool_idx]["status"] =
                        serde_json::Value::String("done".to_string());
                    ui_tool_calls[ui_tool_idx]["result"] = serde_json::Value::String(skip_msg);
                    persist_agent_history_snapshot(&history_file, &messages).await;
                    persist_agent_ui_snapshot(
                        &ui_history_file,
                        &base_ui_history,
                        &display_message,
                        &uploaded_files,
                        &ui_assistant_content,
                        &ui_tool_calls,
                    )
                    .await;
                    continue;
                }

                total_tool_calls += 1;

                let _ = tx.send(Ok(Event::default()
                    .json_data(AgentEvent::ToolCall {
                        tool_call_id: tool_id.clone(),
                        name: name.clone(),
                        args: args_str.clone(),
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
                                tool_call_id: tool_id.clone(),
                                content: err_msg.clone(),
                            })
                            .unwrap()));
                        messages.push(serde_json::json!({
                            "role": "tool",
                            "name": name,
                            "tool_call_id": tool_id,
                            "content": err_msg,
                        }));
                        ui_tool_calls[ui_tool_idx]["status"] =
                            serde_json::Value::String("done".to_string());
                        ui_tool_calls[ui_tool_idx]["result"] = serde_json::Value::String(err_msg);
                        persist_agent_history_snapshot(&history_file, &messages).await;
                        persist_agent_ui_snapshot(
                            &ui_history_file,
                            &base_ui_history,
                            &display_message,
                            &uploaded_files,
                            &ui_assistant_content,
                            &ui_tool_calls,
                        )
                        .await;
                        continue;
                    }
                };
                //session目录下用每个工具的tool_call_id创建子目录存放结果文件
                let tool_call_dir = session_dir.join(&tool_id);
                if let Err(e) = tokio::fs::create_dir_all(&tool_call_dir).await {
                    let err_msg = format!("Failed to create tool_call dir: {}", e);
                    let _ = tx.send(Ok(Event::default()
                        .json_data(AgentEvent::ToolResult {
                            tool_call_id: tool_id.clone(),
                            content: err_msg.clone(),
                        })
                        .unwrap()));
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "name": name,
                        "tool_call_id": tool_id,
                        "content": err_msg,
                    }));
                    ui_tool_calls[ui_tool_idx]["status"] =
                        serde_json::Value::String("done".to_string());
                    ui_tool_calls[ui_tool_idx]["result"] = serde_json::Value::String(err_msg);
                    persist_agent_history_snapshot(&history_file, &messages).await;
                    persist_agent_ui_snapshot(
                        &ui_history_file,
                        &base_ui_history,
                        &display_message,
                        &uploaded_files,
                        &ui_assistant_content,
                        &ui_tool_calls,
                    )
                    .await;
                    continue;
                }

                let result = if needs_cpu_semaphore(&name) {
                    //和main.rs的task公用同一信号量
                    let semaphore = cpu_task_semaphore.clone();
                    match semaphore.clone().try_acquire_owned() {
                        Ok(permit) => {
                            ui_tool_calls[ui_tool_idx]["status"] =
                                serde_json::Value::String("running".to_string());
                            ui_tool_calls[ui_tool_idx]["waitMessage"] =
                                serde_json::Value::String(String::new());
                            persist_agent_ui_snapshot(
                                &ui_history_file,
                                &base_ui_history,
                                &display_message,
                                &uploaded_files,
                                &ui_assistant_content,
                                &ui_tool_calls,
                            )
                            .await;
                            let _ = tx.send(Ok(Event::default()
                                .json_data(AgentEvent::ToolStatus {
                                    tool_call_id: tool_id.clone(),
                                    status: "running".to_string(),
                                    message: None,
                                })
                                .unwrap()));

                            let res = match execute_tool(&name, args_json, &tool_call_dir).await {
                                Ok(v) => v,
                                Err(err) => format!("Error executing tool: {}", err),
                            };
                            drop(permit);
                            res
                        }
                        Err(TryAcquireError::NoPermits) => {
                            ui_tool_calls[ui_tool_idx]["status"] =
                                serde_json::Value::String("waiting".to_string());
                            ui_tool_calls[ui_tool_idx]["waitMessage"] = serde_json::Value::String(
                                "Tool is waiting for an available worker slot...".to_string(),
                            );
                            persist_agent_ui_snapshot(
                                &ui_history_file,
                                &base_ui_history,
                                &display_message,
                                &uploaded_files,
                                &ui_assistant_content,
                                &ui_tool_calls,
                            )
                            .await;
                            let _ = tx.send(Ok(Event::default()
                                .json_data(AgentEvent::ToolStatus {
                                    tool_call_id: tool_id.clone(),
                                    status: "waiting".to_string(),
                                    message: Some(
                                        "Tool is waiting for an available worker slot..."
                                            .to_string(),
                                    ),
                                })
                                .unwrap()));

                            match semaphore.acquire_owned().await {
                                Ok(permit) => {
                                    ui_tool_calls[ui_tool_idx]["status"] =
                                        serde_json::Value::String("running".to_string());
                                    ui_tool_calls[ui_tool_idx]["waitMessage"] =
                                        serde_json::Value::String(String::new());
                                    persist_agent_ui_snapshot(
                                        &ui_history_file,
                                        &base_ui_history,
                                        &display_message,
                                        &uploaded_files,
                                        &ui_assistant_content,
                                        &ui_tool_calls,
                                    )
                                    .await;
                                    let _ = tx.send(Ok(Event::default()
                                        .json_data(AgentEvent::ToolStatus {
                                            tool_call_id: tool_id.clone(),
                                            status: "running".to_string(),
                                            message: None,
                                        })
                                        .unwrap()));

                                    let res = match execute_tool(&name, args_json, &tool_call_dir)
                                        .await
                                    {
                                        Ok(v) => v,
                                        Err(err) => format!("Error executing tool: {}", err),
                                    };
                                    drop(permit);
                                    res
                                }
                                Err(err) => {
                                    format!("Error acquiring shared cpu semaphore: {}", err)
                                }
                            }
                        }
                        Err(TryAcquireError::Closed) => {
                            "Error acquiring shared cpu semaphore: closed".to_string()
                        }
                    }
                } else {
                    match execute_tool(&name, args_json, &tool_call_dir).await {
                        Ok(res) => res,
                        Err(err) => format!("Error executing tool: {}", err),
                    }
                };

                let clipped_result = truncate_text(&result, agent_config.max_tool_result_chars);

                debug_log(&format!(
                    "Tool finished: name={}, result_len={}",
                    name,
                    clipped_result.len()
                ));

                let _ = tx.send(Ok(Event::default()
                    .json_data(AgentEvent::ToolResult {
                        tool_call_id: tool_id.clone(),
                        content: clipped_result.clone(),
                    })
                    .unwrap()));

                messages.push(serde_json::json!({
                    "role": "tool",
                    "name": name,
                    "tool_call_id": tool_id,
                    "content": clipped_result,
                }));
                ui_tool_calls[ui_tool_idx]["status"] =
                    serde_json::Value::String("done".to_string());
                ui_tool_calls[ui_tool_idx]["result"] = serde_json::Value::String(clipped_result);
                persist_agent_history_snapshot(&history_file, &messages).await;
                persist_agent_ui_snapshot(
                    &ui_history_file,
                    &base_ui_history,
                    &display_message,
                    &uploaded_files,
                    &ui_assistant_content,
                    &ui_tool_calls,
                )
                .await;
            }
        } else {
            debug_log("No tool calls in this step; finishing loop.");
            messages.push(message);
            persist_agent_history_snapshot(&history_file, &messages).await;
            persist_agent_ui_snapshot(
                &ui_history_file,
                &base_ui_history,
                &display_message,
                &uploaded_files,
                &ui_assistant_content,
                &ui_tool_calls,
            )
            .await;
            break;
        }
    }

    let mut history_to_save = if messages.len() > 1 {
        messages[1..].to_vec() //历史记录不保存系统提示，因为系统提示是固定的。
    } else {
        messages.clone()
    };

    let user_turns = count_user_turns(&history_to_save);
    if user_turns > 0
        && user_turns % agent_config.summary_every_user_turns == 0
        && let Some(summary) = generate_history_summary(
            &client,
            &agent_config.summary.model,
            &agent_config.summary.api,
            &api_key,
            &summary_prompt,
            &history_to_save,
        )
        .await
    {
        //
        history_to_save.retain(|m| {
            !(get_role(m) == Some("system")
                && get_content(m)
                    .map(|c| c.starts_with(SUMMARY_MARKER))
                    .unwrap_or(false))
        });
        history_to_save.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": format!("{}\n{}", SUMMARY_MARKER, summary)
            }),
        );
    }

    if let Err(err) = write_history(&history_file, &history_to_save).await {
        debug_log(&format!("Failed to write agent history: {}", err));
        let _ = tx.send(Ok(Event::default()
            .json_data(AgentEvent::Error {
                message: format!("Failed to save chat history: {}", err),
            })
            .unwrap()));
    }

    persist_agent_ui_snapshot(
        &ui_history_file,
        &base_ui_history,
        &display_message,
        &uploaded_files,
        &ui_assistant_content,
        &ui_tool_calls,
    )
    .await;

    if let Some(title_task) = title_task {
        let _ = title_task.await;
    }

    Ok(())
}
