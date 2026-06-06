# CompressionService(上下文压缩引擎) 设计文档

## 0. 协议依据

| 协议 | 应用层 | 关键条款 |
|------|--------|---------|
| **Service 集成协议** | 模块对外接口 | §1 插件单入口、§2 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§7 新增/替换流程、§8 红线 |
| **模块内部组件协议** | 模块内部结构 | §1 组件单入口、§2 组件句柄、§3 内部数据共享通道、§4 处理结果、§5 组件元数据声明、§5.2 Orchestrator、§6 模块边界规范、§9 设计决策、§10 新增/替换流程、§11 红线 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单 |

---

## 0.5 功能清单

| 功能 | 描述 | 对应 Component | 优先级 |
|------|------|---------------|--------|
| PID 控制 | 基于 token 差异的自适应压缩强度 | `PidControllerComponent` | P0 |
| Token 计数 | 消息 token 数量估算 | `TokenCounterComponent` | P0 |
| 动态锚点 | 确定压缩范围的锚点窗口 | `AnchorComponent` | P0 |
| 信息熵计算 | 消息信息密度评估 | `EntropyComponent` | P0 |
| 自适应加权评分 | 多维度消息重要性评分 | `ScorerComponent` | P0 |
| 分层 UCB 决策 | 基于多臂老虎机的消息分类 | `UcbDecisionComponent` | P0 |
| 模糊控制 | 边界情况的软决策 | `FuzzyControlComponent` | P0 |
| 实体提取 | NER 实体排重 | `EntityExtractorComponent` | P1 |
| 摘要生成 | 调用 LLM 生成压缩摘要 | `CompressorComponent` | P0 |
| 反馈检测 | 检测压缩后信息丢失 | `FeedbackComponent` | P1 |
| 信息恢复 | 恢复被误压缩的信息 | `RecallComponent` | P1 |
| 压缩日志 | 记录压缩历史 | `JournalComponent` | P1 |

---

## 1. 模块定位（Service 集成协议视角）

### 1.1 双层架构

Compression 同时包含 Service 和 Slot：

```
┌──────────────────────────────────────────────────────────────────┐
│  CompressionService (ServicePlugin)                               │
│  - 后台常驻服务                                                   │
│  - 接收 HookEvent，执行压缩                                        │
│  - 状态机：SLEEP → COMPRESSING → SLEEP                            │
│  - 持有 12 个算法/引擎组件                                         │
└──────────────────────────────────────────────────────────────────┘
          ↑ mpsc channel
┌──────────────────────────────────────────────────────────────────┐
│  CompressionHookSlot (SlotPlugin)                                 │
│  - 在 Memorize 阶段运行                                          │
│  - 发送 HookEvent 到 Service                                     │
│  - 追踪 round_id 和轮次间隔                                      │
└──────────────────────────────────────────────────────────────────┘
```

### 1.2 外部身份（Service 层，§1）

```rust
#[async_trait]
impl ServicePlugin for CompressionService {
    fn name(&self) -> &str;
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError>;
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError>;
    async fn stop(&mut self) -> Result<(), PluginError>;
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

| 方法 | 调用次数 | 用途 |
|------|---------|------|
| `name` | 多次 | 返回 `"compression"` |
| `init` | 1 | 解析 CompressionConfig；创建 12 个组件 |
| `start` | 1 | 注册 Provider；启动后台主循环 |
| `handle_signal` | 多次 | 响应信号（§5） |
| `stop` | 多次 | 暂停，不销毁资源 |
| `shutdown` | 1 | 反注册 Provider、释放所有资源 |

### 1.3 受控访问句柄（ServiceAccessPoint，§2）

```rust
async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
    // §2.1
    let config = ap.get_config();
    ap.log("info", "Compression 服务启动");

    // §2.2 Provider 注册
    let summary: Arc<dyn CompressionSummaryContract> = Arc::new(self.clone());
    ap.register_provider("compression_summary", summary);

    let feedback: Arc<dyn CompressionFeedbackContract> = Arc::new(self.clone());
    ap.register_provider("compression_feedback", feedback);

    // 启动后台主循环
    self.run_loop().await;
    Ok(())
}
```

### 1.4 元数据声明（§4）

**Service 层**：

```yaml
name: compression
category: service
version: 0.1.0
run_mode: background
provides:
  - compression_summary
  - compression_feedback
