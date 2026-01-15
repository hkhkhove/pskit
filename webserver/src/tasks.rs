use crate::config::Config;
use crate::models::TaskStatus;
use pyo3::{prelude::*, types::PyList};
use sqlx::SqlitePool;
use std::collections::HashMap;

pub async fn process_task(form_data: HashMap<String, String>, db_pool: SqlitePool) {
    let task_id = form_data.get("task_id").unwrap().clone();
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

    //运行python代码
    let result: Result<Result<(), PyErr>, tokio::task::JoinError> =
        tokio::task::spawn_blocking(move || run_pskit(form_data)).await;

    // 根据结果更新数据库
    let end_time = chrono::Utc::now();
    let final_status = match result {
        Ok(Ok(_)) => sqlx::query("UPDATE tasks SET status = ?, end_time = ? WHERE id = ?")
            .bind(TaskStatus::Completed)
            .bind(end_time)
            .bind(&task_id),
        Ok(Err(py_err)) => {
            let error_message = Some(format!("Python execution failed: {}", py_err));
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

pub fn run_pskit(params: HashMap<String, String>) -> PyResult<()> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let path = sys.getattr("path")?;
        let path_list = path.downcast::<PyList>()?;

        let home = Config::home();

        path_list.insert(0, home.join("pskit").as_os_str())?;

        let module = py.import("ai.run_pskit")?;
        module.call_method("main", (params,), None)?;
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_run_pskit() {
        use std::path::PathBuf;

        Config::init(PathBuf::from("/home/zh/p/pskit"), "0.0.0.0:10706".into(), 2).unwrap();

        let mut params: HashMap<String, String> = HashMap::new();

        params.insert(
            "input_dir".into(),
            "/home/zh/p/pskit/tasks/uploads/test".into(),
        );
        params.insert(
            "output_dir".into(),
            "/home/zh/p/pskit/tasks/results/test".into(),
        );

        params.insert("task_id".into(), "test".into());
        params.insert("task_name".into(), "pred_bs".into());
        params.insert("input_method".into(), "id".into());
        params.insert("ids".into(), "".into());

        if let Err(pyerr) = run_pskit(params) {
            print!("{:?}", pyerr);
        }
    }
}
