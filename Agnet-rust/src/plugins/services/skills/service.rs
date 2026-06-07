/*! SkillsService —— 技能注入服务 ServicePlugin
 *
 * Phase B 改动（B-1..B-7）：
 * - B-1: SkillConfig::resolve_paths 接收 data_dir
 * - B-2: scan_skills 异步化 I/O（tokio::fs）+ 错误处理（PluginError::config）
 * - B-3: 文件名过滤改 .skill.md
 * - B-4: start() 注册真 Arc<DynProvider<dyn SkillsContractBundle>>
 * - B-5: HealthCheck 5s timeout
 * - B-6: ConfigReload 异步 + 增量（diff + 替换 + 重注册）
 * - B-7: shutdown 通过 self.ap 反注册 Provider
 *
 * 红线遵守：
 * - K-R01: register_provider 用 PROVIDER_SKILLS 常量（不用裸字符串）
 * - T-R01: trait 在 shared_types，不在本文件
 * - D-R01: 用共享的 DynProvider<T>
 * - P-R01: 无 Arc::new(()) 占位
 * - V-R01: HealthCheck 5s 内返回
 * - V-R02: ConfigReload tokio::spawn
 */
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, RwLock as StdRwLock};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::skills::{SkillContract, SkillsContractBundle, PROVIDER_SKILLS};
use crate::shared_types::DynProvider;

use super::config::SkillConfig;
use super::file_skill::FileSkill;
use super::provider::SkillInjectionProvider;

// 跨任务共享的技能集合（Arc<RwLock<...>> 让 bundle.all() 和 ConfigReload 都能读写）
type SharedSkills = Arc<StdRwLock<Vec<Arc<FileSkill>>>>;

struct SkillsInner {
    skills: SharedSkills,
    config: SkillConfig,
    running: bool,
    suspended: bool,
    /// 旧版 SkillInjectionProvider，保留用于 select_skills/format_injection 调用
    /// Phase C 会拆掉它（C-1 会重写 Assembler 侧的 SkillsProvider）
    #[allow(dead_code)]
    provider: Option<SkillInjectionProvider>,
}

/// Bundle 实现——持有 Arc<RwLock<Vec<Arc<FileSkill>>>>
/// 这样 ConfigReload 可以原地修改 Vec，消费者下次调 all() 自动看到新数据
struct SkillsContractBundleImpl {
    skills: SharedSkills,
}

impl SkillsContractBundle for SkillsContractBundleImpl {
    #[allow(clippy::map_clone)]
    fn all(&self) -> Vec<Arc<dyn SkillContract>> {
        self.skills
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|s_arc| -> Arc<dyn SkillContract> { s_arc.clone() })
            .collect()
    }
}

pub struct SkillsService {
    inner: Arc<RwLock<Option<SkillsInner>>>,
    /// start() 时存住的 ap，shutdown() 反注册 + ConfigReload 重注册时使用
    /// Option 是因为 shutdown 后要 take() 掉
    ap: Option<ServiceAccessPoint>,
}

impl SkillsService {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            ap: None,
        }
    }
}