requires: []
conflicts: []
```

**Slot 层**：

```yaml
name: compression_hook
category: slot
version: 0.1.0
permissions:
  - context:read
```

### 1.5 生命周期映射（§5）

**Service 层**：

| 阶段 | 具体操作 |
|------|---------|
| `init(ctx)` | 解析 CompressionConfig；创建 12 个组件并注册到 Orchestrator |
| `start(ap)` | 注册 Provider；启动后台主循环（500ms tick） |
| `handle_signal(signal)` | 6 种信号处理（§5） |
| `stop()` | 设置 `running = false` |
| `shutdown()` | 反注册 Provider、释放资源 |

**Slot 层**：

| 阶段 | 具体操作 |
|------|---------|
| `init(ctx)` | 创建 mpsc channel |
| `run(ap)` | 发送 `HookEvent::NewMessagesArrived` + `HookEvent::RoundComplete` |
| `shutdown()` | 释放 channel |

### 1.6 Provider 注册（§2.2，§8 V-R03）

```rust
ap.register_provider("compression_summary", Arc::new(self.clone()));
ap.register_provider("compression_feedback", Arc::new(self.clone()));
```

`provides` 声明与 `register_provider` 调用一致（V-R03 合规）。

---

## 2. 内部架构总览（模块内部组件协议视角）

### 2.1 模块边界规范（§6）

```rust
// mod.rs 只暴露 3 样
pub struct CompressionService;    // Service 入口
pub struct CompressionHookSlot;   // Slot 入口
pub struct CompressionConfig;     // 配置
// 内部算法、引擎、存储全部 pub(crate) 或 private
```

### 2.2 组件依赖关系（DAG，§5）

```
层级 1 (priority=10, 无依赖):
  PidControllerComponent     provides: ["pid"]
  TokenCounterComponent      provides: ["token_count"]
  AnchorComponent            provides: ["anchor"]
  EntityExtractorComponent   provides: ["entity_extract"]

层级 2 (priority=20, 依赖层级 1):
  EntropyComponent           provides: ["entropy"]           requires: ["token_count"]
  ScorerComponent            provides: ["scoring"]           requires: ["token_count"]

层级 3 (priority=30, 依赖层级 2):
  UcbDecisionComponent       provides: ["ucb_decision"]      requires: ["scoring"]
  FuzzyControlComponent      provides: ["fuzzy_decision"]    requires: ["scoring"]

层级 4 (priority=40, 依赖层级 3):
  CompressorComponent        provides: ["compress"]          requires: ["pid", "anchor", "ucb_decision", "fuzzy_decision"]
  FeedbackComponent          provides: ["feedback"]          requires: ["scoring", "ucb_decision"]
  RecallComponent            provides: ["recall"]            requires: ["compress"]
  JournalComponent           provides: ["journal"]           requires: ["compress"]
```

| 层级 | 组件 | priority | provides | requires |
|------|------|----------|----------|----------|
| 1 | PidControllerComponent | 10 | `pid` | — |
| 1 | TokenCounterComponent | 10 | `token_count` | — |
| 1 | AnchorComponent | 10 | `anchor` | — |
| 1 | EntityExtractorComponent | 10 | `entity_extract` | — |
| 2 | EntropyComponent | 20 | `entropy` | `token_count` |
| 2 | ScorerComponent | 20 | `scoring` | `token_count` |
| 3 | UcbDecisionComponent | 30 | `ucb_decision` | `scoring` |
| 3 | FuzzyControlComponent | 30 | `fuzzy_decision` | `scoring` |
| 4 | CompressorComponent | 40 | `compress` | `pid`, `anchor`, `ucb_decision`, `fuzzy_decision` |
| 4 | FeedbackComponent | 40 | `feedback` | `scoring`, `ucb_decision` |
| 4 | RecallComponent | 40 | `recall` | `compress` |
| 4 | JournalComponent | 40 | `journal` | `compress` |

---

## 3. Component(组件) 详解

### 3.1 PidControllerComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "pid_controller",
    version: "0.1.0",
    priority: 10,
    provides: &["pid"],
    requires: &[],
    config_key: Some("compression.pid"),
}
```

