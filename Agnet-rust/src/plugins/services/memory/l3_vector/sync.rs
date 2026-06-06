/*! VectorSyncService —— L2→L3 同步（后台循环 + 最小可用骨架） */
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use super::chunker::TextChunker;
use super::embedding::EmbeddingService;
use super::metadata::{VectorMetadata, VectorStoreError};
use super::store::VectorStore;

#[derive(Debug, Clone)]
pub struct SyncEvent {
    pub doc_id: String,
    pub action: SyncAction,
    pub timestamp: String,
}
#[derive(Debug, Clone, PartialEq)]
pub enum SyncAction {
    Upsert,
    Delete,
}

pub struct VectorSyncService {
    store: Arc<dyn VectorStore>,
    chunker: TextChunker,
    embedder: Arc<EmbeddingService>,
    running: Arc<AtomicBool>,
    interval_secs: u64,
    l2_path: PathBuf,
    synced_mtime: Arc<Mutex<std::collections::HashMap<PathBuf, std::time::SystemTime>>>,
}

impl Clone for VectorSyncService {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            chunker: self.chunker.clone(),
            embedder: self.embedder.clone(),
            running: self.running.clone(),
            interval_secs: self.interval_secs,
            l2_path: self.l2_path.clone(),
            synced_mtime: self.synced_mtime.clone(),
        }
    }
}

impl VectorSyncService {
    pub fn new(
        store: Arc<dyn VectorStore>,
        chunker: TextChunker,
        embedder: Arc<EmbeddingService>,
    ) -> Self {
        Self {
            store,
            chunker,
            embedder,
            running: Arc::new(AtomicBool::new(false)),
            interval_secs: 300,
            l2_path: PathBuf::new(),
            synced_mtime: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// B-3: 设置 L2 扫描路径
    pub fn set_l2_path(&mut self, path: PathBuf) {
        self.l2_path = path;
    }

    /// B-3: 设置扫描间隔
    pub fn set_interval(&mut self, secs: u64) {
        self.interval_secs = secs;
    }

    /// B-3: 启动后台循环
    pub fn start(&self) {
        self.running.store(true, Ordering::SeqCst);
        let store = self.store.clone();
        let chunker = self.chunker.clone();
        let embedder = self.embedder.clone();
        let running = self.running.clone();
        let interval = self.interval_secs;
        let l2_path = self.l2_path.clone();
        let synced_mtime = self.synced_mtime.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(interval));
            tick.tick().await;
            while running.load(Ordering::SeqCst) {
                tick.tick().await;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match scan_l2_for_changes(&l2_path, &synced_mtime).await {
                    Ok(changed) => {
                        for path in changed {
                            if let Err(e) =
                                sync_one(&store, &chunker, &embedder, &path, &synced_mtime).await
                            {
                                tracing::warn!("sync {} failed: {}", path.display(), e);
                            }
                        }
                    }
                    Err(e) => tracing::warn!("scan_l2 failed: {}", e),
                }
            }
            tracing::debug!("VectorSyncService: 后台循环已退出");
        });
    }

    /// B-3: 停止后台循环
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// 将 L2 文件内容同步到 L3
    pub async fn sync_document(
        &self,
        doc_id: &str,
        content: &str,
        metadata: VectorMetadata,
    ) -> Result<Vec<String>, VectorStoreError> {
        let _ = self.store.delete(&[doc_id.to_string()]).await;
        let chunks = self.chunker.chunk(content);
        if chunks.is_empty() {
            return Ok(Vec::new());
        }
        let texts: Vec<String> = chunks.clone();
        let vectors = self
            .embedder
            .embed(&texts)
            .await
            .map_err(|e| VectorStoreError {
                kind: "embed".into(),
                message: e,
            })?;
        let mut ids = Vec::new();
        let mut items = Vec::new();
        for (i, (chunk, vector)) in chunks.iter().zip(vectors.iter()).enumerate() {
            let id = format!("{}/chunk_{}", doc_id, i);
            let mut meta = metadata.clone();
            meta.text = chunk.clone();
            ids.push(id.clone());
            items.push((id, vector.clone(), meta));
        }
        self.store.upsert(items).await?;
        Ok(ids)
    }
}

/// B-3: 扫描 L2 目录中 .md/.txt 文件的变更（按 mtime）
async fn scan_l2_for_changes(
    l2_path: &PathBuf,
    synced_mtime: &Arc<Mutex<std::collections::HashMap<PathBuf, std::time::SystemTime>>>,
) -> Result<Vec<PathBuf>, String> {
    if !l2_path.exists() {
        return Ok(Vec::new());
    }
    let mut changed = Vec::new();
    let mut entries = tokio::fs::read_dir(l2_path)
        .await
        .map_err(|e| format!("read_dir failed: {}", e))?;
    let mut mtime_map = synced_mtime.lock().await;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("next_entry failed: {}", e))?
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" && ext != "txt" {
            continue;
        }
        let meta = tokio::fs::metadata(&path)
            .await
            .map_err(|e| format!("metadata failed: {}", e))?;
        let mtime = meta
            .modified()
            .map_err(|e| format!("modified failed: {}", e))?;
        if let Some(&prev_mtime) = mtime_map.get(&path) {
            if mtime > prev_mtime {
                changed.push(path.clone());
                mtime_map.insert(path, mtime);
            }
        } else {
            changed.push(path.clone());
            mtime_map.insert(path, mtime);
        }
    }
    Ok(changed)
}

/// B-3: 同步单个文件到 L3（读文件 → chunk → embed → upsert）
async fn sync_one(
    store: &Arc<dyn VectorStore>,
    chunker: &TextChunker,
    embedder: &Arc<EmbeddingService>,
    path: &PathBuf,
    synced_mtime: &Arc<Mutex<std::collections::HashMap<PathBuf, std::time::SystemTime>>>,
) -> Result<(), String> {
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("read_to_string failed: {}", e))?;
    let doc_id = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let meta = VectorMetadata {
        source_doc_id: doc_id.clone(),
        section_title: String::new(),
        text: content.clone(),
        weight: 1.0,
        tags: vec![],
        doc_type: "file".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        last_accessed: chrono::Utc::now().to_rfc3339(),
        access_count: 0,
        is_invalid: false,
    };
    let _ = store.delete(std::slice::from_ref(&doc_id)).await;
    let chunks = chunker.chunk(&content);
    if chunks.is_empty() {
        return Ok(());
    }
    let texts: Vec<String> = chunks.clone();
    let vectors = embedder
        .embed(&texts)
        .await
        .map_err(|e| format!("embed failed: {}", e))?;
    let mut items = Vec::new();
    for (i, (chunk, vector)) in chunks.iter().zip(vectors.iter()).enumerate() {
        let id = format!("{}/chunk_{}", doc_id, i);
        let mut m = meta.clone();
        m.text = chunk.clone();
        items.push((id, vector.clone(), m));
    }
    store
        .upsert(items)
        .await
        .map_err(|e| format!("upsert failed: {}", e))?;
    if let Ok(meta) = tokio::fs::metadata(path).await {
        if let Ok(mtime) = meta.modified() {
            synced_mtime.lock().await.insert(path.clone(), mtime);
        }
    }
    Ok(())
}