#[async_trait]
impl ServicePlugin for SkillsService {
    fn name(&self) -> &str {
        "skills"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // B-2: 错误处理用 ? + PluginError::config，不再吞错
        let mut config: SkillConfig =
            serde_json::from_value(ctx.plugin_config.clone()).map_err(|e| {
                PluginError::config(format!("SkillsService 解析 plugin_config 失败: {e}"))
            })?;
        // B-1: 用 data_dir 锚定，不再用 current_dir
        config.resolve_paths(&ctx.data_dir);
        let skills = Self::scan_skills(&config.skills_dir).await;
        let display_dir = config.skills_dir.clone();
        let shared_skills: SharedSkills =
            Arc::new(StdRwLock::new(skills.into_iter().map(Arc::new).collect()));
        let provider = SkillInjectionProvider::new(config.clone());
        *self.inner.write().await = Some(SkillsInner {
            skills: shared_skills,
            config,
            running: false,
            suspended: false,
            provider: Some(provider),
        });
        let count = self
            .inner
            .read()
            .await
            .as_ref()
            .map(|i| i.skills.read().unwrap_or_else(|e| e.into_inner()).len())
            .unwrap_or(0);
        tracing::info!(
            "SkillsService: 初始化完成，加载了 {} 个技能（目录: {}）",
            count,
            display_dir.display()
        );
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // B-4: 注册真 DynProvider<dyn SkillsContractBundle>，删 Arc::new(())
        {
            let guard = self.inner.read().await;
            let inner = guard
                .as_ref()
                .ok_or_else(|| PluginError::init_failed("skills inner 未初始化"))?;
            let bundle_struct = SkillsContractBundleImpl {
                skills: inner.skills.clone(),
            };
            let bundle_trait: Arc<dyn SkillsContractBundle> = Arc::new(bundle_struct);
            // K-R01: 用 PROVIDER_SKILLS 常量
            ap.register_provider(PROVIDER_SKILLS, Arc::new(DynProvider(bundle_trait)));
        }
        // 标记运行 + 存住 ap（shutdown/ConfigReload 用）
        self.inner
            .write()
            .await
            .as_mut()
            .ok_or_else(|| PluginError::InitFailed("Skills: inner 未初始化".into()))?
            .running = true;
        self.ap = Some(ap);
        tracing::info!("SkillsService: 已注册 Provider PROVIDER_SKILLS");
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::GracefulShutdown | ServiceSignal::ImmediateShutdown => {
                if let Some(inner) = self.inner.write().await.as_mut() {
                    inner.running = false;
                }
            }
            ServiceSignal::ConfigReload => {
                // B-6: 异步重扫（V-R02 不阻塞 5s）
                let inner_arc = self.inner.clone();
                let ap_clone = self.ap.clone();
                tokio::spawn(async move {
                    let (skills_lock, dir) = {
                        let guard = inner_arc.read().await;
                        match guard.as_ref() {
                            Some(i) => (i.skills.clone(), i.config.skills_dir.clone()),
                            None => return,
                        }
                    };
                    let new_skills = Self::scan_skills(&dir).await;
                    // 计算 diff
                    let old_names: HashSet<String> = skills_lock
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .iter()
                        .map(|s| s.name().to_string())
                        .collect();
                    let new_names: HashSet<String> =
                        new_skills.iter().map(|s| s.name().to_string()).collect();
                    let added: Vec<&String> = new_names.difference(&old_names).collect();
                    let removed: Vec<&String> = old_names.difference(&new_names).collect();
                    // 原地替换（消费者下次调 all() 看到新数据）
                    *skills_lock.write().unwrap_or_else(|e| e.into_inner()) =
                        new_skills.into_iter().map(Arc::new).collect();
                    tracing::info!(
                        "SkillsService: 配置重载完成，新增 {} 个，移除 {} 个技能",
                        added.len(),
                        removed.len()
                    );
                    // 主动重注册，让持有旧 Arc 的消费者重新拉取
                    if let Some(ap) = ap_clone {
                        let bundle_struct = SkillsContractBundleImpl {
                            skills: skills_lock,
                        };
                        let bundle_trait: Arc<dyn SkillsContractBundle> = Arc::new(bundle_struct);
                        ap.register_provider(PROVIDER_SKILLS, Arc::new(DynProvider(bundle_trait)));
                    }
                });
            }
            ServiceSignal::Suspend => {
                if let Some(inner) = self.inner.write().await.as_mut() {
                    inner.suspended = true;
                }
            }
            ServiceSignal::Resume => {
                if let Some(inner) = self.inner.write().await.as_mut() {
                    inner.suspended = false;
                }
            }
            ServiceSignal::HealthCheck => {
                // B-5: 5s timeout 内读目录
                let dir = {
                    let guard = self.inner.read().await;
                    match guard.as_ref() {
                        Some(i) => i.config.skills_dir.clone(),
                        None => return Ok(()),
                    }
                };
                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    tokio::fs::read_dir(&dir),
                )
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => tracing::warn!("Skills HealthCheck: 读目录失败: {}", e),
                    Err(_) => tracing::warn!("Skills HealthCheck: 5s 超时"),
                }
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        if let Some(inner) = self.inner.write().await.as_mut() {
            inner.running = false;
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // B-7: 通过 self.ap 反注册 Provider
        if let Some(ap) = self.ap.take() {
            ap.unregister_provider(PROVIDER_SKILLS);
            tracing::info!("SkillsService: 已反注册 Provider PROVIDER_SKILLS");
        }
        self.inner.write().await.take();
        Ok(())
    }
}

