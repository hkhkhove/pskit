use crate::config::Config;
use crate::models::TaskStatus;
use sqlx::SqlitePool;
use std::process::Command;

pub async fn process_task(task_id: String, db_pool: SqlitePool) {
    // 开始处理任务，更新任务状态为 Processing 并设置 start_time
    let start_time = chrono::Utc::now();
    if let Err(e) = sqlx::query("UPDATE tasks SET status = ?, start_time = ? WHERE id = ?")
        .bind(TaskStatus::Processing)
        .bind(start_time)
        .bind(&task_id)
        .execute(&db_pool)
        .await
    {
        eprintln!("Failed to update task {} to processing: {}", task_id, e);
        return;
    }

    // 运行 Python 脚本
    let task_id_clone = task_id.clone();
    let result: Result<Result<(), String>, tokio::task::JoinError> =
        tokio::task::spawn_blocking(move || run_pskit(&task_id_clone)).await;

    // 根据结果更新数据库
    let end_time = chrono::Utc::now();
    let final_status = match result {
        Ok(Ok(_)) => sqlx::query("UPDATE tasks SET status = ?, end_time = ? WHERE id = ?")
            .bind(TaskStatus::Completed)
            .bind(end_time)
            .bind(&task_id),
        Ok(Err(err_msg)) => {
            let error_message = Some(format!("Python execution failed: {}", err_msg));
            sqlx::query("UPDATE tasks SET status = ?, end_time = ?, error_message = ? WHERE id = ?")
                .bind(TaskStatus::Failed)
                .bind(end_time)
                .bind(error_message)
                .bind(&task_id)
        }
        Err(join_err) => {
            let error_message = Some(format!("Task execution failed: {}", join_err));
            sqlx::query("UPDATE tasks SET status = ?, end_time = ?, error_message = ? WHERE id = ?")
                .bind(TaskStatus::Failed)
                .bind(end_time)
                .bind(error_message)
                .bind(&task_id)
        }
    };

    if let Err(e) = final_status.execute(&db_pool).await {
        eprintln!("Failed to update final status for task {}: {}", task_id, e);
    }
}

pub fn run_pskit(task_id: &str) -> Result<(), String> {
    let home = Config::home();

    let form_data_path = home
        .join("tasks")
        .join("uploads")
        .join(task_id)
        .join("form_data.json");

    if !form_data_path.exists() {
        return Err(format!("form_data.json not found: {:?}", form_data_path));
    }

    // 运行 Python 脚本
    let pskit_dir = home.join("pskit");
    let output = Command::new("python")
        .arg("-m")
        .arg("ai.run_pskit")
        .arg(&form_data_path)
        .current_dir(&pskit_dir)
        .output()
        .map_err(|e| format!("Failed to execute python: {}", e))?;

    // 打印 Python 输出（用于调试）
    if !output.stdout.is_empty() {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Python script exited with code {:?}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_pskit() {
        use std::path::PathBuf;

        Config::init(PathBuf::from("/home/zh/p/pskit"), "0.0.0.0:10706".into(), 2).unwrap();

        if let Err(err) = run_pskit("test") {
            println!("Error: {}", err);
        }
    }
}
