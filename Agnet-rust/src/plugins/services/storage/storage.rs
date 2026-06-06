/*!
 * Storage —— 统一数据目录管理
 *
 * 所有运行时数据（会话、压缩、日志、记忆、Chronos、向量库）
 * 统一存放在单一根目录下，通过环境变量可逐层配置。
 *
 * 优先级：AAGNET_XXX_DIR > AAGNET_HOME/xxx/ > 平台默认
 * 根目录名：potoobird
 *
 * 跨平台合规：所有路径通过 dirs + PathBuf::join() 构建，
 * 不使用裸 /tmp/、~、相对路径。
 */

use std::path::PathBuf;

/// 根目录名称常量
const DATA_DIR_NAME: &str = "potoobird";

/// 解析目录路径：优先读环境变量，其次用回调默认
///
/// 若环境变量值为相对路径，则以当前工作目录为基准展开；
/// 若为绝对路径则直接使用。
fn resolve(env_key: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    if let Ok(val) = std::env::var(env_key) {
        let p = PathBuf::from(val);
        if p.is_relative() {
            std::env::current_dir().unwrap_or_default().join(p)
        } else {
            p
        }
    } else {
        default()
    }
}

// ── 根目录 ──

/// 数据根目录 = $AAGNET_HOME 或平台标准 data_dir/potoobird
pub fn home() -> PathBuf {
    resolve("AAGNET_HOME", || {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(DATA_DIR_NAME)
    })
}

// ── 子目录 ──

/// 会话持久化目录 = $AAGNET_SESSIONS_DIR 或 <home>/sessions
pub fn sessions_dir() -> PathBuf {
    resolve("AAGNET_SESSIONS_DIR", || home().join("sessions"))
}

/// 日志输出目录 = $AAGNET_LOGS_DIR 或 <home>/logs
pub fn logs_dir() -> PathBuf {
    resolve("AAGNET_LOGS_DIR", || home().join("logs"))
}

/// 压缩快照目录 = $AAGNET_COMPRESSED_DIR 或 <home>/compressed
pub fn compressed_dir() -> PathBuf {
    resolve("AAGNET_COMPRESSED_DIR", || home().join("compressed"))
}

/// Chronos 决策记录目录 = $AAGNET_CHRONOS_DIR 或 <home>/chronos
pub fn chronos_dir() -> PathBuf {
    resolve("AAGNET_CHRONOS_DIR", || home().join("chronos"))
}

/// 记忆存储目录 = $AAGNET_MEMORY_DIR 或 <home>/memory
pub fn memory_dir() -> PathBuf {
    resolve("AAGNET_MEMORY_DIR", || home().join("memory"))
}

/// 向量数据库开关：读取 AAGNET_VECTOR_DB_ENABLED 环境变量
pub fn vector_db_enabled() -> bool {
    std::env::var("AAGNET_VECTOR_DB_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// 向量数据库目录 = $AAGNET_VECTOR_DB_DIR 或 <memory>/vector_db
pub fn vector_db_dir() -> PathBuf {
    resolve("AAGNET_VECTOR_DB_DIR", || memory_dir().join("vector_db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_home_uses_data_dir() {
        // 测试中安全：清除环境变量以确保走默认路径
        env::remove_var("AAGNET_HOME");
        let h = home();
        // 验证路径以 potoobird 结尾
        assert!(h.ends_with(DATA_DIR_NAME));
    }

    #[test]
    fn test_resolve_env_var_takes_priority() {
        let test_dir = std::env::temp_dir().join("aagnet_test_storage");
        env::set_var("AAGNET_SESSIONS_DIR", test_dir.to_str().unwrap());
        let s = sessions_dir();
        assert_eq!(s, test_dir);
        env::remove_var("AAGNET_SESSIONS_DIR");
    }

    #[test]
    fn test_subdirs_nested_under_home() {
        env::remove_var("AAGNET_HOME");
        env::remove_var("AAGNET_LOGS_DIR");
        let h = home();
        let l = logs_dir();
        assert!(l.starts_with(&h));
    }

    #[test]
    fn test_vector_db_enabled_default_false() {
        env::remove_var("AAGNET_VECTOR_DB_ENABLED");
        assert!(!vector_db_enabled());
    }

    #[test]
    fn test_vector_db_enabled_true() {
        env::set_var("AAGNET_VECTOR_DB_ENABLED", "true");
        assert!(vector_db_enabled());
        env::remove_var("AAGNET_VECTOR_DB_ENABLED");
    }
}
