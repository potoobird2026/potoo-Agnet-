# CompressionService（上下文压缩引擎）严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/compression/compression开发文档.md` 生成 compression 模块的全部代码。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：compression（上下文压缩引擎）
- **模块定位**：同时包含 `ServicePlugin`（后台压缩服务）和 `SlotPlugin`（Memorize 阶段钩子）的双层架构。后台 500ms tick 驱动状态机（SLEEP → COMPRESSING → SLEEP），12 个组件通过 Orchestrator 编排，六算法协同（PID + 熵 + 加权评分 + UCB + 模糊控制 + 锚点）实现自适应上下文压缩。
- **外部接口**：
  - `CompressionService` — Service 入口
  - `CompressionHookSlot` — Slot 入口
  - `CompressionConfig` — 配置
- **内部结构**：
  - `components/` — 12 个 Component 实现（每个含业务 trait + Component trait impl）
  - `services/` — 12 个业务接口 trait 定义
  - `orchestrator.rs` — Orchestrator 编排器
  - `executors/` — 预留
- **依赖项**：`tokio`、`serde`、`serde_json`、`tracing`、`async-trait`、`thiserror`

---

## 硬编码专项预防纲领

### 硬编码分类定义（compression 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 数字阈值 | `temperature = 0.3` | 从 `CompressionConfig.summary_temperature` 读取（v0.2 计划，v0.1 硬编码标注为技术债） |
| Token 计数常量 | `CJK 乘数 1.5` | 定义为模块级 `const CJK_MULTIPLIER: f64` 集中管理，非业务配置 |
| 主循环间隔 | `Duration::from_millis(500)` | 定义为 `const MAIN_LOOP_TICK_MS: u64` 常量 |
| 冷启动阈值 | `collect_messages = 50` | 从 `ColdStartConfig.collect_messages` 读取 |
| CAS 重试次数 | `max_retries = 3` | 定义为 `const CAS_MAX_RETRIES: u32` 常量 |
| 阶段检测阈值 | `busy_round_interval_ms` | 从 `CompressionConfig` 读取 |
| 锚点参数 | `anchor_min` / `anchor_max` | 从 `CompressionConfig` 读取 |
| UCB 参数 | `threshold_high` / `threshold_low` | 从 `CompressionConfig` 读取 |
| 模糊控制参数 | 隶属度函数阈值 | 从 `CompressionConfig` 读取 |

---

## 项目目录结构

```
src/plugins/services/compression/
├── mod.rs                           # 模块入口 + pub use
├── config.rs                        # 配置类型（CompressionConfig / ColdStartConfig 等）
├── service.rs                       # CompressionService（ServicePlugin 实现）
├── slot.rs                          # CompressionHookSlot（SlotPlugin 实现）
├── types.rs                         # 公共数据类型（HookEvent / ServiceState 等）
├── errors.rs                        # 错误类型（CompressError / ComponentError）
├── orchestrator.rs                  # Orchestrator（组件协调器）
├── component.rs                     # Component trait + ComponentMeta + ComponentHandle + AccessPoint
├── services/                        # 业务接口 trait（每个服务一个文件）
│   ├── mod.rs
│   ├── pid_service.rs
│   ├── token_counter_service.rs
│   ├── anchor_service.rs
│   ├── entity_extractor_service.rs
│   ├── entropy_service.rs
│   ├── scorer_service.rs
│   ├── ucb_decision_service.rs
│   ├── fuzzy_control_service.rs
│   ├── compressor_service.rs
│   ├── feedback_service.rs
│   ├── recall_service.rs
│   └── journal_service.rs
└── components/                      # Component 实现（每个组件一个文件）
    ├── mod.rs
    ├── pid_controller.rs
    ├── token_counter.rs
    ├── anchor.rs
    ├── entity_extractor.rs
    ├── entropy.rs
    ├── scorer.rs
    ├── ucb_decision.rs
    ├── fuzzy_control.rs
    ├── compressor.rs
    ├── feedback.rs
    ├── recall.rs
    └── journal.rs
```

