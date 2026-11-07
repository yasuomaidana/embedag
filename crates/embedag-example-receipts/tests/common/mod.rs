//! Provider support shared by the categorization tests: every scenario runs once per local runtime.

use embedag_llm::async_trait;
use embedag_llm::provider::{
    CandleProvider, LlmError, LlmProvider, ModelMetadata, OxillamaProvider,
};
use serde_json::Value;

/// The local inference runtimes the scenarios run against.
#[derive(Debug, Clone, Copy)]
pub enum Backend {
    Candle,
    Oxillama,
}

impl Backend {
    /// A provider of this runtime, with the model the tests use.
    pub fn provider(self) -> AnyProvider {
        match self {
            Self::Candle => AnyProvider(Box::new(CandleProvider::new("candle-smollm"))),
            Self::Oxillama => AnyProvider(Box::new(OxillamaProvider::new(
                "llama-3-8b-instruct",
                "q4_k_m",
            ))),
        }
    }
}

/// A provider of either runtime, so one generic engine serves every scenario.
pub struct AnyProvider(Box<dyn LlmProvider>);

#[async_trait]
impl LlmProvider for AnyProvider {
    async fn generate_structured(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError> {
        self.0.generate_structured(prompt, schema).await
    }

    fn model_info(&self) -> ModelMetadata {
        self.0.model_info()
    }
}
