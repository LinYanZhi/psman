# psman — 项目维护文档

> **本文件目录** — 项目快照式维护文档，供 AI 接手快速了解全貌。
> **最后更新**: 2026-10-01
> **更新者**: DSH session-61b7be30（workspace-project-audit 工作流）
> **过期阈值**: 30 天重检；超过 60 天 AI 不得以此为决策依据，必须 `workspace-health.mjs` 实测。
> **来源**: `README.md` + `git log` + `Cargo.toml` + 实测。

## 0. 速读（30 秒接手）

| 项 | 值 |
|---|---|
| 项目类型 | Windows 进程/端口/线程管理工具（CLI + Tauri 2 GUI） |
| 技术栈 | Rust + sysinfo + Windows 原生 API（IP Helper / Restart Manager / ToolHelp）+ Tauri 2 + React 19 + dockview + Vite（**仅 Windows**） |
| 主入口 | `src/main.rs`（psman CLI，单次执行 + REPL + 系统托盘常驻，单例 + 关窗保活） |
| 当前状态 | ⚙️ 30 天维护 |
| 最近 commit | 2026-09-03 |
| 接手难度 | 4/5 |
| AGENTS.md | ✗ |
| README.md | ✓ |

## 1. 这是什么

Windows 进程 / 端口 / 线程管理工具。CLI（`psman`）+ Tauri 2 GUI（`psman-gui`）双形态：

- **`src/main.rs`**：根 Cargo.toml 构建 psman CLI
- **`src-tauri/`**：暴露 `psman-gui`（Tauri 2 桌面壳，`crate-type` 含 lib/cdylib/staticlib）
- **数据层共享**：`src-tauri` 通过 `psman = { path = ".." }` 复用根 crate 的 `lib.rs` 数据层
- **六大模块**：proc / net / file / handles / services / startup

## 3. 关键命令

```bash
# 前端 / GUI（ui/）
npm --prefix ui run dev          # 前端 dev
npm --prefix ui run build        # 前端 build
npm --prefix ui run typecheck    # TS 校验
npm --prefix ui run preview      # 前端 preview

# Tauri dev（走 app-kit 治本脚本）
node ../../../app-kit/scripts/tauri-dev.mjs

# CLI（仓根）
cargo build --release           # CLI release 构建

# 测试：无
```

## 4. 最近 5 条 commit

- `f62c1e6` chore: 提交 pnpm-lock.yaml，忽略 ui/node_modules（2026-09-03）
- `cd61f5e` feat: 初始提交 psman - 进程/端口/线程管理 CLI 工具（2026-08-30）

**注**：仓库仅 2 条 commit。

## 5. 关键说明 / 坑

- **仅 Windows 平台**：依赖 sysinfo 0.39 + windows 0.58 全套 Win32 feature：
  - IP Helper 取端口表
  - Restart Manager 取文件占用
  - NtQuerySystemInformation / NtQueryObject 取进程句柄
  - sysinfo 取进程枚举
- **无 CSS 框架**：所有 UI 仅 React 19 + dockview 多面板。
- **共享 lib.rs 数据层**：CLI 与 GUI 都依赖根 crate 的 lib.rs（proc/net/file/handles/services/startup 六大模块）。

## 6. 关联项目

- **`GLBT-gz/app-kit`**：通过 `tauri-dev.mjs` 治本脚本集成 Tauri dev 流程。

## 7. 状态摘要

| 指标 | 值 |
|---|---|
| 7d commits | 0 |
| 30d commits | 1 |
| dirty | 0 |
| ahead/behind | 0 / 0 |

---

🤖 *本文档由 workspace-project-audit 工作流生成（DSH workflow，2026-10-01）。如需重检，跑 `node my-skills/scripts/workspace-health.mjs active` 看实际状态。*