---

## AI 宪法

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、组件依赖关系、状态转换规则，必须与 `compression开发文档.md` 完全一致。

2. **零幻觉**：
   - compression 只有 12 个组件（PidController / TokenCounter / Anchor / EntityExtractor / Entropy / Scorer / UcbDecision / FuzzyControl / Compressor / Feedback / Recall / Journal），不凭空生成第 13 个
   - 每个组件只提供一个业务 trait，不凭空生成第二个
   - `ServiceState` 只有 SLEEP 和 COMPRESSING 两态
   - Orchestrator 不支持自动环检测（当前 DAG 是静态已知无环的）

3. **零硬编码**：
   a. Token 计数常量（CJK 1.5 / ASCII 0.25 / Image 85 / Audio 100 / File 50）定义为模块级 const
   b. 主循环 tick 500ms 定义为 `MAIN_LOOP_TICK_MS` 常量
   c. CAS 重试 3 次定义为 `CAS_MAX_RETRIES` 常量
   d. 所有数字阈值从 `CompressionConfig` / 子配置读取
   e. SUMMARY_SYSTEM_PROMPT 和 DETECTION_SYSTEM_PROMPT 定义为常量（字符串模板可接受）
   f. v0.1 中 temperature=0.3 和 max_tokens=1024 标注硬编码技术债，v0.2 迁移到配置

4. **完整实现**：每个 Component 的 `init()` / `process()` / `shutdown()` 必须有完整实现。`process()` 当前为 no-op（文档 §9.2 设计决策明确）。

5. **错误处理**：CAS 写回 3 次失败后放弃，记录 `CompressionCasConflict`。LLM 调用失败记录 warn 并降级为纯文本摘要。不允许 `unwrap()`（测试除外）。

6. **一致性**：组件名、方法名、字段名必须与文档完全一致。

7. **禁止额外依赖**：只允许 std、tokio、serde、serde_json、tracing、async-trait、thiserror。

8. **测试同步生成**：
   - 每个 Component 独立单元测试
   - PidController 测试 PID 公式正确性 + 阶段检测
   - Anchor 测试锚点窗口计算公式
   - Entropy 测试 Shannon 熵计算
   - Scorer 测试 4 维度加权
   - UCB 测试 9×3×3 分类 + 探索/利用
   - Compressor 测试分批压缩策略
   - Service 集成测试 mock mpsc 通道

9. **组件边界**：每个 Component 的 process() 是 no-op（设计决策 §9.2）。Service 主循环直接调用业务 trait 方法。

10. **日志规范**：压缩开始/完成记录 info，CAS 冲突记录 warn，LLM 失败记录 warn，主循环 tick 记录 debug。
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架

**操作**：确认依赖、创建目录结构、`cargo check` 通过

**验收**：目录结构完整，无编译 error

---

### 步骤 1：生成类型层（config.rs + types.rs + errors.rs + component.rs）

**1.1 config.rs — 配置类型**

| 结构体/枚举 | 说明 |
|------------|------|
| `ConversationPhase` | Idle / Busy / ToolHeavy |
| `ColdStartPhase` | Collect / Conservative / Steady |
| `ColdStartConfig` | enabled, collect_messages(50), steady_messages(150), conservative_factor_threshold(1.2), conservative_factor_target(1.3) |
| `DensityConfig` | enabled, density_threshold, density_weight |
| `FeedbackConfig` | enabled, detection_interval, max_window_size, gamma, min_penalty, penalty_mult, pos_window |
| `CompressionConfig` | target_tokens(76800), min_keep_tokens(4000), batch_size(50), enable_journal(true), cold_start, feedback + `pid_coefficients()` 方法 |

**1.2 types.rs — 公共数据类型**

