use axum::{
    Json,
    extract::{Path as PathExtractor, Query},
    http::{StatusCode, header::CONTENT_TYPE},
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct SessionDownloadFile {
    pub content: Vec<u8>,
    pub filename: String,
    pub content_type: &'static str,
}
#[derive(Serialize)]
struct AgentSessionFile {
    path: String,
    tool_call_id: Option<String>,
    filename: String,
    size: u64,
    download_url: String,
}

#[derive(Serialize)]
pub struct AgentSessionFilesResponse {
    session_id: String,
    total_files: usize,
    files: Vec<AgentSessionFile>,
}

#[derive(Serialize)]
pub struct DeleteSessionResponse {
    session_id: String,
    deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListSessionFilesQuery {
    pub tool_call_id: Option<String>,
}

fn is_valid_tool_call_id(tool_call_id: &str) -> bool {
    !tool_call_id.is_empty()
        && tool_call_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_valid_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

pub fn build_session_dir(home: &Path, session_id: &str) -> PathBuf {
    home.join("tasks").join("agent_sessions").join(session_id)
}

pub fn history_file(session_dir: &Path) -> PathBuf {
    session_dir.join("history.json")
}

pub async fn read_history(history_file: &Path) -> Result<Vec<Value>, String> {
    if !history_file.exists() {
        return Ok(Vec::new());
    }

    let content = tokio::fs::read_to_string(history_file)
        .await
        .map_err(|e| format!("Failed to read history: {}", e))?;

    serde_json::from_str::<Vec<Value>>(&content)
        .map_err(|e| format!("Failed to parse history: {}", e))
}

pub async fn write_history(history_file: &Path, history: &[Value]) -> Result<(), String> {
    let history_json = serde_json::to_string_pretty(history)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;
    tokio::fs::write(history_file, history_json)
        .await
        .map_err(|e| format!("Failed to write history: {}", e))
}

pub async fn collect_session_files(session_dir: &Path) -> Result<Vec<(PathBuf, u64)>, String> {
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

fn content_type_for_path(path: &Path) -> &'static str {
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

pub async fn read_session_file_for_download(
    session_dir: &Path,
    relative_path: &str,
) -> Result<SessionDownloadFile, (StatusCode, String)> {
    let file_path = session_dir.join(relative_path);

    if !file_path.exists() {
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    let session_dir_canonical = tokio::fs::canonicalize(session_dir)
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

    Ok(SessionDownloadFile {
        content: file_content,
        filename,
        content_type: content_type_for_path(&file_path_canonical),
    })
}

//前端在得到工具调用结果后，访问/sessions/{session_id}/files接口获取文件列表
pub async fn list_session_files(
    PathExtractor(session_id): PathExtractor<String>,
    Query(query): Query<ListSessionFilesQuery>,
) -> Result<Json<AgentSessionFilesResponse>, (StatusCode, String)> {
    let home = crate::config::Config::home();
    let session_dir = build_session_dir(&home, &session_id);

    let tool_call_id_filter = query
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(tool_call_id) = tool_call_id_filter
        && !is_valid_tool_call_id(tool_call_id)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid tool_call_id query parameter".to_string(),
        ));
    }

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
            let parts = rel_str.split('/').collect::<Vec<_>>();
            if parts.len() != 2 {
                return None;
            }

            let tool_call_id_part = parts[0];
            let filename_part = parts[1];

            if !is_valid_tool_call_id(tool_call_id_part) || !is_valid_filename(filename_part) {
                return None;
            }

            if let Some(tool_call_id) = tool_call_id_filter {
                if tool_call_id_part != tool_call_id {
                    return None;
                }
            }

            let tool_call_id = Some(tool_call_id_part.to_string());
            let filename = filename_part.to_string();
            let download_url = format!(
                "/api/agent/sessions/{}/{}/{}",
                session_id, tool_call_id_part, filename_part
            );

            Some(AgentSessionFile {
                path: rel_str.clone(),
                tool_call_id,
                filename,
                size,
                download_url,
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

pub async fn download_session_tool_file(
    PathExtractor((session_id, tool_call_id, filename)): PathExtractor<(String, String, String)>,
) -> Result<Response, (StatusCode, String)> {
    if !is_valid_tool_call_id(&tool_call_id) {
        return Err((StatusCode::BAD_REQUEST, "Invalid tool_call_id".to_string()));
    }
    if !is_valid_filename(&filename) {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename".to_string()));
    }

    let home = crate::config::Config::home();
    let session_dir = build_session_dir(&home, &session_id);
    let relative_path = format!("{}/{}", tool_call_id, filename);
    let downloadable = read_session_file_for_download(&session_dir, &relative_path).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, downloadable.content_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", downloadable.filename),
        )
        .body(downloadable.content.into())
        .unwrap())
}

pub async fn get_session_history(
    PathExtractor(session_id): PathExtractor<String>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let home = crate::config::Config::home();
    let session_dir = build_session_dir(&home, &session_id);
    let history_file = history_file(&session_dir);
    let history = read_history(&history_file)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(history))
}

pub async fn delete_session(
    PathExtractor(session_id): PathExtractor<String>,
) -> Result<Json<DeleteSessionResponse>, (StatusCode, String)> {
    let home = crate::config::Config::home();
    let session_dir = build_session_dir(&home, &session_id);

    if !session_dir.exists() {
        return Ok(Json(DeleteSessionResponse {
            session_id,
            deleted: false,
        }));
    }

    tokio::fs::remove_dir_all(&session_dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete session: {}", e),
        )
    })?;

    Ok(Json(DeleteSessionResponse {
        session_id,
        deleted: true,
    }))
}
