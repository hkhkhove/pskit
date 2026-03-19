use serde_json::Value;

pub fn parse_tool_args(args_raw: &str) -> Result<Value, String> {
    if let Ok(v) = serde_json::from_str::<Value>(args_raw) {
        return Ok(v);
    }
    //多余双引号
    if let Ok(unwrapped) = serde_json::from_str::<String>(args_raw) {
        if let Ok(v) = serde_json::from_str::<Value>(&unwrapped) {
            return Ok(v);
        }
    }
    //多余花括号
    if let (Some(start), Some(end)) = (args_raw.find('{'), args_raw.rfind('}')) {
        if start < end {
            let candidate = &args_raw[start..=end];
            if let Ok(v) = serde_json::from_str::<Value>(candidate) {
                return Ok(v);
            }
        }
    }

    Err(format!(
        "Invalid tool arguments (first 300 chars): {}",
        args_raw.chars().take(300).collect::<String>()
    ))
}

pub fn truncate_text(input: &str, max_chars: usize) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_string();
    }
    let kept = chars[..max_chars].iter().collect::<String>();
    format!(
        "{}\n\n[TRUNCATED: original length {} chars, kept {} chars]",
        kept,
        chars.len(),
        max_chars
    )
}
