/// LLM バックエンド trait。
///
/// ローカル .alice モデルと API フォールバックの両方に対応。
pub trait AgentProvider: Send {
    /// プロバイダ名。
    fn name(&self) -> &str;

    /// ChatML (role, content) ペア列から生成。
    ///
    /// 返り値: 生成されたテキスト (ツール呼び出しタグを含む可能性あり)。
    fn generate(&self, messages: &[(&str, &str)]) -> Result<String, String>;
}

#[cfg(feature = "local")]
pub mod local;

#[cfg(feature = "api")]
pub mod openai;