#### 业务接口 trait（§9.1）

```rust
pub trait PidControllerService: Send + Sync {
    fn update(&mut self, current: usize, target: usize, min_keep: usize, config: &CompressionConfig) -> usize;
    fn reset(&mut self);
    fn set_phase(&mut self, phase: ConversationPhase, intensity: f64);
    fn set_tool_ratio(&mut self, ratio: f64, config: &CompressionConfig);
    fn record_round_interval(&mut self, interval_ms: u64);
}
```

#### Component 实现

```rust
impl Component for PidControllerComponent {
    fn name(&self) -> &str { "pid_controller" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 PID 系数（跨平台规范 §1：数字阈值从配置读取）
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    /// PID 控制器由 Service 主循环直接调用 update() 驱动
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.2 TokenCounterComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "token_counter",
    version: "0.1.0",
    priority: 10,
    provides: &["token_count"],
    requires: &[],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait TokenCounterService: Send + Sync {
    fn count_message(&self, msg: &Message) -> usize;
    fn count_messages(&self, messages: &[Message]) -> usize;
}
```

#### Component 实现

```rust
impl Component for TokenCounterComponent {
    fn name(&self) -> &str { "token_counter" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §10 设计决策：process() 为 no-op
    /// Token 计数由 CompressorComponent 直接调用 count_messages() 驱动
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

#### 硬编码说明（§1）

Token 计数包含算法常量（CJK 乘数 1.5、ASCII 乘数 0.25、Image 85 tokens、Audio 100 tokens、File 50 tokens）。这些是**语言学特征和模型行为特征**，不是业务配置，不应由用户配置。定义为模块级 `const` 集中管理。

### 3.3 AnchorComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "anchor",
    version: "0.1.0",
    priority: 10,
    provides: &["anchor"],
    requires: &[],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait AnchorService: Send + Sync {
    fn calculate_anchor_window(&self, messages: &[Message], config: &CompressionConfig, pid_delta: usize, current_tokens: usize) -> usize;
    fn calculate_anchor_range(&self, messages: &[Message], config: &CompressionConfig, pid_delta: usize, current_tokens: usize) -> (usize, usize);
}
```

#### Component 实现

```rust
impl Component for AnchorComponent {
    fn name(&self) -> &str { "anchor" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.4 EntropyComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "entropy",
    version: "0.1.0",
    priority: 20,
    provides: &["entropy"],
    requires: &["token_count"],
    config_key: Some("compression.density"),
}
```

#### 业务接口 trait

```rust
pub trait EntropyService: Send + Sync {
    fn calculate_message_entropy(&self, msg: &Message) -> f64;
    fn calculate_messages_entropy(&self, messages: &[Message]) -> Vec<f64>;
    fn calculate_density(&self, msg: &Message, config: &DensityConfig) -> f64;
}
```

#### Component 实现

```rust
impl Component for EntropyComponent {
    fn name(&self) -> &str { "entropy" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 DensityConfig
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.5 ScorerComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "scorer",
    version: "0.1.0",
    priority: 20,
    provides: &["scoring"],
    requires: &["token_count"],
    config_key: Some("compression"),
}
```

#### 业务接口 trait

```rust
pub trait ScorerService: Send + Sync {
    fn score_message(&self, msg: &Message, position: usize, total: usize, entropy: f64, reference_count: f64, config: &CompressionConfig) -> f64;
    fn score_messages(&self, messages: &[Message], entropies: &[f64], reference_counts: &[f64], config: &CompressionConfig) -> Vec<f64>;
    fn record_success(&mut self);
    fn record_failure(&mut self);
}
```

#### Component 实现

```rust
impl Component for ScorerComponent {
    fn name(&self) -> &str { "scorer" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取权重配置
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.6 UcbDecisionComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "ucb_decision",
    version: "0.1.0",
    priority: 30,
    provides: &["ucb_decision"],
    requires: &["scoring"],
    config_key: Some("compression"),
}
```

