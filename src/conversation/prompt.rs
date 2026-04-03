use crate::conversation::message::{AgentMessage, Role};
use crate::tools::ToolSpec;
use regex::Regex;

use super::message::ToolCall;

/// ツール定義を含むシステムプロンプトを構築。
pub fn build_system_prompt(base_prompt: &str, tools: &[ToolSpec]) -> String {
    let mut prompt = base_prompt.to_string();

    if !tools.is_empty() {
        let tool_defs: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();
        let tools_json = serde_json::to_string_pretty(&tool_defs).unwrap_or_default();

        prompt.push_str("\n\n<tools>\n");
        prompt.push_str(&tools_json);
        prompt.push_str("\n</tools>\n\n");
        prompt.push_str(
            "To use a tool, output a <tool_use> block:\n\
             <tool_use>\n\
             {\"name\": \"tool_name\", \"id\": \"call_001\", \"input\": {\"key\": \"value\"}}\n\
             </tool_use>\n\n\
             You may output text before and after tool_use blocks. You may call multiple tools.",
        );
    }

    prompt
}

/// メッセージ履歴を ChatML 用の (role, content) ペア列に変換。
pub fn messages_to_turns(messages: &[AgentMessage]) -> Vec<(&str, String)> {
    let mut turns = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System | Role::User => {
                turns.push((msg.role.as_str(), msg.content.clone()));
            }
            Role::Assistant => {
                // ツール呼び出しがある場合、テキスト + <tool_use> ブロックを結合
                let mut content = msg.content.clone();
                for tc in &msg.tool_calls {
                    let json = serde_json::json!({
                        "name": tc.name,
                        "id": tc.id,
                        "input": tc.input,
                    });
                    content.push_str("\n<tool_use>\n");
                    content.push_str(&serde_json::to_string(&json).unwrap_or_default());
                    content.push_str("\n</tool_use>");
                }
                turns.push(("assistant", content));
            }
            Role::Tool => {
                let id = msg.tool_call_id.as_deref().unwrap_or("unknown");
                let content = format!("<tool_result id=\"{id}\">\n{}\n</tool_result>", msg.content);
                turns.push(("tool", content));
            }
        }
    }

    turns
}

/// アシスタント出力から <tool_use> ブロックをパース。
pub fn parse_tool_calls(response_text: &str) -> Vec<ToolCall> {
    let re = Regex::new(r"(?s)<tool_use>\s*(\{.*?\})\s*</tool_use>").unwrap();
    re.captures_iter(response_text)
        .filter_map(|cap| {
            let json_str = &cap[1];
            serde_json::from_str::<serde_json::Value>(json_str)
                .ok()
                .and_then(|v| {
                    Some(ToolCall {
                        id: v.get("id")?.as_str()?.to_string(),
                        name: v.get("name")?.as_str()?.to_string(),
                        input: v.get("input")?.clone(),
                    })
                })
        })
        .collect()
}

/// アシスタント出力からテキスト部分のみ抽出 (<tool_use> ブロックを除去)。
pub fn extract_text(response_text: &str) -> String {
    let re = Regex::new(r"(?s)<tool_use>.*?</tool_use>").unwrap();
    let text = re.replace_all(response_text, "");
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_calls_single() {
        let text = r#"Let me check.
<tool_use>
{"name": "bash", "id": "call_001", "input": {"command": "ls"}}
</tool_use>"#;
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bash");
        assert_eq!(calls[0].id, "call_001");
        assert_eq!(calls[0].input["command"], "ls");
    }

    #[test]
    fn test_parse_tool_calls_multiple() {
        let text = r#"
<tool_use>
{"name": "bash", "id": "c1", "input": {"command": "ls"}}
</tool_use>
Then read:
<tool_use>
{"name": "read_file", "id": "c2", "input": {"path": "src/main.rs"}}
</tool_use>"#;
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "bash");
        assert_eq!(calls[1].name, "read_file");
    }

    #[test]
    fn test_parse_tool_calls_none() {
        let text = "Just a normal response without tools.";
        let calls = parse_tool_calls(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_extract_text() {
        let text = r#"Let me check.
<tool_use>
{"name": "bash", "id": "c1", "input": {"command": "ls"}}
</tool_use>
Done."#;
        let result = extract_text(text);
        assert_eq!(result, "Let me check.\n\nDone.");
    }

    #[test]
    fn test_build_system_prompt_with_tools() {
        let tools = vec![ToolSpec {
            name: "bash".to_string(),
            description: "Execute a command".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"command": {"type": "string"}}}),
            permission: crate::permission::PermissionLevel::FullAccess,
        }];
        let prompt = build_system_prompt("You are ALICE.", &tools);
        assert!(prompt.contains("<tools>"));
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("</tools>"));
    }

    #[test]
    fn test_messages_to_turns() {
        let messages = vec![
            AgentMessage::user("hello"),
            AgentMessage::assistant("I'll check.", vec![ToolCall {
                id: "c1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({"command": "ls"}),
            }]),
            AgentMessage::tool_result("c1", "file1.rs\nfile2.rs", false),
        ];
        let turns = messages_to_turns(&messages);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].0, "user");
        assert_eq!(turns[1].0, "assistant");
        assert!(turns[1].1.contains("<tool_use>"));
        assert_eq!(turns[2].0, "tool");
        assert!(turns[2].1.contains("tool_result"));
    }
}
