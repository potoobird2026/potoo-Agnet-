/*!
 * Logger — 异步写入器
 *
 * 功能描述：后台 tokio 任务，从 mpsc 通道接收 LogEntry，
 * 序列化为 JSON 行写入文件，支持按时间/大小的文件滚动。
 */

use std::path::PathBuf;

use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use super::config::{LoggerConfig, RotationPolicy};
use super::event::LogEntry;
use crate::core::types::Timestamp;

pub struct AsyncWriter {
    config: LoggerConfig,
    current_file: Option<File>,
    current_file_path: Option<PathBuf>,
    current_file_size: u64,
    current_date_hint: String,
    current_hour_hint: u32,
    sequence: u32,
}

impl AsyncWriter {
    pub fn new(config: LoggerConfig) -> Self {
        Self {
            config,
            current_file: None,
            current_file_path: None,
            current_file_size: 0,
            current_date_hint: String::new(),
            current_hour_hint: 0,
            sequence: 0,
        }
    }

    /// Main event loop — blocks until the channel closes.
    pub async fn run(&mut self, mut rx: mpsc::UnboundedReceiver<LogEntry>) {
        // Ensure output directory exists
        if let Err(e) = tokio::fs::create_dir_all(&self.config.output_dir).await {
            tracing::error!("logger: failed to create output dir: {}", e);
            return;
        }

        while let Some(entry) = rx.recv().await {
            if self.should_rotate() {
                if let Err(e) = self.rotate().await {
                    tracing::error!("logger: rotation failed: {}", e);
                }
            }

            if let Err(e) = self.write_entry(&entry).await {
                tracing::error!("logger: write failed: {}", e);
            }
        }

        // Flush on shutdown
        self.flush().await;
    }

    fn should_rotate(&self) -> bool {
        let now = Timestamp::now();
        let date_str = now.format_ymd();
        match &self.config.rotation {
            RotationPolicy::Hourly => {
                let hour = now.format_hour().parse::<u32>().unwrap_or(0);
                self.current_date_hint != date_str || self.current_hour_hint != hour
            }
            RotationPolicy::Daily => self.current_date_hint != date_str,
            RotationPolicy::SizeBased(max_size) => self.current_file_size > *max_size,
            RotationPolicy::Never => false,
        }
    }

    async fn rotate(&mut self) -> Result<(), std::io::Error> {
        self.flush().await;

        let now = Timestamp::now();
        let date_str = now.format_ymd();
        let hour = now.format_hour().parse::<u32>().unwrap_or(0);

        let filename = match &self.config.rotation {
            RotationPolicy::Hourly => {
                format!("{}_{}_{:02}.jsonl", self.config.file_prefix, date_str, hour)
            }
            RotationPolicy::Daily => {
                format!("{}_{}.jsonl", self.config.file_prefix, date_str)
            }
            RotationPolicy::SizeBased(_) => {
                self.sequence += 1;
                format!(
                    "{}_{}_{:04}.jsonl",
                    self.config.file_prefix, date_str, self.sequence
                )
            }
            RotationPolicy::Never => {
                format!("{}.jsonl", self.config.file_prefix)
            }
        };

        self.current_file_path = Some(self.config.output_dir.join(&filename));
        self.current_file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(
                    self.current_file_path
                        .as_ref()
                        .ok_or_else(|| std::io::Error::other("file path not set"))?,
                )
                .await?,
        );
        self.current_file_size = 0;
        self.current_date_hint = date_str;
        self.current_hour_hint = hour;

        Ok(())
    }

    async fn write_entry(&mut self, entry: &LogEntry) -> Result<(), std::io::Error> {
        if self.current_file.is_none() {
            self.rotate().await?;
        }

        let mut line = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
        line.push('\n');

        let file = self
            .current_file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("file handle not open"))?;
        file.write_all(line.as_bytes()).await?;
        self.current_file_size += line.len() as u64;

        Ok(())
    }

    async fn flush(&mut self) {
        if let Some(ref mut file) = self.current_file {
            let _ = file.flush().await;
        }
    }
}
