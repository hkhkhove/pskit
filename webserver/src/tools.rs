use async_openai::types::chat::{ChatCompletionTool, ChatCompletionTools, FunctionObject};
use serde_json::json;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

fn resolve_session_file_path(
    session_dir: &Path,
    file_path: &str,
) -> Result<std::path::PathBuf, String> {
    let candidate = Path::new(file_path);
    let full_path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        session_dir.join(candidate)
    };

    if !full_path.exists() {
        return Err(format!("File not found: {}", full_path.display()));
    }

    let session_dir_canonical = std::fs::canonicalize(session_dir).map_err(|e| e.to_string())?;
    let file_canonical = std::fs::canonicalize(&full_path).map_err(|e| e.to_string())?;
    if !file_canonical.starts_with(&session_dir_canonical) {
        return Err("Access denied: file must be inside current session directory".to_string());
    }

    Ok(file_canonical)
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

pub fn get_tools() -> Vec<ChatCompletionTools> {
    vec![
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "search_pdb".to_string(),
                description: Some(
                    "Search the RCSB PDB for protein/nucleic acid structures using natural language keywords."
                        .to_string(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Keywords to search for, e.g., 'human p53 DNA complex'"
                        }
                    },
                    "required": ["query"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "download_pdb_file".to_string(),
                description: Some(
                    "Download a structure file from RCSB by PDB ID and save it into the current agent session."
                        .to_string(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_id": {
                            "type": "string",
                            "description": "4-character PDB ID, e.g., '8W2S'"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["cif", "pdb"],
                            "description": "File format to download. Defaults to cif."
                        }
                    },
                    "required": ["pdb_id"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "fetch_pdb_info".to_string(),
                description: Some(
                    "Get metadata for a specific PDB ID (resolution, species, chain entities, etc.)."
                        .to_string(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_id": {
                            "type": "string",
                            "description": "4-character PDB ID, e.g., '1TUP'"
                        }
                    },
                    "required": ["pdb_id"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "split_pdb_by_chain".to_string(),
                description: Some("Split a PDB/mmCIF file into separate chain files.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_path": { "type": "string" }
                    },
                    "required": ["pdb_path"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "split_complex".to_string(),
                description: Some("Split complex parts using pskit-cli split-complex.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_path": { "type": "string" }
                    },
                    "required": ["pdb_path"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "extract_fragment".to_string(),
                description: Some("Extract a chain fragment from a structure file.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_path": { "type": "string" },
                        "chain": { "type": "string" },
                        "start": { "type": "integer" },
                        "end": { "type": "integer" }
                    },
                    "required": ["pdb_path", "chain"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "calculate_contact_map".to_string(),
                description: Some("Calculate contact map JSON from structure.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_path": { "type": "string" },
                        "chain": { "type": "string" },
                        "mode": { "type": "string", "enum": ["d", "d2", "knn"] },
                        "k": { "type": "integer" }
                    },
                    "required": ["pdb_path"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "annotate_binding_pairs".to_string(),
                description: Some("Annotate protein-nucleic acid binding residue pairs.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_path": { "type": "string" },
                        "cutoff": { "type": "number" }
                    },
                    "required": ["pdb_path"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "predict_binding_sites".to_string(),
                description: Some("Predict DNA/RNA binding sites using PSKit INABe model.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_id_or_path": { "type": "string" },
                        "ligand_type": { "type": "string", "enum": ["DNA", "RNA"] }
                    },
                    "required": ["pdb_id_or_path", "ligand_type"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "extract_language_model_features".to_string(),
                description: Some("Extract ESM2/SaProt features via run_pskit lm_embed.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_path": { "type": "string" },
                        "model_type": { "type": "string", "enum": ["esm2", "saprot", "both"] }
                    },
                    "required": ["pdb_path", "model_type"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "extract_empirical_features".to_string(),
                description: Some("Extract empirical features via run_pskit emp_feats.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "pdb_id_or_path": { "type": "string" },
                        "emp_feats": { "type": "string", "description": "comma-separated features, e.g. dssp" },
                        "rosetta_relax": { "type": "string", "enum": ["true", "false"] }
                    },
                    "required": ["pdb_id_or_path"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "read_result_file".to_string(),
                description: Some(
                    "Read a result text file with token-safe truncation (line range + char limit)."
                        .to_string(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to result file (relative to current session or absolute inside current session)."
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "1-based starting line number. Default: 1"
                        },
                        "max_lines": {
                            "type": "integer",
                            "description": "Maximum number of lines to read. Default: 120"
                        },
                        "max_chars": {
                            "type": "integer",
                            "description": "Maximum number of characters to return. Default: 12000"
                        }
                    },
                    "required": ["file_path"]
                })),
                ..Default::default()
            },
        }),
        ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObject {
                name: "predict_interaction".to_string(),
                description: Some("Predict protein-nucleic acid interaction from sequence pairs.".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "protein_sequence": { "type": "string" },
                        "nucleic_sequence": { "type": "string" }
                    },
                    "required": ["protein_sequence", "nucleic_sequence"]
                })),
                ..Default::default()
            },
        }),
    ]
}

