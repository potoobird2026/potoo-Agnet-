/*! IdentityManager —— L1 身份管理 */
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::super::config::L1Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityMetadata {
    pub name: String,
    pub version: String,
    pub updated: String,
    #[serde(skip)]
    pub inode: u64,
    #[serde(skip)]
    pub mtime: i64,
}

#[derive(Debug, Clone)]
pub struct IdentitySection {
    pub title: String,
    pub content: String,
}

pub struct IdentityManager {
    config: L1Config,
    metadata: Option<IdentityMetadata>,
    sections: Vec<IdentitySection>,
    file_path: PathBuf,
}

impl IdentityManager {
    pub fn new(config: L1Config, workspace_dir: &Path) -> Self {
        let file_path = workspace_dir.join(&config.identity_path);
        Self {
            config,
            metadata: None,
            sections: Vec::new(),
            file_path,
        }
    }

    /// 加载身份文件，解析 YAML frontmatter + Markdown 正文
    pub fn load(&mut self) -> Result<(), String> {
        if !self.file_path.exists() {
            tracing::warn!(
                "IdentityManager: 身份文件不存在 '{}'，使用空身份",
                self.file_path.display()
            );
            return Ok(());
        }
        let content =
            fs::read_to_string(&self.file_path).map_err(|e| format!("读取身份文件失败: {}", e))?;
        self.parse(&content)?;
        // 记录 inode/mtime
        if let Ok(meta) = fs::metadata(&self.file_path) {
            if let Some(m) = self.metadata.as_mut() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    m.inode = meta.ino();
                }
                #[cfg(not(unix))]
                {
                    m.inode = 0;
                }
                m.mtime = meta
                    .modified()
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    })
                    .unwrap_or(0);
            }
        }
        Ok(())
    }

    fn parse(&mut self, content: &str) -> Result<(), String> {
        let mut sections = Vec::new();
        let mut current_title = String::new();
        let mut current_content = String::new();
        let mut in_frontmatter = false;
        let mut frontmatter_lines = Vec::new();
        let mut first = true;

        for line in content.lines() {
            if first && line.trim() == "---" {
                in_frontmatter = true;
                first = false;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    first = false;
                    continue;
                }
                frontmatter_lines.push(line);
                continue;
            }
            first = false;
            if line.starts_with("# ") && !line.starts_with("## ") {
                if !current_title.is_empty() {
                    sections.push(IdentitySection {
                        title: current_title,
                        content: current_content.trim().to_string(),
                    });
                }
                current_title = line[2..].trim().to_string();
                current_content = String::new();
            } else {
                if !current_content.is_empty() {
                    current_content.push('\n');
                }
                current_content.push_str(line);
            }
        }
        if !current_title.is_empty() {
            sections.push(IdentitySection {
                title: current_title,
                content: current_content.trim().to_string(),
            });
        }

        let _fm_str = frontmatter_lines.join("\n");
        let metadata = Self::parse_frontmatter(&frontmatter_lines);

        self.metadata = Some(metadata);
        self.sections = sections;
        Ok(())
    }

    /// 格式化为 System Prompt 前缀
    pub fn inject_to_prompt(&self) -> String {
        let mut out = self.config.inject_prefix.clone();
        if let Some(ref meta) = self.metadata {
            out.push_str(&format!("**{}** v{}\n\n", meta.name, meta.version));
        }
        for section in &self.sections {
            out.push_str(&format!("### {}\n{}\n\n", section.title, section.content));
        }
        out
    }

    /// 检测外部修改（对比 inode/mtime）
    pub fn check_modified(&self) -> bool {
        if !self.file_path.exists() {
            return false;
        }
        if let Ok(meta) = fs::metadata(&self.file_path) {
            if let Some(ref current) = self.metadata {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if meta.ino() != current.inode {
                        return true;
                    }
                }
                let mtime = meta
                    .modified()
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    })
                    .unwrap_or(0);
                return mtime != current.mtime;
            }
        }
        false
    }

    /// 原子写入
    pub fn update(&mut self, content: &str, reason: &str) -> Result<(), String> {
        let tmp_path = self.file_path.with_extension("md.tmp");
        let mut f = fs::File::create(&tmp_path).map_err(|e| format!("创建临时文件失败: {}", e))?;
        f.write_all(content.as_bytes())
            .map_err(|e| format!("写入临时文件失败: {}", e))?;
        f.flush().map_err(|e| format!("刷新失败: {}", e))?;
        if self.file_path.exists() {
            fs::remove_file(&self.file_path).map_err(|e| format!("删除旧文件失败: {}", e))?;
        }
        fs::rename(&tmp_path, &self.file_path).map_err(|e| format!("rename 失败: {}", e))?;
        self.parse(content)?;
        tracing::info!("IdentityManager: 身份已更新（原因: {}）", reason);
        Ok(())
    }

    pub fn sections(&self) -> &[IdentitySection] {
        &self.sections
    }
    pub fn metadata(&self) -> Option<&IdentityMetadata> {
        self.metadata.as_ref()
    }

    fn parse_frontmatter(lines: &[&str]) -> IdentityMetadata {
        let mut name = "Assistant".to_string();
        let mut version = "1.0".to_string();
        let mut updated = Utc::now().format("%Y-%m-%d").to_string();
        for line in lines {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let val = value.trim().trim_matches('"').trim();
                match key.trim() {
                    "name" => name = val.to_string(),
                    "version" => version = val.to_string(),
                    "updated" => updated = val.to_string(),
                    _ => {}
                }
            }
        }
        IdentityMetadata {
            name,
            version,
            updated,
            inode: 0,
            mtime: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_identity() -> &'static str {
        "---\nname: Assistant\nversion: \"1.0\"\nupdated: \"2026-01-15\"\n---\n\n# Core Identity\nI am a helpful coding assistant.\n\n# Preferences\n- Prefer Rust\n"
    }

    #[test]
    fn test_parse_identity() {
        let tmp = std::env::temp_dir().join("test_identity.md");
        let mut f = fs::File::create(&tmp).unwrap();
        f.write_all(sample_identity().as_bytes()).unwrap();
        drop(f);

        let mut mgr = IdentityManager::new(L1Config::default(), &std::env::temp_dir());
        mgr.file_path = tmp.clone();
        mgr.load().unwrap();
        assert!(mgr.metadata.is_some());
        assert_eq!(mgr.sections.len(), 2);
        assert_eq!(mgr.sections[0].title, "Core Identity");
        assert!(!mgr.inject_to_prompt().is_empty());

        let _ = fs::remove_file(&tmp);
    }
}
