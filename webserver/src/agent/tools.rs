use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use uuid::Uuid;

const HTTP_TIMEOUT_SECS: u64 = 60;
const TOOL_TIMEOUT_SECS: u64 = 1800;

fn validate_agent_input_file_path(
    output_dir: &Path,
    requested_path: &str,
) -> Result<PathBuf, String> {
    let session_dir = output_dir
        .parent()
        .ok_or_else(|| "Invalid tool output directory".to_string())?;
    let session_dir_canonical = session_dir.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve session directory {}: {}",
            session_dir.display(),
            e
        )
    })?;
    let requested_path = Path::new(requested_path);
    let requested_canonical = requested_path.canonicalize().map_err(|e| {
        format!(
            "Invalid input file path {}: {}",
            requested_path.display(),
            e
        )
    })?;

    if !requested_canonical.starts_with(&session_dir_canonical) {
        return Err(format!(
            "Access denied: input file must be under {}",
            session_dir_canonical.display()
        ));
    }

    let metadata = std::fs::metadata(&requested_canonical).map_err(|e| {
        format!(
            "Failed to read input file metadata {}: {}",
            requested_canonical.display(),
            e
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "Invalid input file path {}: expected a regular file",
            requested_canonical.display()
        ));
    }

    Ok(requested_canonical)
}

fn validate_simple_token(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(format!("Invalid {} '{}'", field, value));
    }
    Ok(())
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

#[derive(Debug, Deserialize)]
struct ToolCatalog {
    tools: Vec<ToolDefinition>,
}

#[derive(Debug, Deserialize)]
struct ToolDefinition {
    name: String,
    description: String,
    parameters: toml::Value,
}

fn read_result_file_preview(
    file_path: &Path,
    start_line: usize,
    max_lines: usize,
    max_chars: usize,
) -> Result<String, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|_| "File is not UTF-8 text or cannot be read as text".to_string())?;
    let total_chars = content.chars().count();
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();

    let start_idx = start_line.saturating_sub(1).min(total_lines);
    let end_idx = (start_idx + max_lines).min(total_lines);
    let mut snippet = all_lines[start_idx..end_idx].join("\n");

    let mut truncated_by_chars = false;
    if snippet.chars().count() > max_chars {
        snippet = snippet.chars().take(max_chars).collect::<String>();
        truncated_by_chars = true;
    }

    let truncated_by_lines = end_idx < total_lines;

    Ok(format!(
        "file: {}\nstart_line: {}\nreturned_lines: {}\ntotal_lines: {}\nreturned_chars: {}\ntotal_chars: {}\ntruncated_by_lines: {}\ntruncated_by_chars: {}\n---\n{}",
        file_path.display(),
        start_idx + 1,
        end_idx.saturating_sub(start_idx),
        total_lines,
        snippet.chars().count(),
        total_chars,
        truncated_by_lines,
        truncated_by_chars,
        snippet
    ))
}

pub fn get_tools() -> anyhow::Result<Vec<serde_json::Value>> {
    let config_path = crate::config::Config::home()
        .join("agent_config")
        .join("tools.toml");
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("Failed to read tools config: {}", e))?;
    let catalog: ToolCatalog = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse tools config: {}", e))?;

    let mut tools = Vec::with_capacity(catalog.tools.len());
    for tool in catalog.tools {
        let parameters = serde_json::to_value(tool.parameters)
            .map_err(|e| anyhow::anyhow!("Failed to convert tool parameters to JSON: {}", e))?;
        tools.push(json!({
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description,
                "parameters": parameters,
            }
        }));
    }

    Ok(tools)
}

