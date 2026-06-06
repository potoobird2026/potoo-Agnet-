/*! NativePlatform —— 平台抽象层 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKind {
    Windows,
    Linux,
    MacOs,
}

pub struct NativePlatform {
    os: OsKind,
    command_timeout_secs: u64,
}
impl NativePlatform {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            os: Self::detect_os(),
            command_timeout_secs: timeout_secs,
        }
    }
    pub fn detect_os() -> OsKind {
        if cfg!(target_os = "windows") {
            OsKind::Windows
        } else if cfg!(target_os = "macos") {
            OsKind::MacOs
        } else {
            OsKind::Linux
        }
    }
    pub fn default_shell(&self) -> (&str, &[&str]) {
        match self.os {
            OsKind::Windows => ("cmd", &["/C"]),
            _ => ("sh", &["-c"]),
        }
    }
    pub fn command_timeout_secs(&self) -> u64 {
        self.command_timeout_secs
    }
    pub fn os(&self) -> OsKind {
        self.os
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_os_returns_valid() {
        let os = NativePlatform::detect_os();
        // 应该是 Windows/Linux/MacOs 之一
        assert!(matches!(
            os,
            OsKind::Windows | OsKind::Linux | OsKind::MacOs
        ));
    }

    #[test]
    fn test_platform_timeout() {
        let p = NativePlatform::new(30);
        assert_eq!(p.command_timeout_secs(), 30);
    }

    #[test]
    fn test_default_shell_not_empty() {
        let p = NativePlatform::new(60);
        let (cmd, _args) = p.default_shell();
        assert!(!cmd.is_empty());
    }
}