#### 业务接口 trait

```rust
pub trait UcbDecisionService: Send + Sync {
    fn categorize_message(msg: &Message) -> FineCategory;
    fn get_ucb(&self, category: &FineCategory) -> f64;
    fn decide(&self, category: &FineCategory, config: &CompressionConfig) -> UcbDecision;
    fn record_success(&mut self, category: &FineCategory);
    fn record_failure(&mut self, category: &FineCategory);
}
```

#### Component 实现

```rust
impl Component for UcbDecisionComponent {
    fn name(&self) -> &str { "ucb_decision" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 UCB 配置（threshold_high, threshold_low, exploration_c 等）
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.7 FuzzyControlComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "fuzzy_control",
    version: "0.1.0",
    priority: 30,
    provides: &["fuzzy_decision"],
    requires: &["scoring"],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait FuzzyControlService: Send + Sync {
    fn decide(&self, importance: f64, entropy: f64, ucb_uncertainty: f64, config: &CompressionConfig) -> FuzzyDecision;
}
```

#### Component 实现

```rust
impl Component for FuzzyControlComponent {
    fn name(&self) -> &str { "fuzzy_control" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.8 EntityExtractorComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "entity_extractor",
    version: "0.1.0",
    priority: 10,
    provides: &["entity_extract"],
    requires: &[],
    config_key: Some("compression.self_reference"),
}
```

#### 业务接口 trait

```rust
pub trait EntityExtractorService: Send + Sync {
    fn extract_entities(&self, text: &str) -> HashSet<String>;
    fn has_unique_entity(&self, candidate_text: &str, retained_texts: &[String]) -> bool;
}
```

#### Component 实现

```rust
impl Component for EntityExtractorComponent {
    fn name(&self) -> &str { "entity_extractor" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 SelfReferenceConfig（entity_patterns, min_entity_length）
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.9 CompressorComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "compressor",
    version: "0.1.0",
    priority: 40,
    provides: &["compress"],
    requires: &["pid", "anchor", "ucb_decision", "fuzzy_decision"],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait CompressorService: Send + Sync {
    async fn compress_range(&mut self, messages: &[Message], start: usize, end: usize, config: &CompressionConfig) -> Result<CompressResult, CompressError>;
    fn replace_messages(messages: &mut Vec<Message>, start: usize, end: usize, summary: &str) -> Result<(), CompressError>;
    fn reset(&mut self);
    fn update_summary(&mut self, summary: String);
    fn existing_summary(&self) -> &str;
}
```

#### Component 实现

```rust
impl Component for CompressorComponent {
    fn name(&self) -> &str { "compressor" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 设置 LLM 摘要合约（从配置读取）
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

#### 硬编码说明（§1）

CompressorComponent 包含硬编码值：

| 值 | 位置 | 说明 | 处理 |
|---|------|------|------|
| `SUMMARY_SYSTEM_PROMPT` | compressor.rs:48 | LLM 系统提示词 | 字符串模板类别（§1：可接受） |
| `temperature = 0.3` | compressor.rs:169 | LLM 温度 | 应从配置读取（§1 超时秒数类别） |
| `max_tokens = 1024` | compressor.rs:170 | LLM 最大 token | 应从配置读取（§1 数字阈值类别） |
| `"[摘要] {} 条消息"` | compressor.rs:193 | 降级摘要模板 | 字符串模板类别（§1：可接受） |

### 3.10 FeedbackComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "feedback",
    version: "0.1.0",
    priority: 40,
    provides: &["feedback"],
    requires: &["scoring", "ucb_decision"],
    config_key: Some("compression.feedback"),
}
```

#### 业务接口 trait

```rust
pub trait FeedbackService: Send + Sync {
    async fn detect_feedback(&self, user_message: &str, context_summary: &str, llm: Option<&dyn LlmContract>) -> LossSignal;
    fn apply_eligibility(&self, signal: &LossSignal, scorer: &mut Scorer, ucb: &mut HierarchicalUCB);
    fn apply_positive_reinforcement(&self, scorer: &mut Scorer, ucb: &mut HierarchicalUCB);
    fn should_detect(&mut self) -> bool;
    fn record_round(&mut self, categories: Vec<FineCategory>, compressed_tokens: usize);
}
```