impl SkillsService {
    /// 异步扫描技能目录
    ///
    /// B-2: 用 tokio::fs::read_dir（不用 std::fs::read_dir）
    /// B-3: 文件名以 .skill.md 结尾才加载（.md 但非 .skill.md 的文件被过滤）
    async fn scan_skills(dir: &Path) -> Vec<FileSkill> {
        if !dir.exists() {
            tracing::warn!("SkillsService: 技能目录不存在 '{}'", dir.display());
            return Vec::new();
        }
        let mut rd = match tokio::fs::read_dir(dir).await {
            Ok(rd) => rd,
            Err(e) => {
                tracing::warn!("SkillsService: 读取技能目录失败: {}", e);
                return Vec::new();
            }
        };
        let mut skills = Vec::new();
        let mut total_md = 0usize;
        let mut matched = 0usize;
        while let Some(entry) = rd.next_entry().await.transpose() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("SkillsService: 读取目录条目失败: {}", e);
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name() else {
                continue;
            };
            let name = name.to_string_lossy().to_string();
            if name.ends_with(".md") {
                total_md += 1;
            }
            if !name.ends_with(".skill.md") {
                continue;
            }
            match FileSkill::load(&path).await {
                Ok(skill) => {
                    matched += 1;
                    skills.push(skill);
                }
                Err(e) => {
                    tracing::warn!("SkillsService: 加载技能 '{}' 失败: {}", path.display(), e)
                }
            }
        }
        tracing::debug!(
            "SkillsService: scan_skills 完成，目录={}，共 {} 个 .md，其中 {} 个 .skill.md 匹配",
            dir.display(),
            total_md,
            matched
        );
        skills
    }
}

impl Default for SkillsService {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// 单元测试
// ============================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::access::{ProviderRegistry, ServiceAccessImpl};
    use crate::core::types::plugin::{AgentConfig, PluginInitContext};
    use std::sync::Mutex;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    /// 生成唯一 tempdir 路径（用当前 nanos 作后缀，避免并行测试冲突）
    fn unique_tempdir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("aagnet_test_{label}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &std::path::Path, content: &str) {
        std::fs::write(path, content).unwrap();
    }

    /// 清理 tempdir
    fn cleanup(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    const SKILL_CONTENT: &str = "\
---
name: alpha
title: Alpha
description: First skill
tags: [alpha]
---
## TL;DR
Alpha summary.
";

    const NOT_A_SKILL_CONTENT: &str = "\
---
name: beta
title: Beta
description: Plain markdown
---
## TL;DR
Should be filtered out.
";

    /// 测试用 ServiceAccessImpl mock——基于真实 ProviderRegistry + 调用追踪
    struct TestServiceAccess {
        registry: ProviderRegistry,
        register_calls: Mutex<Vec<String>>,
        unregister_calls: Mutex<Vec<String>>,
    }

    impl TestServiceAccess {
        fn new() -> Self {
            Self {
                registry: ProviderRegistry::new(),
                register_calls: Mutex::new(Vec::new()),
                unregister_calls: Mutex::new(Vec::new()),
            }
        }
        fn registered(&self) -> Vec<String> {
            self.register_calls.lock().unwrap().clone()
        }
        fn unregistered(&self) -> Vec<String> {
            self.unregister_calls.lock().unwrap().clone()
        }
    }

    impl ServiceAccessImpl for TestServiceAccess {
        fn get_config(&self) -> AgentConfig {
            AgentConfig::default()
        }
        fn log(&self, _level: &str, _message: &str) {}
        fn register_provider(&self, name: &str, _provider: Arc<dyn std::any::Any + Send + Sync>) {
            self.register_calls.lock().unwrap().push(name.to_string());
        }
        fn provider_raw(&self, name: &str) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
            self.registry.get_raw(name)
        }
        fn unregister_provider(&self, name: &str) {
            self.unregister_calls.lock().unwrap().push(name.to_string());
        }
    }

