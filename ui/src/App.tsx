import { lazy, Suspense, useEffect, useRef, useState } from "react";
import type * as React from "react";

import { invoke } from "@tauri-apps/api/core";
import type { DockviewApi } from "dockview";

import { relaunchAsAdmin, systemOverview } from "./api.ts";
import type { SystemOverview } from "./api.ts";
import DetailPanel from "./components/DetailPanel.tsx";
import FilesPanel from "./components/FilesPanel.tsx";
import PerfPanel from "./components/PerfPanel.tsx";
import PortsPanel from "./components/PortsPanel.tsx";
import ProcessListPanel from "./components/ProcessListPanel.tsx";
import ServicesPanel from "./components/ServicesPanel.tsx";
import StartupPanel from "./components/StartupPanel.tsx";
import { buildDefaultLayout, clearLayout, type PanelId, PANEL_META, savePopouts } from "./panels.ts";
import { initStore, refreshAll, setTheme, useAppState } from "./store.ts";
import { formatSize } from "./utils.ts";

// DockLayout 懒加载：dockview 拆到独立 chunk。
// 主窗口首屏不加载 dockview；弹出窗口路径完全不加载它（弹出窗口启动更快的关键）。
const DockLayout = lazy(() => import("./components/DockLayout.tsx"));

// 渲染前同步已保存主题，避免首帧闪烁（localStorage 跨窗口共享，弹出窗口同样生效）
const savedTheme = localStorage.getItem("psman-theme");
if (savedTheme === "light") {
  document.documentElement.dataset.theme = "light";
}

function ThemeIcon({ theme }: { theme: "dark" | "light" }) {
  if (theme === "dark") {
    // 当前深色 → 月亮图标
    return (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
      </svg>
    );
  }
  // 当前浅色 → 太阳图标
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </svg>
  );
}

// 主题同步到 DOM（主窗口与弹出窗口共用）
function useThemeEffect() {
  const { theme } = useAppState();
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
}

/** 顶栏系统总览（挂载查一次 + 5s 轮询，后台线程采样开销可忽略） */
function SystemOverview() {
  const [overview, setOverview] = useState<SystemOverview | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const o = await systemOverview();
        if (!cancelled) setOverview(o);
      } catch {
        /* 轮询失败静默，下次重试 */
      }
    };
    tick();
    const t = setInterval(tick, 5000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, []);

  if (!overview) return null;
  return (
    <div className="sys-overview">
      <span className="sys-item">
        CPU <b>{overview.cpu.toFixed(1)}%</b>
      </span>
      <span className="sys-item">
        内存 <b>{formatSize(overview.mem_used)}</b>
        <span className="dim"> / {formatSize(overview.mem_total)}</span>
      </span>
      <span className="sys-item">
        磁盘 <b>{formatSize(overview.disk_total - overview.disk_free)}</b>
        <span className="dim"> / {formatSize(overview.disk_total)}</span>
      </span>
    </div>
  );
}

// ── 主窗口：顶栏 + dock 布局 ─────────────────────────

function MainApp() {
  const { theme } = useAppState();
  const dockApi = useRef<DockviewApi | null>(null);
  useThemeEffect();

  // 重置布局：恢复默认停靠，清空持久化与弹出注册表
  const resetLayout = () => {
    if (!dockApi.current) return;
    clearLayout();
    savePopouts([]);
    const api = dockApi.current;
    api.clear();
    buildDefaultLayout(api);
  };

  return (
    <div className="app">
      <header className="app-topbar">
        <div className="app-title">psman</div>
        <SystemOverview />
        <div className="app-actions">
          <button
            className="btn ghost icon"
            onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
          >
            <ThemeIcon theme={theme} />
          </button>
          <button className="btn" onClick={() => refreshAll()}>
            刷新
          </button>
          <button className="btn ghost" onClick={resetLayout} title="恢复默认面板布局">
            重置布局
          </button>
          <button
            className="btn ghost"
            onClick={() =>
              relaunchAsAdmin().catch((e) => alert(`提权重启失败：${e}`))
            }
          >
            以管理员重启
          </button>
        </div>
      </header>
      <main className="app-content">
        <Suspense fallback={<div className="dock-loading">正在加载布局…</div>}>
          <DockLayout
            onApiReady={(api) => {
              dockApi.current = api;
            }}
          />
        </Suspense>
      </main>
    </div>
  );
}

// ── 弹出窗口：单面板 + 钉回 ─────────────────────────

const POPOUT_COMPONENTS: Record<PanelId, () => React.JSX.Element> = {
  process: () => <ProcessListPanel />,
  detail: () => <DetailPanel />,
  files: () => <FilesPanel />,
  ports: () => <PortsPanel />,
  services: () => <ServicesPanel />,
  startup: () => <StartupPanel />,
  perf: () => <PerfPanel />,
};

function PopoutApp({ panelId }: { panelId: PanelId }) {
  const { theme } = useAppState();
  useThemeEffect();
  const meta = PANEL_META[panelId];

  // 窗口 X 关闭由后端 on_window_event 统一处理（强制钉回 + 销毁），前端无需拦截

  return (
    <div className="app popout-app">
      <header className="app-topbar">
        <div className="app-title">{meta.title}</div>
        <div className="app-actions">
          <button className="btn" onClick={() => refreshAll()}>
            刷新
          </button>
          <button
            className="btn ghost icon"
            onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
          >
            <ThemeIcon theme={theme} />
          </button>
          <button
            className="btn"
            title="把面板放回主窗口停靠"
            onClick={() => invoke("dock_back", { label: panelId }).catch(() => {})}
          >
            钉回主窗口
          </button>
        </div>
      </header>
      <main className="app-content">{POPOUT_COMPONENTS[panelId]()}</main>
    </div>
  );
}

export default function App() {
  // 独立窗口（index.html?popout=<panelId>）与主窗口共用入口
  const params = new URLSearchParams(window.location.search);
  const popout = params.get("popout") as PanelId | null;

  useEffect(() => {
    initStore();
  }, []);

  if (popout && popout in PANEL_META) {
    return <PopoutApp panelId={popout} />;
  }
  return <MainApp />;
}