#### Component 实现

```rust
impl Component for FeedbackComponent {
    fn name(&self) -> &str { "feedback" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 FeedbackConfig
        Ok(())
    }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.11 RecallComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "recall",
    version: "0.1.0",
    priority: 40,
    provides: &["recall"],
    requires: &["compress"],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait RecallService: Send + Sync {
    async fn recall(&self, session_id: &str, signal_topic: Option<&str>, journal_entries: &[&JournalEntry]) -> RecallAction;
}
```

#### Component 实现

```rust
impl Component for RecallComponent {
    fn name(&self) -> &str { "recall" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.12 JournalComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "journal",
    version: "0.1.0",
    priority: 40,
    provides: &["journal"],
    requires: &["compress"],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait JournalService: Send + Sync {
    fn add_entry(&mut self, entry: JournalEntry);
    fn search_summary(&self, query: &str) -> Vec<&JournalEntry>;
    fn get_loss_entries(&self) -> Vec<&JournalEntry>;
    fn update_loss_status(&mut self, round_id: usize, topic: Option<String>);
}
```

#### Component 实现

```rust
impl Component for JournalComponent {
    fn name(&self) -> &str { "journal" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §10 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

---

## 4. Orchestrator(协调器) 编排逻辑（§5.2）

### 4.1 压缩流程

```
CompressionService 主循环（500ms tick）
  │
  ├── 1. 收到 HookEvent::NewMessagesArrived
  │     └── 重置 compressor + PID（如果是新会话）
  │
  ├── 2. 收到 HookEvent::RoundComplete
  │     └── PID.record_round_interval() → 检测阶段变化
  │
  ├── 3. check_and_compress()
  │     ├── 冷启动阶段判断
  │     ├── token 数量检查（TokenCounter.count_messages）
  │     └── 超过阈值 → compress_round()
  │
  └── 4. compress_round()
        ├── PID 计算压缩量 delta（PidController.update）
        ├── Scorer 评分（Scorer.score_messages）
        ├── Anchor 计算压缩范围（Anchor.calculate_anchor_range）
        ├── UCB 分类决策（HierarchicalUCB.decide）
        ├── Fuzzy 边界决策（FuzzyController.decide）
        ├── EntityExtractor 排重（EntityExtractor.has_unique_entity）
        ├── Compressor 生成摘要（Compressor.compress_range）
        ├── CAS 写回共享仓库（SharedMessageStore.compare_and_write）
        ├── Feedback 记录（FeedbackEngine.record_round）
        └── Journal 记录（CompressionJournal.add_entry）
```

### 4.2 状态机

```
┌─────────┐     超过阈值     ┌─────────────┐
│  SLEEP  │ ──────────────→ │ COMPRESSING │
│         │ ←────────────── │             │
└─────────┘     完成压缩     └─────────────┘
```

### 4.3 冷启动阶段（§10 设计决策）

```
消息数 < collect_messages (50)  → Collect（仅紧急压缩）
消息数 < steady_messages (150)  → Conservative（阈值×1.2，目标×1.3）
消息数 ≥ steady_messages (150)  → Steady（正常压缩）
```

### 4.4 CAS 写回机制（§10 设计决策）

```
压缩结果写回 SharedMessageStore
  ├── 1. 读取当前版本号
  ├── 2. compare_and_write(expected_version, messages)
  │     ├── 成功 → 写入完成
  │     └── 失败 → 重试 3 次（每次重新读取版本号）
  └── 3. 3 次失败 → 放弃本次压缩结果，记录 CompressionCasConflict
