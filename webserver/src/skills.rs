use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub workflow: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub stop_when: String,
    pub terminal_tool: Option<String>,
    #[serde(default)]
    pub max_calls_per_tool: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillCatalog {
    skills: Vec<Skill>,
}

pub fn load_skills() -> anyhow::Result<Vec<Skill>> {
    let config_path = crate::config::Config::home()
        .join("webserver")
        .join("config")
        .join("skills.json");
    let content = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|_| include_str!("../config/skills.json").to_string());
    let catalog: SkillCatalog = serde_json::from_str(&content)?;
    Ok(catalog.skills)
}

pub fn default_skill(skills: &[Skill]) -> Option<Skill> {
    skills
        .iter()
        .find(|s| s.id == "binding_pair_annotation")
        .cloned()
        .or_else(|| skills.first().cloned())
}

pub fn allowed_tool_set(skill: &Skill) -> HashSet<String> {
    skill.allowed_tools.iter().cloned().collect()
}

pub fn render_skill_prompt(skill: &Skill) -> String {
    let workflow = skill
        .workflow
        .iter()
        .enumerate()
        .map(|(i, step)| format!("{}. {}", i + 1, step))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "[Skill Selected]\nID: {}\nName: {}\nDescription: {}\nWhen to use: {}\nWorkflow:\n{}\nAllowed tools: {}\nStop condition: {}\nAfter stop condition is met, do not call more tools and provide final concise answer.",
        skill.id,
        skill.name,
        skill.description,
        skill.when_to_use,
        workflow,
        skill.allowed_tools.join(", "),
        skill.stop_when,
    )
}

pub async fn select_skill_with_llm(
    client: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    user_message: &str,
    skills: &[Skill],
) -> anyhow::Result<Option<String>> {
    let mut skill_lines = Vec::new();
    for skill in skills {
        skill_lines.push(format!(
            "- {}: {} | when: {}",
            skill.id, skill.description, skill.when_to_use
        ));
    }

    let selector_prompt = format!(
        "Select the best skill ID for the user request.\nAvailable skills:\n{}\nRespond with strict JSON: {{\"skill_id\":\"<id>\"}}\nIf none match, return {{\"skill_id\":\"\"}}",
        skill_lines.join("\n")
    );

    let request_body = serde_json::json!({
        "model": "gemini-3-flash-preview",
        "stream": false,
        "messages": [
            {
                "role": "system",
                "content": "You are a skill router. Output JSON only."
            },
            {
                "role": "user",
                "content": format!("{}\n\nUser request:\n{}", selector_prompt, user_message)
            }
        ]
    });

    let response = client
        .post(api_url)
        .bearer_auth(api_key)
        .json(&request_body)
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let value: serde_json::Value = response.json().await?;
    let content = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("message"))
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if content.is_empty() {
        return Ok(None);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            // Try to recover JSON block from fenced or mixed text.
            if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
                serde_json::from_str(&content[start..=end]).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            }
        }
    };

    let skill_id = parsed
        .get("skill_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if skill_id.is_empty() {
        return Ok(None);
    }

    let exists = skills.iter().any(|s| s.id == skill_id);
    if exists { Ok(Some(skill_id)) } else { Ok(None) }
}
