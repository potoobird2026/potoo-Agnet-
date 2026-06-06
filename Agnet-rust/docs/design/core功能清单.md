# Core 功能清单

> 更新日期：2026-05-28。业务类型已全部迁出 core。

按文件逐一列出每个文件的所有功能。

---

## `core/types/mod.rs`

**基础设施类型——仅 3 个，消除外部依赖。**

1. **Timestamp** — 时间戳。毫秒级 Unix 时间戳，替代 chrono。
2. **Version** — 版本号。语义化版本 "1.0.0"，替代 semver。
3. **CancellationToken** — 取消开关。基于 AtomicBool，替代 tokio-util。

> **已迁出**：`SharedMessageStore` → `core/runtime.rs`（它是运行时组件）。
> **已迁出**：`Message`/`ContentBlock`/`ToolCall`/`MessageRole` → `shared_types/message.rs`。
> **已迁出**：`Thought`/`Action`/`Observation`/`ActionResult`/`Turn` → `plugins/slots/llm_thinker/types.rs`。
> **已迁出**：`StepResponse` → `shared_types/step_response.rs`。
> **已迁出**：`ToolDefinition` → `plugins/slots/tool_registry/types.rs`。

---

## `core/types/error.rs`

**错误类型定义。**

1. **PluginError** — 插件统一错误类型。10 个变体（InitFailed/Runtime/Config/PermissionDenied/NotFound/Timeout/Shutdown/DuplicateName/DependencyNotFound/Internal）。
2. **AgentError** — Agent 顶层错误。5 个变体（PluginFailed/PipelineAborted/SessionError/RuntimeShuttingDown/Internal）。

---

## `core/types/plugin.rs`

**插件相关数据类型。**

1. **PluginInitContext** — 插件初始化时收到的信息包。
2. **AgentConfig** — Agent 全局配置。由 `CoreConfig::to_agent_config()` 从 TOML 转换而来。
3. **PluginMetadata** — 插件元数据声明。含 run_mode / config_schema（新增字段）。
4. **RunMode** — 运行模式。Background / OnDemand / Cron。

---

## `core/types/persistence.rs`

**持久化通信命令定义。**

1. **PersistenceCommand** — 持久化命令。
2. **PersistenceAck** — 持久化回复。

---

## `core/component.rs`

**模块内部组件协议。**

1. **Component** trait — 模块内部处理单元统一接口（init→process→shutdown）。
2. **Processing** enum — 处理结果（Continue/BreakChain/Restart/Warn）。
3. **InternalAccessPoint** trait — 模块内部组件之间的受控通道。
4. **ComponentHandle** trait — 跨组件调用句柄（通过 downcast 获取具体类型）。
5. **ComponentError** — 组件错误类型。
6. **ComponentMeta** — 组件元数据声明。

> **注意**：原 `core/types/data_contract.rs`（ComponentDescriptor/DescriptorKind 等）已迁至 `infra/metadata/descriptor.rs`。

---

## `core/slot.rs`

**槽口插件标准接口（SlotPlugin）和执行指令（SlotDirective）。**

1. **SlotDirective** — 执行指令。Continue/BreakPhase/BreakStep/RestartStep/AbortStep/AbortPipeline/JumpTo。
2. **SlotPlugin** — 槽口插件的标准接口。name/init/run/shutdown。

---

## `core/service.rs`

**服务插件标准接口（ServicePlugin）和信号（ServiceSignal）。**

1. **ServicePlugin** — 服务插件的标准接口。name/init/start/handle_signal/stop/shutdown。
2. **ServiceSignal** — 服务信号。GracefulShutdown/ImmediateShutdown/ConfigReload/HealthCheck/Suspend/Resume。

---

## `core/phase.rs`

**阶段标识符。**

1. **Phase** — 透明的阶段标识。核心不做语义假设。
2. 7 个预设阶段：Init、Context、Think、Audit、Execute、Loop、Memorize。

---

## `core/pipeline.rs`

**Pipeline 执行引擎。**

1. **建阶段列表** — add_phase / insert_phase_before / insert_phase_after / remove_phase。
2. **注册槽口** — add_slot / register。
3. **跑流程** — run()，按阶段顺序执行 Slot，处理 SlotDirective。
4. **后向跳转保护** — max_backward_jumps 计数器（默认 10 次）防死循环。
5. **验证完整性** — validate()，检查阶段非空、每阶段至少一个 Slot。

---

## `core/context.rs`

**执行上下文——slot 执行期间读写数据的通道。**

1. **StepContext** — 执行上下文。装着消息列表、当前轮次、当前阶段、通用上下文数据。
2. **存取上下文数据** — set_context/get_context（按 String key 索引，替代旧 set_data/get_data）。
3. **查 Provider** — provider_raw() 查找 Service 注册的业务能力。
4. **跳转/中止请求** — request_jump / request_abort，经权限校验。
5. **AgentHandle** — 外部通过它给 runtime 发消息。
6. **StepInput** — 一步输入（session_id + 消息内容 + 响应通道）。

---

## `core/access/mod.rs`

**通信接口——Provider 注册表 + Slot/Service 接入通道。**

1. **ProviderRegistry** — 能力注册表。Service 注册能力，Slot 查找能力。支持 register / get / get_raw / unregister / has / list。
2. **SlotAccessPoint** — 槽口接入点。Slot 通过它读写上下文（write_context_raw/read_context_raw）、写观察结果（write_observation，类型擦除）、查能力（provider_raw）、请求跳转/中止（经权限校验）。
3. **ServiceAccessPoint** — 服务接入点。Service 通过它查配置（get_config）、注册/反注册能力（register_provider / unregister_provider）、写日志。

---

## `core/runtime.rs`

**AgentRuntime——主循环、会话管理、SharedMessageStore。**

1. **SharedMessageStore** — 共享消息仓库。CAS 写入防丢失，压缩服务与运行时之间的消息权威来源。
2. **收消息** — 监听 mpsc 通道，接收外部 StepInput。
3. **取历史** — 从 SharedMessageStore 读出旧对话。
4. **拼消息** — 用户新消息 + 历史记录拼一起。
5. **跑流程** — 交给 Pipeline 执行所有阶段。
6. **存消息** — 写回 SharedMessageStore（含版本号递增）。
7. **通知存盘** — 可选持久化通道异步落盘。
8. **上下文裁剪** — 超 token 限制时保留 System + 最近 10 条，裁剪最早的非 System 消息。
9. **限制会话数量** — max_sessions 上限。
10. **配置注入** — new_with_config(pipeline, config) 由 main.rs 从 TOML 注入，非硬编码。

---

## `src/lib.rs`

**crate 根入口——声明所有顶层模块。**

1. **声明模块** — core / shared_types / plugins / infra 四大模块。
2. 不包含业务逻辑，仅做模块声明。
