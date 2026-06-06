/*! IdentityProvider（设计文档 §5.2，pri=5）

从 StepContext 读取身份内容。不可裁剪、不可压缩。
*/

use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::context::CONTEXT_IDENTITY;
use async_trait::async_trait;

pub struct IdentityProvider;

#[async_trait]
impl ContextProvider for IdentityProvider {
    fn name(&self) -> &str {
        "identity"
    }
    fn priority(&self) -> u8 {
        5
    }
    fn allow_truncation(&self) -> bool {
        false
    }
    fn silent_on_empty(&self) -> bool {
        true
    }

    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize {
        config.max_tokens
    }

    async fn provide(
        &self,
        ap: &dyn SlotAccessPoint,
        quota: &ContextQuota,
        _config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError> {
        let identity = ap
            .read_context_raw(CONTEXT_IDENTITY)
            .and_then(|any| any.downcast_ref::<crate::shared_types::IdentitySection>())
            .cloned();

        match identity {
            Some(id) => {
                let text = id.content;
                let tokens = (text.len() as f64 / 4.0).ceil() as usize;
                let max_tokens = quota.max_tokens.min(tokens);
                Ok(ProvidedContext {
                    blocks: vec![ContextBlock {
                        section_title: "## Agent Identity".into(),
                        content: text,
                        source: "identity".into(),
                        token_count: max_tokens,
                    }],
                    tokens_used: max_tokens,
                })
            }
            None => Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            }),
        }
    }
}