    fn make_init_ctx(
        data_dir: &std::path::Path,
        plugin_config: serde_json::Value,
    ) -> PluginInitContext {
        PluginInitContext::new(
            "skills",
            plugin_config,
            AgentConfig::default(),
            data_dir.to_path_buf(),
        )
    }

    // ── scan_skills 已有 4 个测试（D-1/Phase B 已覆盖）──

    #[tokio::test]
    async fn scan_skills_filters_by_skill_extension() {
        let dir = unique_tempdir("filter");
        write_file(&dir.join("alpha.skill.md"), SKILL_CONTENT);
        write_file(&dir.join("beta.md"), NOT_A_SKILL_CONTENT);
        let result = SkillsService::scan_skills(&dir).await;
        cleanup(&dir);
        assert_eq!(result.len(), 1, "应只加载 1 个技能（alpha.skill.md）");
        assert_eq!(result[0].name(), "alpha");
    }

    #[tokio::test]
    async fn scan_skills_returns_empty_for_nonexistent_dir() {
        let path = std::env::temp_dir().join("aagnet_test_does_not_exist_xyz_123");
        let result = SkillsService::scan_skills(&path).await;
        assert!(result.is_empty(), "不存在的目录应返回空 Vec");
    }

    #[tokio::test]
    async fn scan_skills_returns_empty_for_empty_dir() {
        let dir = unique_tempdir("empty");
        let result = SkillsService::scan_skills(&dir).await;
        cleanup(&dir);
        assert!(result.is_empty(), "空目录应返回空 Vec");
    }

    #[tokio::test]
    async fn scan_skills_loads_multiple_skill_files() {
        let dir = unique_tempdir("multi");
        write_file(
            &dir.join("one.skill.md"),
            &SKILL_CONTENT.replace("alpha", "one"),
        );
        write_file(
            &dir.join("two.skill.md"),
            &SKILL_CONTENT.replace("alpha", "two"),
        );
        let result = SkillsService::scan_skills(&dir).await;
        cleanup(&dir);
        assert_eq!(result.len(), 2, "应加载 2 个技能");
        let names: HashSet<String> = result.iter().map(|s| s.name().to_string()).collect();
        assert!(names.contains("one"));
        assert!(names.contains("two"));
    }

    // ── D-2 生命周期测试 ──

    /// 构造 plugin_config 指向 tempdir（让 resolve_paths 把 data_dir/skills_dir 拼起来）
    fn skills_dir_config(dir: &std::path::Path) -> serde_json::Value {
        serde_json::json!({ "skills_dir": dir.to_string_lossy() })
    }

    #[tokio::test]
    async fn init_scans_skills_dir() {
        let dir = unique_tempdir("init_scan");
        write_file(&dir.join("first.skill.md"), SKILL_CONTENT);
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.expect("init 应成功");
        cleanup(&dir);
        let count = svc
            .inner
            .read()
            .await
            .as_ref()
            .unwrap()
            .skills
            .read()
            .unwrap()
            .len();
        assert_eq!(count, 1, "init 后应加载 1 个技能");
    }

    #[tokio::test]
    async fn init_skips_non_skill_md() {
        let dir = unique_tempdir("init_skip");
        write_file(&dir.join("good.skill.md"), SKILL_CONTENT);
        write_file(&dir.join("bad.md"), NOT_A_SKILL_CONTENT);
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.unwrap();
        cleanup(&dir);
        let count = svc
            .inner
            .read()
            .await
            .as_ref()
            .unwrap()
            .skills
            .read()
            .unwrap()
            .len();
        assert_eq!(count, 1, "init 后应只加载 1 个（.skill.md）");
    }

    #[tokio::test]
    async fn init_propagates_config_error() {
        let dir = unique_tempdir("init_err");
        let mut svc = SkillsService::new();
        // 无效 JSON（数组而非对象）—— SkillConfig 期望 object
        let ctx = make_init_ctx(&dir, serde_json::json!([1, 2, 3]));
        let result = svc.init(&ctx).await;
        cleanup(&dir);
        assert!(result.is_err(), "无效 JSON 应传播错误而非吞掉");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("SkillsService"),
            "错误消息应提及 SkillsService: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn start_registers_provider() {
        let dir = unique_tempdir("start_reg");
        write_file(&dir.join("a.skill.md"), SKILL_CONTENT);
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.unwrap();
        let mock = Arc::new(TestServiceAccess::new());
        let ap = ServiceAccessPoint::new(mock.clone());
        svc.start(ap).await.expect("start 应成功");
        cleanup(&dir);
        // mock 应收到 1 次 register_provider 调用，name=PROVIDER_SKILLS
        let calls = mock.registered();
        assert_eq!(calls.len(), 1, "应只 1 次 register_provider 调用");
        assert_eq!(calls[0], PROVIDER_SKILLS, "key 应是 PROVIDER_SKILLS");
    }

