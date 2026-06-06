# infra/config 设计文档

## 一句话说清它是干嘛的

**开机时读 config.toml 文件，把里面的设置分给核心和各插件。**

---

## 它和 AgentConfig 是什么关系？

```
硬盘上的文件                     内存里的结构体                谁在用？
─────────────────              ──────────────────           ─────────
config.toml  ──读文件──→  AgentConfig {                   核心自己用
  [core]                         agent_id: "xxx",           (runtime.rs)
  agent_id = "xxx"               workspace: "xxx",
  log_level = "info"             log_level: "info",
                                 data_dir: "xxx",
                               }
                                 
                               PluginInitContext {         每个插件 init() 时
                                  plugin_name: "llm",       收到自己的配置段
                                  plugin_config: {          只读不改
                                     api_key: "xxx"
                                  },
                                  agent_config: ...,        (上面那个 AgentConfig)
                               }
```

---

## 现在需要什么功能？（v0.x 阶段）

**四个操作，每个都极简单，不加任何多余功能。**

### 1. `new(path)` — 启动时读文件

```
用户给了路径 → 读文件 → 解析 TOML → 存内存
用户没给路径 → 用默认值（不报错，不自动创建文件）
读文件失败   → 用默认值 + 打一行警告日志
```

### 2. `current()` — 查看当前配置

直接返回内存里那份 `AgentConfig`，不做任何额外操作。

### 3. `get_section(name)` — 查看某个插件的配置

从内存里找到 `[plugins.xxx]` 那段，原样返回。插件在 `init()` 时调用一次。

### 4. `update_config(new)` — 改配置并写回硬盘

```
收到新配置 → 写回 config.toml → 更新内存
```

---

## 未来可能加什么？（暂时不做，只列在这里）

| 功能 | 什么时候可能需要 | 加在哪里 |
|------|----------------|---------|
| 文件监听热更新 | 用户手动改了 config.toml，程序自动重读 | `new()` 里加一个文件 watch |
| 配置校验 | 用户写错了配置项，想报错提示 | `update_config()` 里加 validator |
| 事件通知 | 配置改了，想通知所有插件重新读 | `update_config()` 里加广播 |
| 多个配置文件合并 | 支持 include 其他文件 | `new()` 里加递归读取 |

现在**不需要**以上任何一个。

---

## 代码结构

```
src/infra/
  └── config/
        ├── mod.rs       # 暴露 ConfigLoader
        ├── types.rs     # AagnetConfig + CoreConfig 定义（含 From<CoreConfig> for AgentConfig）
        └── loader.rs    # ConfigLoader 实现（~50 行）
```

> **注意**：`CoreConfig` 通过 `to_agent_config()` 或 `From` trait 转换为 `core::types::plugin::AgentConfig`。`AgentRuntime` 使用 `new_with_config(pipeline, config)` 接收注入的配置，不再硬编码默认值。

---

## 关键设计原则

1. **只读文件，不写死** — 所有配置从文件读，不允许硬编码默认值取代配置文件
2. **失败不崩溃** — 文件不存在或格式错误，用内置默认值继续运行，只打警告
3. **不存插件私有状态** — 只存全局配置，插件自己的运行时状态（如计数器、连接池）各自管理
4. **不主动通知任何人** — 只是被调用时返回数据，不监听、不广播、不推送
5. **未来扩展不加 break change** — `update_config()` 是唯一的写入入口，未来加校验/通知都在这个函数里扩
