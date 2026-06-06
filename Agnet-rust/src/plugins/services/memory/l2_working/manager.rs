/*! WorkingMemoryManager —— L2 工作记忆管理 */
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::super::config::L2Config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryFileType {
    Experience,
    Project,
    Correction,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFileFrontmatter {
    pub weight: f64,
    pub tags: Vec<String>,
    pub created: String,
    pub last_accessed: String,
    pub access_count: u64,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct MemoryFile {
    pub path: PathBuf,
    pub frontmatter: MemoryFileFrontmatter,
    pub content: String,
    pub file_type: MemoryFileType,
}

pub struct WorkingMemoryManager {
    config: L2Config,
    base_dir: PathBuf,
    files: Vec<MemoryFile>,
    index_path: PathBuf,
}

impl WorkingMemoryManager {
    pub fn new(config: L2Config, workspace_dir: &Path) -> Self {
        let base_dir = workspace_dir.join(&config.base_dir);
        let index_path = base_dir.join(&config.index_path);
        Self {
            config,
            base_dir,
            files: Vec::new(),
            index_path,
        }
    }

    pub fn init(&mut self) -> Result<(), String> {
        tracing::debug!("L2: max_files={}", self.config.max_files);
        for dir in &["experiences", "projects", "corrections", "archive"] {
            fs::create_dir_all(self.base_dir.join(dir))
                .map_err(|e| format!("创建目录 {} 失败: {}", dir, e))?;
        }
        self.load_index()?;
        Ok(())
    }

    fn load_index(&mut self) -> Result<(), String> {
        if !self.index_path.exists() {
            return Ok(());
        }
        let content =
            fs::read_to_string(&self.index_path).map_err(|e| format!("读取索引失败: {}", e))?;
        self.files = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, '|').collect();
            if parts.len() == 2 {
                let path = self.base_dir.join(parts[0].trim());
                if path.exists() {
                    if let Ok(file) = self.load_file(&path) {
                        self.files.push(file);
                    }
                }
            }
        }
        Ok(())
    }

    fn load_file(&self, path: &Path) -> Result<MemoryFile, String> {
        let content = fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;
        let (fm, body) = Self::split_frontmatter(&content);
        let frontmatter: MemoryFileFrontmatter =
            serde_json::from_str(&fm).unwrap_or_else(|_| MemoryFileFrontmatter {
                weight: 0.5,
                tags: vec![],
                created: Utc::now().to_rfc3339(),
                last_accessed: Utc::now().to_rfc3339(),
                access_count: 0,
                source: "unknown".into(),
            });
        let file_type = if path.to_string_lossy().contains("archive") {
            MemoryFileType::Archive
        } else if path.to_string_lossy().contains("projects") {
            MemoryFileType::Project
        } else if path.to_string_lossy().contains("corrections") {
            MemoryFileType::Correction
        } else {
            MemoryFileType::Experience
        };
        Ok(MemoryFile {
            path: path.to_path_buf(),
            frontmatter,
            content: body,
            file_type,
        })
    }

    fn split_frontmatter(content: &str) -> (String, String) {
        let mut lines = content.lines();
        let mut fm_lines = Vec::new();
        if lines.next().map(|l| l.trim() == "{").unwrap_or(false) {
            for line in &mut lines {
                if line.trim() == "}" {
                    break;
                }
                fm_lines.push(line);
            }
            (fm_lines.join("\n"), lines.collect::<Vec<_>>().join("\n"))
        } else {
            (String::new(), content.to_string())
        }
    }

    pub fn write_entry(&mut self, entry: MemoryFile) -> Result<(), String> {
        let dir = match entry.file_type {
            MemoryFileType::Experience => self.base_dir.join("experiences"),
            MemoryFileType::Project => self.base_dir.join("projects"),
            MemoryFileType::Correction => self.base_dir.join("corrections"),
            MemoryFileType::Archive => self.base_dir.join("archive"),
        };
        fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
        let filename = format!("{}.md", uuid::Uuid::new_v4());
        let file_path = dir.join(&filename);
        let fm_json = serde_json::to_string_pretty(&entry.frontmatter)
            .map_err(|e| format!("序列化失败: {}", e))?;
        let full = format!("{{\n{}\n}}\n\n{}", fm_json, entry.content);
        let mut f = fs::File::create(&file_path).map_err(|e| format!("创建文件失败: {}", e))?;
        f.write_all(full.as_bytes())
            .map_err(|e| format!("写入失败: {}", e))?;
        let mut entry = entry;
        entry.path = file_path.clone();
        self.files.push(entry);
        self.save_index()?;
        Ok(())
    }

    pub fn search(&self, tags: &[String], query: &str, top_k: usize) -> Vec<&MemoryFile> {
        let mut scored: Vec<(&MemoryFile, f64)> = self
            .files
            .iter()
            .filter(|f| f.file_type != MemoryFileType::Archive)
            .map(|f| {
                let tag_score = if tags.is_empty() {
                    0.5
                } else {
                    tags.iter()
                        .filter(|t| f.frontmatter.tags.contains(t))
                        .count() as f64
                        / tags.len() as f64
                };
                let query_score = if query.is_empty() {
                    0.5
                } else if f.content.to_lowercase().contains(&query.to_lowercase()) {
                    1.0
                } else {
                    0.0
                };
                (
                    f,
                    tag_score * 0.6 + query_score * 0.3 + f.frontmatter.weight * 0.1,
                )
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.iter().take(top_k).map(|(f, _)| *f).collect()
    }

    pub fn rebuild_index(&mut self) -> Result<(), String> {
        self.files.clear();
        for dir in &["experiences", "projects", "corrections"] {
            let dir_path = self.base_dir.join(dir);
            if !dir_path.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir_path).map_err(|e| format!("读取目录失败: {}", e))?
            {
                let entry = entry.map_err(|e| format!("目录条目失败: {}", e))?;
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Ok(file) = self.load_file(&path) {
                        self.files.push(file);
                    }
                }
            }
        }
        self.save_index()?;
        Ok(())
    }

    fn save_index(&self) -> Result<(), String> {
        let mut content = String::from("# Memory Index\n\n");
        for f in &self.files {
            if let Ok(rel) = f.path.strip_prefix(&self.base_dir) {
                content.push_str(&format!(
                    "{} | weight={:.2} | tags={:?}\n",
                    rel.display(),
                    f.frontmatter.weight,
                    f.frontmatter.tags
                ));
            }
        }
        fs::write(&self.index_path, content).map_err(|e| format!("写入索引失败: {}", e))?;
        Ok(())
    }

    pub fn active_files(&self) -> &[MemoryFile] {
        &self.files
    }
    pub fn archive_dir(&self) -> PathBuf {
        self.base_dir.join("archive")
    }
}
