pub mod message;
pub mod prompt;
pub mod session;

use crate::conversation::message::{AgentMessage, ToolCall};
use crate::conversation::prompt::{
    build_system_prompt, extract_text, messages_to_turns, parse_tool_calls,
};
use crate::permission::{PermissionLevel, PermissionPolicy};
use crate::provider::AgentProvider;
use crate::tools::{ToolExecutor, ToolSpec};
use std::io::{self, Write};

const MAX_TOOL_LOOPS: usize = 25;

pub struct ConversationRuntime {
    provider: Box<dyn AgentProvider>,
    tool_executor: Box<dyn ToolExecutor>,
    messages: Vec<AgentMessage>,
    system_prompt: String,
    tool_specs: Vec<ToolSpec>,
    permission_policy: PermissionPolicy,
}

impl ConversationRuntime {
    pub fn new(
        provider: Box<dyn AgentProvider>,
        tool_executor: Box<dyn ToolExecutor>,
        system_prompt: String,
        permission_policy: PermissionPolicy,
    ) -> Self {
        let tool_specs = tool_executor.specs();
        Self {
            provider,
            tool_executor,
            messages: Vec::new(),
            system_prompt,
            tool_specs,
            permission_policy,
        }
    }

    pub fn messages(&self) -> &[AgentMessage] {
        &self.messages
    }

    /// セッション復元用: メッセージ履歴を設定。
    pub fn restore_messages(&mut self, messages: Vec<AgentMessage>) {
        self.messages = messages;
    }

    /// 1ターン実行: ユーザー入力 → (ツール呼び出しループ) → 最終テキスト返却。
    pub fn run_turn(&mut self, user_input: &str) -> Result<String, String> {
        self.messages.push(AgentMessage::user(user_input));

        for iteration in 0..MAX_TOOL_LOOPS {
            // system prompt + ツール定義を結合
            let full_system = build_system_prompt(&self.system_prompt, &self.tool_specs);
            let mut all_messages = vec![AgentMessage::system(&full_system)];
            all_messages.extend(self.messages.clone());

            // メッセージ → (role, content) ペア列
            let turns = messages_to_turns(&all_messages);
            let turn_refs: Vec<(&str, &str)> =
                turns.iter().map(|(r, c)| (*r, c.as_str())).collect();

            // LLM 生成
            let response = self.provider.generate(&turn_refs)?;

            // ツール呼び出しをパース
            let tool_calls = parse_tool_calls(&response);

            if tool_calls.is_empty() {
                // テキストのみ — ターン完了
                self.messages
                    .push(AgentMessage::assistant(&response, Vec::new()));
                return Ok(extract_text(&response));
            }

            // ツール呼び出しあり
            self.messages
                .push(AgentMessage::assistant(&response, tool_calls.clone()));

            for tc in &tool_calls {
                let result = self.execute_tool(tc)?;
                self.messages.push(result);
            }

            if iteration == MAX_TOOL_LOOPS - 1 {
                return Err("max tool loop iterations exceeded".to_string());
            }
        }

        Err("unexpected end of conversation loop".to_string())
    }

    fn execute_tool(&self, tc: &ToolCall) -> Result<AgentMessage, String> {
        // パーミッションチェック
        let tool_spec = self.tool_specs.iter().find(|s| s.name == tc.name);
        let required = tool_spec
            .map(|s| s.permission)
            .unwrap_or(PermissionLevel::FullAccess);

        if !self.permission_policy.allows(required) {
            // ユーザーに確認
            eprint!(
                "  [permission] {} requires {:?}. Allow? [y/N] ",
                tc.name, required
            );
            io::stderr().flush().ok();
            let mut input = String::new();
            io::stdin().read_line(&mut input).ok();
            if !input.trim().eq_ignore_ascii_case("y") {
                return Ok(AgentMessage::tool_result(
                    &tc.id,
                    format!("Permission denied for tool: {}", tc.name),
                    true,
                ));
            }
        }

        // ツール実行
        match self.tool_executor.execute(&tc.name, &tc.input) {
            Ok(output) => Ok(AgentMessage::tool_result(&tc.id, output, false)),
            Err(e) => Ok(AgentMessage::tool_result(&tc.id, e, true)),
        }
    }
}