```

---

## 5. 运行时信号（§3）

| 信号 | 处理方式 |
|------|---------|
| `GracefulShutdown` | 停止主循环 |
| `ImmediateShutdown` | 立即停止 |
| `ConfigReload` | 热更新配置（`reload_config()`） |
| `HealthCheck` | 检查运行状态 |
| `Suspend` | 暂停压缩 |
| `Resume` | 恢复压缩 |

**约束**：`handle_signal()` 不得阻塞超过 5 秒（§8 V-R02）。

---

## 6. 主循环

```rust
async fn run_loop(&mut self) {
    let mut tick_interval = tokio::time::interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            event = receiver.recv() => {
                match event {
                    Some(event) => {
                        let mut guard = self.inner.write().await;
                        guard.handle_hook_event(event, &handle).await;
                    }
                    None => break,
                }
            }
            _ = tick_interval.tick() => {
                let mut guard = self.inner.write().await;
                guard.run_state_machine(&handle).await;
            }
        }
    }
}
```

---

## 7. 跨平台与硬编码规范视角

### 7.1 硬编码值分类（§1，9 类逐条对照）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 不涉及 | ✅ |
| 2 | 模型名 | 涉及 | ⚠️ `compressor.rs:48` 系统提示词硬编码（§9.8 已记录修复计划） |
| 3 | 超时秒数 | 涉及 | ⚠️ `compressor.rs:169` temperature=0.3 硬编码（§9.8 已记录修复计划） |
| 4 | API 版本号 | 不涉及 | ✅ |
| 5 | User-Agent | 不涉及 | ✅ |
| 6 | 文件路径 | 涉及 | ✅ 所有路径通过 `dirs::home_dir()` + `join()` 构建 |
| 7 | 数字阈值 | 涉及 | ⚠️ `compressor.rs:170` max_tokens=1024 硬编码（§9.8 已记录修复计划） |
| 8 | 字符串模板 | 涉及 | ✅ 提示词、摘要模板为字符串模板类别（可接受） |
| 9 | 平台指令 | 不涉及 | ✅ |

### 7.2 跨平台路径规则（§2，8 条逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径 | ✅ |
| 2.2 | 禁止裸用 `~` | ✅ |
| 2.3 | 禁止相对路径依赖 CWD | ✅ |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ |
| 2.5 | 路径分隔符判断 | ✅ 不涉及 |
| 2.6 | 文件扩展名判断 | ✅ 使用 `.jsonl`、`.json` 跨平台扩展名 |
| 2.7 | 临时文件/目录 | ✅ 使用 `std::env::temp_dir()` |
| 2.8 | 数据目录 | ✅ 使用 `dirs::home_dir()` |

### 7.3 测试代码规范（§3，3 条逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 3.1 | 临时路径用 `std::env::temp_dir()` | ✅ |
| 3.2 | 平台特定测试用 `#[cfg()]` | ✅ 不涉及 |
| 3.3 | 网络测试用 mock 或 `#[ignore]` | ✅ LLM 调用使用 mock |

### 7.4 自查清单（§4，10 项逐项）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ |
| 2 | 模型名来自配置 | ⚠️ `compressor.rs:48` 硬编码（§9.8 已记录修复计划） |
| 3 | 超时值来自配置或常量 | ⚠️ temperature/max_tokens 硬编码（§9.8 已记录修复计划） |
| 4 | API 版本号为模块级 const | ✅ 不涉及 |
| 5 | User-Agent 为 const | ✅ 不涉及 |
| 6 | 路径用 `dirs` + `join()` | ✅ |
| 7 | 数字阈值从配置读取 | ⚠️ max_tokens 硬编码（§9.8 已记录修复计划） |
| 8 | 平台指令用 `OsKind` | ✅ 不涉及 |
| 9 | 测试无硬编码路径 | ✅ |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 8. 红线

### Service 集成协议红线（§8）

| 编号 | 红线 | 合规 |
|------|------|:----:|
| V-R01 | 必须响应 `HealthCheck` | ✅ |
| V-R02 | `handle_signal` 不得阻塞超过 5 秒 | ✅ |
| V-R03 | `provides` 与 `register_provider` 一致 | ✅ |

### 模块内部组件协议红线（§11）

| 编号 | 红线 | 合规 |
|------|------|:----:|
| C-R01 | `call()` 后必须 downcast | ✅ |
| C-R02 | `requires` 必须真实可验证 | ✅ |
| C-R03 | `process()` 必须可重入 | ✅ |

