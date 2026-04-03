use crate::provider::AgentProvider;
use alice_train::inference::{AliceModel, GenerationConfig};
use alice_train::tokenizer::BpeTokenizer;
use std::io::{self, Write};
use std::time::Instant;

pub struct LocalProvider {
    model: AliceModel,
    tokenizer: BpeTokenizer,
    gen_config: GenerationConfig,
}

impl LocalProvider {
    /// .alice モデルと tokenizer.json をロード。
    pub fn load(model_path: &str, tokenizer_path: &str) -> Result<Self, String> {
        eprintln!("[ALICE] モデル読み込み中...");
        let start = Instant::now();

        let model =
            AliceModel::from_file(model_path).map_err(|e| format!("model load error: {e}"))?;

        let tokenizer = BpeTokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("tokenizer load error: {e}"))?;

        let elapsed = start.elapsed();
        eprintln!(
            "[ALICE] {} 起動完了 ({:.1}s)",
            model
                .meta
                .config
                .num_hidden_layers
                .to_string()
                + "層モデル",
            elapsed.as_secs_f64()
        );

        Ok(Self {
            model,
            tokenizer,
            gen_config: GenerationConfig {
                max_tokens: 4096,
                temperature: 0.3,
                top_k: 40,
                repetition_penalty: 1.1,
            },
        })
    }
}

impl AgentProvider for LocalProvider {
    fn name(&self) -> &str {
        "alice-local"
    }

    fn generate(&self, messages: &[(&str, &str)]) -> Result<String, String> {
        let prompt_ids = self.tokenizer.format_multi_turn(messages);

        let stop_sequences = ["</tool_use>", "<|im_end|>"];

        // ストリーミング表示
        let generated = self.model.generate_streaming_with_stop(
            &prompt_ids,
            &self.gen_config,
            self.tokenizer.eos_token_id,
            &stop_sequences,
            &self.tokenizer,
            |chunk| {
                eprint!("{chunk}");
                io::stderr().flush().ok();
                true
            },
        );

        eprintln!(); // 改行
        Ok(generated)
    }
}
