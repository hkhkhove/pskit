use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

// 对应数据库中的结构
// FromRow是sqlx的宏，允许sqlx自动从数据库中查询出一行数据，自动匹配字段得到一个Task结构体
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    pub upload_time: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, sqlx::Type, PartialEq, Serialize)]
//sqlx::Type 告诉 SQLx 如何在 Rust 类型和数据库类型之间转换
//PartialEq 允许使用 == 和 != 比较枚举值
#[sqlx(type_name = "TEXT")] //表示这个枚举在数据库存储为TEXT类型
pub enum TaskStatus {
    Pending, //存储为“Pending”，以下同理
    Processing,
    Completed,
    Failed,
}

//upload_task的返回类型
#[derive(Serialize)]
pub struct TaskCreateResponse {
    pub task_id: String,
    pub message: String,
}

// get_task_status的返回类型
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum TaskResponse {
    Pending {
        upload_time: DateTime<Utc>,
        start_time: String,
        end_time: String,
        position: Option<usize>,
    },
    Processing {
        upload_time: DateTime<Utc>,
        start_time: Option<DateTime<Utc>>,
        end_time: String,
    },
    Completed {
        upload_time: DateTime<Utc>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    },
    Failed {
        upload_time: DateTime<Utc>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        error: String,
    },
}

#[derive(Serialize)]
pub struct File {
    pub filename: String,
    pub size: u64,
    pub download_url: String,
}

//get_results的返回类型
#[derive(Serialize)]
pub struct TaskResultsResponse {
    pub task_id: String,
    pub files: Vec<File>,
    pub total_files: usize,
}
