use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{Json, Response},
    routing::{get, post},
};
use sqlx::SqlitePool;
use std::{collections::HashMap, env, path::PathBuf, sync::Arc};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{Mutex, Semaphore},
};
use tower_http::services::{ServeDir, ServeFile};

mod config;
mod database;
mod models;
mod tasks;

use config::Config;
use models::{File, Task, TaskCreateResponse, TaskResponse, TaskResultsResponse, TaskStatus};

#[derive(Clone)]
struct AppState {
    cpu_task_semaphore: Arc<Semaphore>,
    db_pool: SqlitePool,
    task_sender: tokio::sync::mpsc::UnboundedSender<String>,
    task_queue: Arc<Mutex<std::collections::VecDeque<String>>>,
}

// API处理函数
async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "message": "PSKit API Server",
        "version": "0.1.0"
    }))
}

// 接收用户上传的任务，添加到任务队列
// 如果 Ok(Json(...))，就返回一个 200 状态码、JSON 格式的 HTTP 响应
// 如果 Err((StatusCode, String))，axum 会自动把这个元组转换成 HTTP 响应
async fn upload_task(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<TaskCreateResponse>, (StatusCode, String)> {
    let mut form_data = HashMap::new();
    let mut file_fields = Vec::new(); // 暂存文件数据

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let field_name = field.name().unwrap_or("unknown").to_string();

        if let Some(filename) = field.file_name() {
            let filename = filename.to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            file_fields.push((field_name, filename, data));
        } else {
            let value = field
                .text()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            form_data.insert(field_name, value);
        }
    }
    let task_id = form_data
        .get("task_id")
        .ok_or((StatusCode::BAD_REQUEST, "task_id is required".to_string()))?
        .clone();
    let task_name = form_data
        .get("task_name")
        .ok_or((StatusCode::BAD_REQUEST, "task_name is required".to_string()))?
        .clone();

    let home = Config::home();

    let upload_dir = home.join("tasks").join("uploads").join(task_id.to_string());
    let results_dir = home.join("tasks").join("results").join(task_id.to_string());

    fs::create_dir_all(&upload_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::create_dir_all(&results_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut uploaded_files = Vec::new();
    for (_field_name, filename, data) in file_fields {
        let file_path = upload_dir.join(&filename);
        let mut file = fs::File::create(&file_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        file.write_all(&data)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        uploaded_files.push(filename);
    }

    //把task的信息保存到数据库中
    sqlx::query(r#"INSERT INTO tasks (id, name, status, upload_time) VALUES (?, ?, ?, ?)"#)
        .bind(&task_id)
        .bind(task_name)
        .bind(TaskStatus::Pending)
        .bind(chrono::Utc::now())
        .execute(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    form_data.insert(
        "input_dir".to_string(),
        upload_dir.to_string_lossy().to_string(),
    );
    form_data.insert(
        "output_dir".to_string(),
        results_dir.to_string_lossy().to_string(),
    );

    //保存form_data
    let form_data_path = home
        .join("tasks")
        .join("uploads")
        .join(task_id.to_string())
        .join("form_data.json");
    let form_data_json = serde_json::to_string(&form_data).unwrap();
    fs::write(form_data_path, form_data_json)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 添加task到队列
    state.task_queue.lock().await.push_back(task_id.clone());

    // 通过通道通知调度器有新任务
    if let Err(_) = state.task_sender.send(task_id.clone()) {
        eprintln!("Failed to send task {} to dispatcher", task_id);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to queue task".to_string(),
        ));
    }

    Ok(Json(TaskCreateResponse {
        task_id,
        message: "Task created successfully and added to the queue.".to_string(),
    }))
}

// 获取特定任务状态
async fn get_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResponse>, (StatusCode, String)> {
    let task: Task = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(&task_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Task not found".to_string()))?;

    let response = match task.status {
        TaskStatus::Pending => {
            let queue = state.task_queue.lock().await;
            let position = queue
                .iter()
                .position(|id| id == &task_id)
                .map(|p| p + 1 + Config::workers());
            TaskResponse::Pending {
                upload_time: task.upload_time,
                start_time: "".to_string(),
                end_time: "".to_string(),
                position: position,
            }
        }
        TaskStatus::Processing => TaskResponse::Processing {
            upload_time: task.upload_time,
            start_time: task.start_time,
            end_time: "".to_string(),
        },
        TaskStatus::Completed => TaskResponse::Completed {
            upload_time: task.upload_time,
            start_time: task.start_time,
            end_time: task.end_time,
        },
        TaskStatus::Failed => TaskResponse::Failed {
            upload_time: task.upload_time,
            start_time: task.start_time,
            end_time: task.end_time,
            error: task
                .error_message
                .clone()
                .unwrap_or_else(|| "Unknown error".to_string()),
        },
    };

    Ok(Json(response))
}

async fn get_results(
    Path(task_id): Path<String>,
) -> Result<Json<TaskResultsResponse>, (StatusCode, String)> {
    let home = Config::home();
    let results_dir = home.join("tasks").join("results").join(&task_id);

    // 检查文件是否存在
    if !std::path::Path::new(&results_dir).exists() {
        return Err((StatusCode::NOT_FOUND, "Task not found".to_string()));
    }

    let mut files = Vec::new();

    let mut entries: fs::ReadDir = fs::read_dir(&results_dir).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read results directory".to_string(),
        )
    })?;

    while let Some(entry) = entries.next_entry().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read directory entry".to_string(),
        )
    })? {
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                let metadata = entry.metadata().await.map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to read file metadata".to_string(),
                    )
                })?;

                files.push(File {
                    filename: filename.to_string(),
                    size: metadata.len(),
                    download_url: format!("/api/tasks/{}/results/{}", task_id, filename),
                });
            }
        }
    }

    // 返回文件列表
    Ok(Json(TaskResultsResponse {
        task_id,
        total_files: files.len(),
        files,
    }))
}