- `HookEvent` 枚举：`NewMessagesArrived { session_id }` / `RoundComplete { session_id, round_id, interval_ms }`
- `ServiceState` 枚举：`Sleep` / `Compressing`
- `CompressResult` 结构体：original_tokens, compressed_tokens, compression_ratio, compressed_range, summary
- `LossSignal` 结构体：lost_info, topic, severity
- `RecallAction` 枚举：None / RestoreMessages / InjectSystemMessage
- `JournalEntry` 结构体：round_id, compressed_range, summary, original_tokens, compressed_tokens, compression_ratio
- `FineCategory` / `CoarseCategory` / `CategoryRole` / `ContentType` / `LengthBucket` / `UcbDecision` / `FuzzyDecision`

**1.3 errors.rs**

- `CompressError` 枚举：LlmError(String) / Timeout(Duration) / ReplaceError(String) / InvalidRange { start, end, total }

**1.4 component.rs — Component 框架**

- `ComponentMeta` 结构体：name, version, priority, provides, requires, config_key
- `Component` trait：meta() / init(ctx) / process(ap) / shutdown()
- `ComponentHandle` trait：name() / as_any() / as_any_mut()
- `AccessPoint` trait：read_any() / write_any() / call() / config()
- `ComponentError` 枚举：Config / Internal / NotFound
- `Processing` 枚举：Continue / BreakChain / Restart / Warn
- `Orchestrator`：register() / init_all() / process_all() / shutdown_all()

**验收**：`cargo check` 通过，所有类型可序列化/反序列化

---

### 步骤 2：生成层级 1 组件（priority=10，无依赖）

**2.1 PidControllerComponent**

- 业务 trait `PidControllerService`：update(current, target, min_keep, config) → usize / reset() / set_phase() / set_tool_ratio() / record_round_interval()
- PID 公式：`u(t) = Kp×e(t) + Ki×Σe(i) + Kd×[e(t)-e(t-1)]`
- 对话阶段检测：busy_threshold / busy_consecutive_rounds / tool_heavy_ratio

**2.2 TokenCounterComponent**

- 业务 trait `TokenCounterService`：count_message(msg) → usize / count_messages(messages) → usize
- 常量：`CJK_MULTIPLIER = 1.5` / `ASCII_MULTIPLIER = 0.25` / `IMAGE_TOKENS = 85` / `AUDIO_TOKENS = 100` / `FILE_TOKENS = 50`

**2.3 AnchorComponent**

- 业务 trait `AnchorService`：calculate_anchor_window(messages, config, pid_delta, current_tokens) → usize / calculate_anchor_range(messages, config, pid_delta, current_tokens) → (usize, usize)
- 公式：`N = max(N_min, ceil(H_recent / H_avg × base_N_adjusted))`

**2.4 EntityExtractorComponent**

- 业务 trait `EntityExtractorService`：extract_entities(text) → HashSet / has_unique_entity(candidate, retained_texts) → bool
- 正则模式从 `SelfReferenceConfig.entity_patterns` 读取

**验收**：每个组件独立测试通过

---

### 步骤 3：生成层级 2 组件（priority=20，依赖层级 1）

**3.1 EntropyComponent**

- 业务 trait `EntropyService`：calculate_message_entropy(msg) → f64 / calculate_messages_entropy(messages) → Vec / calculate_density(msg, config) → f64
- Shannon 熵公式：`H(m) = -Σ p(w) × log₂(p(w))`
- requires: ["token_count"]

**3.2 ScorerComponent**

- 业务 trait `ScorerService`：score_message(msg, position, total, entropy, reference_count, config) → f64 / score_messages(messages, entropies, reference_counts, config) → Vec / record_success() / record_failure()
- 4 维度评分：entropy / position / length / reference_count
- requires: ["token_count"]

**验收**：熵计算正确性测试、评分分配测试

---

### 步骤 4：生成层级 3 组件（priority=30，依赖层级 2）

**4.1 UcbDecisionComponent**