fn run_cli_tool(toolkit_dir: &Path, args: &[String]) -> Result<String, String> {
    let output = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("pskit-cli")
        .arg("--")
        .args(args)
        .current_dir(toolkit_dir)
        .output()
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

fn run_python_pskit(
    ai_root: &Path,
    params: &serde_json::Value,
    params_file: &Path,
) -> Result<String, String> {
    if let Some(parent) = params_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        params_file,
        serde_json::to_vec_pretty(params).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let output = Command::new("python")
        .arg("-m")
        .arg("ai.run_pskit")
        .arg(params_file)
        .current_dir(ai_root)
        .output()
        .map_err(|e| format!("Failed to execute python run_pskit: {}", e))?;

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
            "run_pskit failed\nstdout:\n{}\nstderr:\n{}",
            stdout, stderr
        ))
    }
}

fn copy_or_set_id_input(
    pdb_id_or_path: &str,
    input_dir: &Path,
) -> Result<(String, String), String> {
    let maybe_path = Path::new(pdb_id_or_path);
    if maybe_path.exists() {
        std::fs::create_dir_all(input_dir).map_err(|e| e.to_string())?;
        let filename = maybe_path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or("Invalid input file name")?
            .to_string();
        let dest = input_dir.join(filename);
        std::fs::copy(maybe_path, &dest).map_err(|e| e.to_string())?;
        Ok(("file".to_string(), "".to_string()))
    } else {
        Ok(("id".to_string(), pdb_id_or_path.to_string()))
    }
}