---

## 9. 设计决策（§10）

### 9.1 Service + Slot 双层架构

**决策**：Compression 同时包含 Service（后台压缩）和 Slot（Memorize 阶段钩子）。

**理由**：
1. **职责分离**：Slot 只负责通知（轻量），Service 负责执行（重量）
2. **异步解耦**：Slot 通过 mpsc channel 发送事件，不阻塞 Pipeline
3. **独立生命周期**：Service 可以独立于 Pipeline 运行

### 9.2 所有组件的 process() 为 no-op

**理由**：Service 的业务逻辑在主循环中，不在 `process()` 中。保留 `process()` 是为了未来扩展——如果需要定期维护任务，可以直接在 `process()` 中添加逻辑。

### 9.3 六算法协同架构

**决策**：使用 6 个独立算法组件协同工作。

**理由**：
1. **关注点分离**：每个算法只负责一个维度
2. **可替换性**：任何算法可以独立替换
3. **可测试性**：每个算法可以独立测试

### 9.4 CAS 写回机制

**决策**：压缩结果通过 CAS 写回 `SharedMessageStore`。

**理由**：压缩过程中可能有其他写入（如新消息到达）。CAS 保证不会覆盖并发写入。

### 9.5 冷启动策略

**决策**：冷启动分三个阶段（Collect → Conservative → Steady）。

**理由**：消息太少时不压缩，消息适中时保守压缩，消息充足时正常压缩。

### 9.6 Token 计数硬编码值

**决策**：Token 计数中的 CJK 乘数（1.5）、ASCII 乘数（0.25）、Image/Audio/File token 估算（85/100/50）硬编码为常量。

**理由**：这些是语言学特征和模型行为特征，不是业务配置。定义为模块级 `const` 集中管理。

### 9.7 mpsc channel 使用无界通道

**决策**：Service 和 Slot 之间的 HookEvent 通道使用 `tokio::sync::mpsc::unbounded_channel()`（无界通道）。

**理由**：
1. **HookEvent 体积小**：每条事件 < 1KB（仅包含 session_id、round_id、interval_ms）
2. **产生速率有限**：Pipeline 每次 Memorize 阶段触发一次，不会高频产生
3. **不阻塞 Pipeline**：无界通道保证 Slot 的 `send()` 永远不会阻塞，符合"不阻塞 Pipeline"的设计目标
4. **堆积可控**：即使短时堆积（压缩慢于 Pipeline），总内存占用可忽略，事件最终都会被处理

### 9.8 CompressorComponent LLM 参数硬编码

**决策**：`temperature = 0.3` 和 `max_tokens = 1024` 暂时硬编码在 CompressorComponent 内部。

**理由**：LLM 摘要参数由 CompressorComponent 内部决定，暂不暴露给用户。摘要生成是内部实现细节，用户不需要控制温度和 token 上限。如果未来需要用户可配置，迁移到 `CompressionConfig` 即可（v0.2 计划）。

**修复计划**：
- v0.1：保持硬编码，标注为技术债（§3.9、§7.4 已诚实标注）
- v0.2：迁移到 `CompressionConfig.summary_temperature` 和 `CompressionConfig.summary_max_tokens`

---

## 10. 新增/替换流程（§10）

### 新增算法组件

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 新建组件文件 | `components/my_algo.rs` |
| 2 | 实现 `Component` trait + 业务 trait | 同上 |
| 3 | 在 `orchestrator.rs` 注册 | `orch.register(Box::new(MyAlgo::new()), priority)` |
| 4 | 在 `components/mod.rs` 添加模块声明 | `pub mod my_algo;` |
| 5 | `cargo check` | — |

### 替换现有算法

| 步骤 | 做什么 |
|------|--------|
| 1 | 确认新旧 `meta().provides` 一致 |
| 2 | 确认新旧 `meta().requires` 是旧的子集 |
| 3 | 编写新 `impl Component`，替换原文件 |
| 4 | 若 `name` 不变，`orchestrator.rs` 无需修改 |
| 5 | `cargo check` + 单元测试 |