async fn download_result_file(
    Path((task_id, filename)): Path<(String, String)>,
) -> Result<Response, (StatusCode, String)> {
    let home = Config::home();
    let file_path = home
        .join("tasks")
        .join("results")
        .join(&task_id)
        .join(&filename);

    // 安全检查：确保文件路径在结果目录内
    let results_dir = home.join("tasks").join("results").join(&task_id);
    if !file_path.starts_with(&results_dir) {
        return Err((StatusCode::BAD_REQUEST, "Invalid file path".to_string()));
    }

    if !file_path.exists() {
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    let file_content = fs::read(&file_path).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read file".to_string(),
        )
    })?;

    // 根据文件扩展名设置 MIME 类型
    let content_type = match file_path.extension().and_then(|ext| ext.to_str()) {
        Some("txt") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        Some("npy") => "application/octet-stream",
        Some("dssp") => "text/plain; charset=utf-8",
        Some("log") => "text/plain; charset=utf-8",
        Some("pdb") => "chemical/x-pdb",
        Some("cif") => "chemical/x-cif",
        _ => "application/octet-stream",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(file_content.into())
        .unwrap())
}

// 任务调度器
async fn task_dispatcher(
    mut task_receiver: tokio::sync::mpsc::UnboundedReceiver<String>,
    db_pool: SqlitePool,
    cpu_task_semaphore: Arc<Semaphore>,
    task_queue: Arc<Mutex<std::collections::VecDeque<String>>>,
) {
    while let Some(task_id) = task_receiver.recv().await {
        // 获取信号量许可
        let permit = cpu_task_semaphore.clone().acquire_owned().await.unwrap();

        {
            // //将当前task移出队列
            // let mut queue = task_queue.lock().await;
            // if let Some(pos) = queue.iter().position(|id| id == &task_id) {
            //     queue.remove(pos);
            // }
            // 直接移除队列首部任务
            let mut queue = task_queue.lock().await;
            if let Some(front_task) = queue.pop_front() {
                // 验证是否是期望的任务
                if front_task != task_id {
                    eprintln!("Warning: Expected task {}, but got {}", task_id, front_task);
                }
            }
        }

        let pool_clone = db_pool.clone();

        tokio::spawn(async move {
            tasks::process_task(task_id, pool_clone).await;
            drop(permit);
        });
    }

    println!("Task dispatcher stopped");
}

#[tokio::main]
async fn main() {
    println!("Usage: pskit-webserver <work_dir> <address> <max_workers>");
    let home = env::args()
        .nth(1)
        .map(|arg| PathBuf::from(arg))
        .unwrap_or_else(|| PathBuf::from("./"));

    let addr = env::args().nth(2).unwrap_or("127.0.0.1:10706".to_string());

    let workers = env::args()
        .nth(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2);

    if let Err(e) = Config::init(home.clone(), addr.clone(), workers) {
        eprintln!("Failed to initialize config: {}", e);
        std::process::exit(1);
    }

    println!("Using work directory: {}", Config::home().display());
    println!("Server will start on: {}", Config::addr());

    let tasks_dir = home.join("tasks");
    fs::create_dir_all(&tasks_dir)
        .await
        .expect("Failed to create tasks directory");

    let database_url = format!("sqlite:{}", home.join("tasks").join("tasks.db").display());

    // 初始化数据库
    let db_pool = database::init_db(&database_url)
        .await
        .expect("Failed to initialize database");

    // 创建任务通道
    let (task_sender, task_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();

    // 初始化 AppState
    let app_state = AppState {
        cpu_task_semaphore: Arc::new(Semaphore::new(Config::workers())),
        db_pool: db_pool.clone(),
        task_sender,
        task_queue: Arc::new(Mutex::new(std::collections::VecDeque::new())),
    };

    //创建并启动任务调度器
    let cpu_task_semaphore = app_state.cpu_task_semaphore.clone();
    let task_queue = app_state.task_queue.clone();
    tokio::spawn(task_dispatcher(
        task_receiver,
        db_pool,
        cpu_task_semaphore,
        task_queue,
    ));

    let api_routes = Router::new()
        .route("/", get(root))
        .route("/tasks", post(upload_task))
        .route("/tasks/{task_id}", get(get_task_status))
        .route("/tasks/{task_id}/results", get(get_results))
        .route(
            "/tasks/{task_id}/results/{filename}",
            get(download_result_file),
        )
        .layer(DefaultBodyLimit::max(250 * 1024 * 1024)) // 250 MB limit for file uploads
        .with_state(app_state);

    let app = Router::new()
        .nest("/api", api_routes)
        .nest_service(
            "/favicon.svg",
            ServeFile::new(home.join("webpage").join("dist").join("favicon.svg")),
        )
        .nest_service(
            "/assets",
            ServeDir::new(home.join("webpage").join("dist").join("assets")),
        )
        .nest_service(
            "/vendor",
            ServeDir::new(home.join("webpage").join("dist").join("vendor")),
        )
        .fallback_service(ServeFile::new(
            home.join("webpage").join("dist").join("index.html"),
        ));

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
