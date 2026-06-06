use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod error;
pub mod persistence;
pub mod plugin;

// ============================================
// Timestamp —— 毫秒级 Unix 时间戳（UTC）
// ============================================
/// 替代 chrono::DateTime<Utc> 以消除 core 对 chrono 的依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    pub fn now() -> Self {
        let dur = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("SystemTime before Unix epoch");
        Self(dur.as_millis() as i64)
    }

    pub fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    pub fn as_millis(&self) -> i64 {
        self.0
    }

    pub fn duration_since(&self, other: Timestamp) -> std::time::Duration {
        if self.0 > other.0 {
            std::time::Duration::from_millis((self.0 - other.0) as u64)
        } else {
            std::time::Duration::from_secs(0)
        }
    }

    fn epoch_secs(&self) -> i64 {
        self.0 / 1000
    }

    fn epoch_subsec_millis(&self) -> u32 {
        (self.0 % 1000).unsigned_abs() as u32
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_ymdhms(&self) -> (i32, u32, u32, u32, u32, u32) {
        let secs = self.epoch_secs();
        let z = secs / 86400 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = y + if m <= 2 { 1 } else { 0 };
        let rem = if secs >= 0 {
            secs % 86400
        } else {
            86400 - ((-secs) % 86400)
        };
        let hour = (rem / 3600) as u32;
        let minute = ((rem % 3600) / 60) as u32;
        let second = (rem % 60) as u32;
        (year as i32, m, d, hour, minute, second)
    }

    /// RFC3339 格式："2024-01-15T14:30:00.123Z"
    pub fn format_rfc3339(&self) -> String {
        let (y, m, d, hh, mm, ss) = self.to_ymdhms();
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            y,
            m,
            d,
            hh,
            mm,
            ss,
            self.epoch_subsec_millis()
        )
    }

    /// "YYYY-MM-DD"
    pub fn format_ymd(&self) -> String {
        let (y, m, d, _, _, _) = self.to_ymdhms();
        format!("{:04}-{:02}-{:02}", y, m, d)
    }

    /// "HH" (hour, 0-padded)
    pub fn format_hour(&self) -> String {
        let (_, _, _, hh, _, _) = self.to_ymdhms();
        format!("{:02}", hh)
    }

    /// "YYYYMMDD-HHMMSS"（用于备份文件名）
    pub fn format_compact(&self) -> String {
        let (y, m, d, hh, mm, ss) = self.to_ymdhms();
        format!("{:04}{:02}{:02}-{:02}{:02}{:02}", y, m, d, hh, mm, ss)
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        i64::deserialize(deserializer).map(Timestamp)
    }
}

// ============================================
// Version —— 语义化版本号
// ============================================
/// 代替 semver::Version，避免 core 依赖 semver crate。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("无效版本格式: {}", s));
        }
        let major = parts[0]
            .parse()
            .map_err(|_| format!("无效主版本号: {}", parts[0]))?;
        let minor = parts[1]
            .parse()
            .map_err(|_| format!("无效次版本号: {}", parts[1]))?;
        let patch = parts[2]
            .parse()
            .map_err(|_| format!("无效补丁版本号: {}", parts[2]))?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ============================================
// CancellationToken —— 轻量取消令牌
// ============================================
/// 代替 tokio_util::sync::CancellationToken，避免 core 依赖 tokio-util。
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
