/*!
 * Logger ?配置管理
 *
 * 功能描述：LoggerConfig 提供完整的日志系统配置，包括输出路径? * 滚动策略、事件级别过滤、聚合配置、保留策略? */

use std::path::PathBuf;

/// Event severity level filter
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum EventLevel {
    Debug = 0,
    #[default]
    Info = 1,
    Warning = 2,
    Error = 3,
}

/// File rotation policy
#[derive(Debug, Clone, Default)]
pub enum RotationPolicy {
    /// Rotate every hour
    Hourly,
    /// Rotate every day
    #[default]
    Daily,
    /// Rotate when file exceeds byte limit, appending a sequence number
    SizeBased(u64),
    /// Never rotate ?all events in one file
    Never,
}

/// Aggregation configuration
#[derive(Debug, Clone, Default)]
pub struct AggregationConfig {
    /// Interval between aggregation runs, in seconds. 0 disables.
    pub interval_secs: u64,
    /// Which aggregators are enabled
    pub enabled: Vec<AggregatorType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregatorType {
    CompressionStats,
    FeedbackPatterns,
    PersistenceStatus,
}

/// Retention policy for log files
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Keep files for this many days. 0 = never auto-delete.
    pub days: u32,
    /// Max total disk usage in MB. 0 = no limit.
    pub max_disk_mb: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            days: 30,
            max_disk_mb: 0,
        }
    }
}

/// Logger configuration
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    /// Master switch
    pub enabled: bool,
    /// Output directory for log files
    pub output_dir: PathBuf,
    /// Filename prefix, e.g. "aagnetlog"
    pub file_prefix: String,
    /// File rotation policy
    pub rotation: RotationPolicy,
    /// Minimum event level to record
    pub min_level: EventLevel,
    /// Periodic aggregation (optional)
    pub aggregation: Option<AggregationConfig>,
    /// Retention policy
    pub retention: RetentionPolicy,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        let base = crate::plugins::services::storage::logs_dir();
        Self {
            enabled: true,
            output_dir: base,
            file_prefix: "aagnetlog".into(),
            rotation: RotationPolicy::Daily,
            min_level: EventLevel::Info,
            aggregation: None,
            retention: RetentionPolicy::default(),
        }
    }
}