- 业务 trait `UcbDecisionService`：categorize_message(msg) → FineCategory / get_ucb(category) → f64 / decide(category, config) → UcbDecision / record_success(category) / record_failure(category)
- 3 维分类：`CategoryRole(3) × ContentType(3) × LengthBucket(3)` = 27 细类
- requires: ["scoring"]

**4.2 FuzzyControlComponent**

- 业务 trait `FuzzyControlService`：decide(importance, entropy, ucb_uncertainty, config) → FuzzyDecision
- 模糊规则引擎：3 输入（重要性/熵/UCB 不确定度）→ 1 输出（压缩置信度）
- requires: ["scoring"]

**验收**：UCB 探索/利用平衡测试、模糊规则覆盖测试

---

### 步骤 5：生成层级 4 组件（priority=40，依赖层级 3）

**5.1 CompressorComponent**

- 业务 trait `CompressorService`：compress_range(messages, start, end, config) → Result<CompressResult, CompressError> / replace_messages(messages, start, end, summary) / reset() / update_summary() / existing_summary()
- 分批策略：单批 ≤ batch_size → 直接摘要；多批 → 递归合并摘要
- 降级：无 LLM 合约时使用模板摘要 `"[摘要] {} 条消息"`
- requires: ["pid", "anchor", "ucb_decision", "fuzzy_decision"]

**5.2 FeedbackComponent**

- 业务 trait `FeedbackService`：detect_feedback(user_message, context_summary, llm) → LossSignal / apply_eligibility(signal, scorer, ucb) / should_detect() → bool / record_round(categories, compressed_tokens)
- 资格迹回溯：最近 N 轮压缩决策的 eligibility_window
- requires: ["scoring", "ucb_decision"]

**5.3 RecallComponent**

- 业务 trait `RecallService`：recall(session_id, signal_topic, journal_entries) → RecallAction
- requires: ["compress"]

**5.4 JournalComponent**

- 业务 trait `JournalService`：add_entry(entry) / search_summary(query) → Vec / get_loss_entries() → Vec / update_loss_status(round_id, topic)
- requires: ["compress"]

**验收**：分批压缩测试、反馈检测 mock LLM 测试、日志增删查测试

---

### 步骤 6：生成 Orchestrator + Service + Slot

**6.1 orchestrator.rs**

- 在 `CompressionService::init()` 中注册 12 个组件
- 按 priority 排序，拓扑分层（4 层）
- `init_all()` / `process_all()` / `shutdown_all()`

**6.2 service.rs — CompressionService**

- 实现 `ServicePlugin` 生命周期：name("compression") / init() / start() / handle_signal() / stop() / shutdown()
- `run_loop()` — 500ms tick，`tokio::select!` 监听 HookEvent channel + tick
- 状态机：SLEEP → COMPRESSING → SLEEP
- `check_and_compress()` — 冷启动阶段判断 → token 检查 → compress_round()
- `compress_round()` — 10 子步骤完整流程（§4.1）
- CAS 写回：compare_and_write → 重试 3 次

**6.3 slot.rs — CompressionHookSlot**

- 实现 `SlotPlugin`：name("compression_hook") / phases([Memorize]) / run()
- `run()` 发送 `NewMessagesArrived` + `RoundComplete`
- 追踪 round_id (AtomicUsize) 和 last_run (Mutex<Instant>)

**6.4 Provider 注册（start 中）**

```
ap.register_provider("compression_summary", Arc::new(self.clone()));
ap.register_provider("compression_feedback", Arc::new(self.clone()));
```

**验收**：Service 主循环 mock 测试、Slot 事件发送测试、状态机转换测试

---

### 步骤 7：终态自检

1. `cargo test --all` 全量通过
2. 验证 12 个组件全部注册
3. 验证 4 层 DAG 拓扑排序正确
4. 硬编码扫描：确认所有数字阈值从配置读取
5. 对照 `compression开发文档.md` §7.4 的 10 项自查清单
