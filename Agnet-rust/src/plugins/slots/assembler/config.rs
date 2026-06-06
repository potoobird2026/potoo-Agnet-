/*! Assembler 模块级常量与配置解析（设计文档 §11）

遵循跨平台规范 §2.3/§2.4。
*/

use crate::shared_types::assembler::AssemblerConfig;

/// 日志前缀（跨平台规范 §1.7——常量集中管理）
pub const LOG_PREFIX: &str = "assembler:";

/// 模板占位符常量（防止散落字符串）
pub const RULES_PLACEHOLDER: &str = "{{rules}}";
pub const ENV_INFO_PLACEHOLDER: &str = "{{env_info}}";
pub const IDENTITY_PLACEHOLDER: &str = "{{identity}}";
pub const COMPRESSION_SUMMARY_PLACEHOLDER: &str = "{{compression_summary}}";
pub const WORKING_MEMORY_PLACEHOLDER: &str = "{{working_memory}}";
pub const VECTOR_MEMORY_PLACEHOLDER: &str = "{{vector_memory}}";
pub const WORK_DIR_PLACEHOLDER: &str = "{{work_dir}}";
pub const PLATFORM_PLACEHOLDER: &str = "{{platform}}";
pub const TODAY_PLACEHOLDER: &str = "{{today}}";

impl AssemblerConfig {
    /// 基于 data_dir 解析模板路径（跨平台规范 §2.3：不依赖 CWD；§2.4：使用 join()）
    pub fn resolve_paths(&mut self, data_dir: &std::path::Path) {
        if !self.base_prompt_path.is_empty() {
            self.base_prompt_path = data_dir
                .join(&self.base_prompt_path)
                .to_string_lossy()
                .to_string();
        }
        if !self.injection_layout_path.is_empty() {
            self.injection_layout_path = data_dir
                .join(&self.injection_layout_path)
                .to_string_lossy()
                .to_string();
        }
    }
}

/// 从文件读取模板内容（设计文档 §5.1，宪法 §7f：文件不存在时返回默认字符串）
///
/// 遵循跨平台规范 §2.3：路径已由 resolve_paths 解析为绝对路径。
pub async fn load_template(path: &str, default: &str) -> String {
    if path.is_empty() {
        return default.to_string();
    }
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            tracing::info!("{} 已加载模板: {}", LOG_PREFIX, path);
            content
        }
        Err(e) => {
            tracing::info!(
                "{} 模板文件不存在 ({}: {}), 使用默认模板",
                LOG_PREFIX,
                path,
                e
            );
            default.to_string()
        }
    }
}