async fn run_cli_tool(args: &[String]) -> Result<String, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(TOOL_TIMEOUT_SECS),
        Command::new("pskit-cli")
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| format!("pskit-cli timed out after {} seconds", TOOL_TIMEOUT_SECS))?
    .map_err(|e| format!("Failed to execute pskit-cli: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "pskit-cli failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub async fn execute_tool(
    name: &str,
    args: serde_json::Value,
    output_dir: &Path,
) -> Result<String, String> {
    let home = crate::config::Config::home();
    let ai_root = home.join("pskit");

    match name {
        "search_pdb" => {
            let query = args["query"].as_str().ok_or("Missing query")?;
            let body = json!({
                "query": {
                    "type": "terminal",
                    "service": "full_text",
                    "parameters": { "value": query }
                },
                "return_type": "entry",
                "request_options": { "paginate": { "start": 0, "rows": 10 } }
            });

            let client = http_client()?;
            let response = client
                .post("https://search.rcsb.org/rcsbsearch/v2/query")
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !response.status().is_success() {
                return Err(format!("RCSB search failed: {}", response.status()));
            }

            let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            let ids = value
                .get("result_set")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.get("identifier").and_then(|id| id.as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            Ok(format!(
                "Found {} entries. Top hits: {}",
                ids.len(),
                if ids.is_empty() {
                    "(none)".to_string()
                } else {
                    ids.join(", ")
                }
            ))
        }
        "download_pdb_file" => {
            let pdb_id = args["pdb_id"].as_str().ok_or("Missing pdb_id")?;
            let format = args["format"]
                .as_str()
                .unwrap_or("cif")
                .to_ascii_lowercase();
            if format != "cif" && format != "pdb" {
                return Err("format must be one of: cif, pdb".to_string());
            }

            let normalized_id = pdb_id.trim().to_ascii_uppercase();
            if normalized_id.len() != 4 {
                return Err(format!(
                    "Invalid pdb_id '{}': expected 4-character PDB ID",
                    pdb_id
                ));
            }

            let ext = if format == "pdb" { "pdb" } else { "cif" };
            let url = format!("https://files.rcsb.org/download/{}.{}", normalized_id, ext);

            let client = http_client()?;
            let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "Failed to download {} from RCSB: {}",
                    normalized_id,
                    response.status()
                ));
            }

            let bytes = response.bytes().await.map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let output_file =
                output_dir.join(format!("{}.{}", normalized_id.to_ascii_lowercase(), ext));
            std::fs::write(&output_file, &bytes).map_err(|e| e.to_string())?;

            Ok(format!(
                "Downloaded {} ({})\nSaved to: {}",
                normalized_id,
                format,
                output_file.display()
            ))
        }
        "fetch_pdb_info" => {
            let pdb_id = args["pdb_id"].as_str().ok_or("Missing pdb_id")?;
            let url = format!("https://data.rcsb.org/rest/v1/core/entry/{}", pdb_id);
            let client = http_client()?;
            let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if response.status().is_success() {
                let text = response.text().await.map_err(|e| e.to_string())?;
                Ok(text)
            } else {
                Err(format!(
                    "Failed to fetch info for PDB ID {}: {}",
                    pdb_id,
                    response.status()
                ))
            }
        }
        "split_pdb_by_chain" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let format = args["format"]
                .as_str()
                .ok_or("Missing format")?
                .to_ascii_lowercase();
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let out = run_cli_tool(&[
                "split-by-chain".to_string(),
                "-i".to_string(),
                pdb_path.to_string_lossy().to_string(),
                "-F".to_string(),
                format.to_string(),
                "-o".to_string(),
                output_dir.to_string_lossy().to_string(),
            ])
            .await?;
            Ok(format!("{}", out))
        }
        "split_complex" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let format = args["format"]
                .as_str()
                .ok_or("Missing format")?
                .to_ascii_lowercase();
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let out = run_cli_tool(&[
                "split-complex".to_string(),
                "-i".to_string(),
                pdb_path.to_string_lossy().to_string(),
                "-o".to_string(),
                output_dir.to_string_lossy().to_string(),
                "-F".to_string(),
                format.to_string(),
            ])
            .await?;
            Ok(format!("{}", out))
        }
        "extract_fragment" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let chain = args["chain"].as_str().ok_or("Missing chain")?;
            validate_simple_token(chain, "chain")?;
            let format = args["format"]
                .as_str()
                .ok_or("Missing format")?
                .to_ascii_lowercase();
            let filename = pdb_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}", Uuid::new_v4()));
            let output_file = output_dir.join(format!("{}_chain_{}.{}", filename, chain, format));

            let mut cli_args = vec![
                "extract-fragment".to_string(),
                "-i".to_string(),
                pdb_path.to_string_lossy().to_string(),
                "-c".to_string(),
                chain.to_string(),
                "-o".to_string(),
                output_file.to_string_lossy().to_string(),
                "-F".to_string(),
                format.to_string(),
            ];
            if let Some(start) = args["start"].as_i64() {
                cli_args.push("--start".to_string());
                cli_args.push(start.to_string());
            }
            if let Some(end) = args["end"].as_i64() {
                cli_args.push("--end".to_string());
                cli_args.push(end.to_string());
            }
            let out = run_cli_tool(&cli_args).await?;
            Ok(format!("{}", out))
        }
        "extract_sequences" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let format = args["format"]
                .as_str()
                .ok_or("Missing format")?
                .to_ascii_lowercase();
            let filename = pdb_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}", Uuid::new_v4()));
            let output_file = output_dir.join(format!("{}.fasta", filename));

            let cli_args = vec![
                "extract-sequences".to_string(),
                "-i".to_string(),
                pdb_path.to_string_lossy().to_string(),
                "-o".to_string(),
                output_file.to_string_lossy().to_string(),
                "-F".to_string(),
                format.to_string(),
            ];
            let out = run_cli_tool(&cli_args).await?;
            Ok(format!("{}", out))
        }
        "calculate_contact_map" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let filename = pdb_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}", Uuid::new_v4()));
            let output_file = output_dir.join(format!("contact_map_{}.json", filename));
            let mode = args["mode"].as_str().unwrap_or("d");
            let format = args["format"]
                .as_str()
                .ok_or("Missing format")?
                .to_ascii_lowercase();
            let mut cli_args = vec![
                "contact-map".to_string(),
                "-i".to_string(),
                pdb_path.to_string_lossy().to_string(),
                "-o".to_string(),
                output_file.to_string_lossy().to_string(),
                "-m".to_string(),
                mode.to_string(),
                "-F".to_string(),
                format.to_string(),
            ];
            if let Some(chain) = args["chain"].as_str() {
                validate_simple_token(chain, "chain")?;
                cli_args.push("-c".to_string());
                cli_args.push(chain.to_string());
            }
            if let Some(k) = args["k"].as_u64() {
                cli_args.push("--k".to_string());
                cli_args.push(k.to_string());
            }
            let out = run_cli_tool(&cli_args).await?;
            Ok(format!("{}", out))
        }
        "annotate_binding_pairs" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let filename = pdb_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}", Uuid::new_v4()));
            let cutoff = args["cutoff"].as_f64().unwrap_or(3.5);
            let output_file = output_dir.join(format!("{}_binding_pairs.csv", filename));
            let format = args["format"]
                .as_str()
                .ok_or("Missing format")?
                .to_ascii_lowercase();
            let out = run_cli_tool(&[
                "annotate-binding-pairs".to_string(),
                "-i".to_string(),
                pdb_path.to_string_lossy().to_string(),
                "-F".to_string(),
                format.to_string(),
                "-o".to_string(),
                output_file.to_string_lossy().to_string(),
                "--cutoff".to_string(),
                cutoff.to_string(),
            ])
            .await?;
            Ok(format!("{}", out))
        }
        "predict_binding_sites" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let ligand_type = args["ligand_type"].as_str().unwrap_or("DNA");
            if ligand_type != "DNA" && ligand_type != "RNA" {
                return Err("ligand_type must be one of: DNA, RNA".to_string());
            }
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let output = tokio::time::timeout(
                Duration::from_secs(TOOL_TIMEOUT_SECS),
                Command::new("python")
                    .arg("-m")
                    .arg("ai.INABe")
                    .arg("--pdb_path")
                    .arg(&pdb_path)
                    .arg("--output_dir")
                    .arg(&output_dir)
                    .arg("--ligand_type")
                    .arg(ligand_type)
                    .current_dir(ai_root.clone())
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .map_err(|_| {
                format!(
                    "predict_binding_sites timed out after {} seconds",
                    TOOL_TIMEOUT_SECS
                )
            })?
            .map_err(|e| format!("Failed to execute predict_binding_sites: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                if stderr.is_empty() {
                    Ok(stdout)
                } else {
                    Ok(format!("{}\n[stderr]\n{}", stdout, stderr))
                }
            } else {
                Err(format!(
                    "predict_binding_sites failed\nstdout:\n{}\nstderr:\n{}",
                    stdout, stderr
                ))
            }
        }
        "extract_empirical_features" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let pdb_path = validate_agent_input_file_path(output_dir, pdb_path)?;
            let emp_feats = args["emp_feats"].as_str().unwrap_or("dssp");
            let rosetta_relax = args["rosetta_relax"].as_str().unwrap_or("false");
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let output = tokio::time::timeout(
                Duration::from_secs(TOOL_TIMEOUT_SECS),
                Command::new("python")
                    .arg("-m")
                    .arg("ai.feature.empirical_feats")
                    .arg("--pdb_path")
                    .arg(&pdb_path)
                    .arg("--output_dir")
                    .arg(&output_dir)
                    .arg("--emp_feats")
                    .arg(emp_feats)
                    .arg("--rosetta_relax")
                    .arg(rosetta_relax)
                    .current_dir(ai_root.clone())
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .map_err(|_| {
                format!(
                    "extract_empirical_features timed out after {} seconds",
                    TOOL_TIMEOUT_SECS
                )
            })?
            .map_err(|e| format!("Failed to execute extract_empirical_features: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                if stderr.is_empty() {
                    Ok(stdout)
                } else {
                    Ok(format!("{}\n[stderr]\n{}", stdout, stderr))
                }
            } else {
                Err(format!(
                    "extract_empirical_features failed\nstdout:\n{}\nstderr:\n{}",
                    stdout, stderr
                ))
            }
        }
        "predict_interaction" => {
            let protein = args["protein_seq"]
                .as_str()
                .ok_or("Missing protein_sequence")?;
            let nucleic = args["nucleic_acid_seq"]
                .as_str()
                .ok_or("Missing nucleic_acid_sequence")?;
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let output = tokio::time::timeout(
                Duration::from_secs(TOOL_TIMEOUT_SECS),
                Command::new("python")
                    .arg("-m")
                    .arg("ai.PAIR")
                    .arg("--protein_seq")
                    .arg(protein)
                    .arg("--nucleic_acid_seq")
                    .arg(nucleic)
                    .arg("--output_dir")
                    .arg(&output_dir)
                    .current_dir(ai_root)
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .map_err(|_| {
                format!(
                    "predict_interaction timed out after {} seconds",
                    TOOL_TIMEOUT_SECS
                )
            })?
            .map_err(|e| format!("Failed to execute predict_interaction: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                if stderr.is_empty() {
                    Ok(stdout)
                } else {
                    Ok(format!("{}\n[stderr]\n{}", stdout, stderr))
                }
            } else {
                Err(format!(
                    "predict_interaction failed\nstdout:\n{}\nstderr:\n{}",
                    stdout, stderr
                ))
            }
        }
        "read_result_file" => {
            let file_path = args["file_path"].as_str().ok_or("Missing file_path")?;
            let start_line = args["start_line"]
                .as_u64()
                .map(|v| v as usize)
                .unwrap_or(1)
                .max(1);
            let max_lines = args["max_lines"]
                .as_u64()
                .map(|v| v as usize)
                .unwrap_or(1000)
                .clamp(1, 2000);
            let max_chars = args["max_chars"]
                .as_u64()
                .map(|v| v as usize)
                .unwrap_or(12_000)
                .clamp(200, 120_000);

            let safe_file_path = validate_agent_input_file_path(output_dir, file_path)?;
            read_result_file_preview(&safe_file_path, start_line, max_lines, max_chars)
        }
        _ => Err(format!("Unknown tool: {}", name)),
    }
}