pub async fn execute_tool(
    name: &str,
    args: serde_json::Value,
    session_dir: &Path,
) -> Result<String, String> {
    let home = crate::config::Config::home();
    let toolkit_dir = home.join("pskit").join("toolkit");
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

            let client = reqwest::Client::new();
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

            let response = reqwest::get(&url).await.map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "Failed to download {} from RCSB: {}",
                    normalized_id,
                    response.status()
                ));
            }

            let bytes = response.bytes().await.map_err(|e| e.to_string())?;
            let output_dir = session_dir.join("downloads");
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
            let response = reqwest::get(&url).await.map_err(|e| e.to_string())?;
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
            let output_dir = session_dir.join("split_chains");
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let out = run_cli_tool(
                &toolkit_dir,
                &[
                    "split-by-chain".to_string(),
                    "-i".to_string(),
                    pdb_path.to_string(),
                    "-o".to_string(),
                    output_dir.to_string_lossy().to_string(),
                ],
            )?;
            Ok(format!("{}\nOutput dir: {}", out, output_dir.display()))
        }
        "split_complex" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let output_dir = session_dir.join("split_complex");
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
            let out = run_cli_tool(
                &toolkit_dir,
                &[
                    "split-complex".to_string(),
                    "-i".to_string(),
                    pdb_path.to_string(),
                    "-o".to_string(),
                    output_dir.to_string_lossy().to_string(),
                ],
            )?;
            Ok(format!("{}\nOutput dir: {}", out, output_dir.display()))
        }
        "extract_fragment" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let chain = args["chain"].as_str().ok_or("Missing chain")?;
            let output = session_dir.join(format!("fragment_{}_{}.cif", chain, Uuid::new_v4()));
            let mut cli_args = vec![
                "extract-fragment".to_string(),
                "-i".to_string(),
                pdb_path.to_string(),
                "-c".to_string(),
                chain.to_string(),
                "-o".to_string(),
                output.to_string_lossy().to_string(),
            ];
            if let Some(start) = args["start"].as_i64() {
                cli_args.push("--start".to_string());
                cli_args.push(start.to_string());
            }
            if let Some(end) = args["end"].as_i64() {
                cli_args.push("--end".to_string());
                cli_args.push(end.to_string());
            }
            let out = run_cli_tool(&toolkit_dir, &cli_args)?;
            Ok(format!("{}\nOutput file: {}", out, output.display()))
        }
        "calculate_contact_map" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let output = session_dir.join(format!("contact_map_{}.json", Uuid::new_v4()));
            let mode = args["mode"].as_str().unwrap_or("d");
            let mut cli_args = vec![
                "contact-map".to_string(),
                "-i".to_string(),
                pdb_path.to_string(),
                "-o".to_string(),
                output.to_string_lossy().to_string(),
                "-m".to_string(),
                mode.to_string(),
            ];
            if let Some(chain) = args["chain"].as_str() {
                cli_args.push("-c".to_string());
                cli_args.push(chain.to_string());
            }
            if let Some(k) = args["k"].as_u64() {
                cli_args.push("--k".to_string());
                cli_args.push(k.to_string());
            }
            let out = run_cli_tool(&toolkit_dir, &cli_args)?;
            Ok(format!("{}\nOutput file: {}", out, output.display()))
        }
        "annotate_binding_pairs" => {
            let pdb_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let cutoff = args["cutoff"].as_f64().unwrap_or(3.5);
            let output = session_dir.join(format!("binding_pairs_{}.tsv", Uuid::new_v4()));
            let format = Path::new(pdb_path)
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .and_then(|ext| match ext.as_str() {
                    "cif" => Some("cif"),
                    "pdb" => Some("pdb"),
                    _ => None,
                })
                .unwrap_or("auto");
            let out = run_cli_tool(
                &toolkit_dir,
                &[
                    "annotate-binding-pairs".to_string(),
                    "-i".to_string(),
                    pdb_path.to_string(),
                    "-F".to_string(),
                    format.to_string(),
                    "-o".to_string(),
                    output.to_string_lossy().to_string(),
                    "--cutoff".to_string(),
                    cutoff.to_string(),
                ],
            )?;
            Ok(format!("{}\nOutput file: {}", out, output.display()))
        }
        "predict_binding_sites" => {
            let pdb_id_or_path = args["pdb_id_or_path"]
                .as_str()
                .ok_or("Missing pdb_id_or_path")?;
            let ligand_type = args["ligand_type"].as_str().unwrap_or("DNA");
            let task_root = session_dir.join(format!("pred_nbs_{}", Uuid::new_v4()));
            let input_dir = task_root.join("input");
            let output_dir = task_root.join("output");
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

            let (input_method, ids) = copy_or_set_id_input(pdb_id_or_path, &input_dir)?;
            let params = json!({
                "task_id": Uuid::new_v4().to_string(),
                "task_name": "pred_nbs",
                "input_method": input_method,
                "ids": ids,
                "input_dir": input_dir.to_string_lossy().to_string(),
                "output_dir": output_dir.to_string_lossy().to_string(),
                "ligand_type": ligand_type,
            });
            let params_file = task_root.join("params.json");
            let out = run_python_pskit(&ai_root, &params, &params_file)?;
            Ok(format!("{}\nOutput dir: {}", out, output_dir.display()))
        }
        "extract_language_model_features" => {
            let pdb_id_or_path = args["pdb_path"].as_str().ok_or("Missing pdb_path")?;
            let model_type = args["model_type"].as_str().unwrap_or("esm2");
            let task_root = session_dir.join(format!("lm_embed_{}", Uuid::new_v4()));
            let input_dir = task_root.join("input");
            let output_dir = task_root.join("output");
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

            let (input_method, ids) = copy_or_set_id_input(pdb_id_or_path, &input_dir)?;
            let params = json!({
                "task_id": Uuid::new_v4().to_string(),
                "task_name": "lm_embed",
                "input_method": input_method,
                "ids": ids,
                "input_dir": input_dir.to_string_lossy().to_string(),
                "output_dir": output_dir.to_string_lossy().to_string(),
                "model_type": model_type,
            });
            let params_file = task_root.join("params.json");
            let out = run_python_pskit(&ai_root, &params, &params_file)?;
            Ok(format!("{}\nOutput dir: {}", out, output_dir.display()))
        }
        "extract_empirical_features" => {
            let pdb_id_or_path = args["pdb_id_or_path"]
                .as_str()
                .ok_or("Missing pdb_id_or_path")?;
            let emp_feats = args["emp_feats"].as_str().unwrap_or("dssp");
            let rosetta_relax = args["rosetta_relax"].as_str().unwrap_or("false");
            let task_root = session_dir.join(format!("emp_feats_{}", Uuid::new_v4()));
            let input_dir = task_root.join("input");
            let output_dir = task_root.join("output");
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

            let (input_method, ids) = copy_or_set_id_input(pdb_id_or_path, &input_dir)?;
            let params = json!({
                "task_id": Uuid::new_v4().to_string(),
                "task_name": "emp_feats",
                "input_method": input_method,
                "ids": ids,
                "input_dir": input_dir.to_string_lossy().to_string(),
                "output_dir": output_dir.to_string_lossy().to_string(),
                "emp_feats": emp_feats,
                "rosetta_relax": rosetta_relax,
            });
            let params_file = task_root.join("params.json");
            let out = run_python_pskit(&ai_root, &params, &params_file)?;
            Ok(format!("{}\nOutput dir: {}", out, output_dir.display()))
        }
        "predict_interaction" => {
            let protein = args["protein_sequence"]
                .as_str()
                .ok_or("Missing protein_sequence")?;
            let nucleic = args["nucleic_sequence"]
                .as_str()
                .ok_or("Missing nucleic_sequence")?;
            let task_root = session_dir.join(format!("pred_pni_{}", Uuid::new_v4()));
            let input_dir = task_root.join("input");
            let output_dir = task_root.join("output");
            std::fs::create_dir_all(&input_dir).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

            let sequence_pairs = json!([
                {
                    "protein": protein,
                    "nucleic": nucleic,
                }
            ])
            .to_string();

            let params = json!({
                "task_id": Uuid::new_v4().to_string(),
                "task_name": "pred_pni",
                "input_method": "file",
                "ids": "",
                "input_dir": input_dir.to_string_lossy().to_string(),
                "output_dir": output_dir.to_string_lossy().to_string(),
                "sequence_pairs": sequence_pairs,
            });
            let params_file = task_root.join("params.json");
            let out = run_python_pskit(&ai_root, &params, &params_file)?;
            Ok(format!("{}\nOutput dir: {}", out, output_dir.display()))
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
                .unwrap_or(120)
                .clamp(1, 2000);
            let max_chars = args["max_chars"]
                .as_u64()
                .map(|v| v as usize)
                .unwrap_or(12_000)
                .clamp(200, 120_000);

            let resolved = resolve_session_file_path(session_dir, file_path)?;
            read_result_file_preview(&resolved, start_line, max_lines, max_chars)
        }
        _ => Err(format!("Unknown tool: {}", name)),
    }
}