    #[tokio::test]
    async fn handle_signal_healthcheck_returns_within_5s() {
        let dir = unique_tempdir("health");
        write_file(&dir.join("a.skill.md"), SKILL_CONTENT);
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.unwrap();
        let mock = Arc::new(TestServiceAccess::new());
        let ap = ServiceAccessPoint::new(mock.clone());
        svc.start(ap).await.unwrap();
        // 健康检查应 5s 内返回
        let start = Instant::now();
        svc.handle_signal(ServiceSignal::HealthCheck)
            .await
            .expect("HealthCheck 应成功");
        let elapsed = start.elapsed();
        cleanup(&dir);
        assert!(
            elapsed < std::time::Duration::from_millis(5_500),
            "HealthCheck 应在 5.5s 内返回，实际: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn handle_signal_healthcheck_for_nonexistent_dir_still_fast() {
        // 不存在的目录也应快速返回（不 panic）
        let dir = unique_tempdir("health_empty");
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.unwrap();
        // init 后 inner.skills 是空，HealthCheck 也会走 (但目录是空 dir)
        // 删除目录模拟不存在场景
        cleanup(&dir);
        let start = Instant::now();
        let result = svc.handle_signal(ServiceSignal::HealthCheck).await;
        let elapsed = start.elapsed();
        assert!(result.is_ok(), "HealthCheck 对不存在目录应 Ok(())");
        assert!(
            elapsed < std::time::Duration::from_secs(6),
            "应 6s 内返回（含 5s timeout buffer）"
        );
    }

    #[tokio::test]
    async fn handle_signal_configreload_spawns() {
        let dir = unique_tempdir("reload");
        write_file(&dir.join("a.skill.md"), SKILL_CONTENT);
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.unwrap();
        let mock = Arc::new(TestServiceAccess::new());
        let ap = ServiceAccessPoint::new(mock.clone());
        svc.start(ap).await.unwrap();
        // 1 次 register 来自 start()
        assert_eq!(mock.registered().len(), 1);
        // 触发 ConfigReload（异步 spawn）
        svc.handle_signal(ServiceSignal::ConfigReload)
            .await
            .unwrap();
        // 等待 spawn 任务完成（最多 2s）
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if mock.registered().len() >= 2 {
                break;
            }
        }
        cleanup(&dir);
        // ConfigReload 会再 register 一次（重注册用新数据）
        let calls = mock.registered();
        assert!(
            calls.len() >= 2,
            "ConfigReload 应触发至少 1 次额外 register_provider，实际 {} 次",
            calls.len()
        );
        assert!(
            calls.iter().all(|c| c == PROVIDER_SKILLS),
            "所有 register 都应使用 PROVIDER_SKILLS"
        );
    }

    #[tokio::test]
    async fn shutdown_unregisters_provider() {
        let dir = unique_tempdir("shutdown");
        write_file(&dir.join("a.skill.md"), SKILL_CONTENT);
        let mut svc = SkillsService::new();
        let ctx = make_init_ctx(&dir, skills_dir_config(&dir));
        svc.init(&ctx).await.unwrap();
        let mock = Arc::new(TestServiceAccess::new());
        let ap = ServiceAccessPoint::new(mock.clone());
        svc.start(ap).await.unwrap();
        assert_eq!(mock.registered().len(), 1);
        // shutdown 应反注册
        svc.shutdown().await.unwrap();
        cleanup(&dir);
        let unregs = mock.unregistered();
        assert_eq!(unregs.len(), 1, "shutdown 应调用 1 次 unregister_provider");
        assert_eq!(unregs[0], PROVIDER_SKILLS);
    }
}
