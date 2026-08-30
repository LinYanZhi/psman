import type { DockviewApi } from "dockview";

export type PanelId = "process" | "detail" | "files" | "ports" | "services" | "startup" | "perf";

export interface PanelMeta {
  id: PanelId;
  /** dock 标签标题 */
  title: string;
  /** 弹出窗口标题 */
  windowTitle: string;
  /** dockview 注册的组件名（与 DockLayout components 键对应） */
  component: string;
}

export const PANEL_META: Record<PanelId, PanelMeta> = {
  process: { id: "process", title: "进程列表", windowTitle: "psman — 进程列表", component: "process" },
  detail: { id: "detail", title: "进程详情", windowTitle: "psman — 进程详情", component: "detail" },
  files: { id: "files", title: "文件占用", windowTitle: "psman — 文件占用", component: "files" },
  ports: { id: "ports", title: "端口表", windowTitle: "psman — 端口表", component: "ports" },
  services: { id: "services", title: "服务", windowTitle: "psman — 服务", component: "services" },
  startup: { id: "startup", title: "启动项", windowTitle: "psman — 启动项", component: "startup" },
  perf: { id: "perf", title: "性能趋势", windowTitle: "psman — 性能趋势", component: "perf" },
};

export const PANEL_IDS: PanelId[] = ["process", "detail", "files", "ports", "services", "startup", "perf"];

// ── 布局持久化（localStorage） ────────────────────────

const LAYOUT_KEY = "psman-layout-v1";
const POPOUT_KEY = "psman-popouts-v1";

export function loadLayout(): string | null {
  return localStorage.getItem(LAYOUT_KEY);
}

export function saveLayout(json: string) {
  localStorage.setItem(LAYOUT_KEY, json);
}

export function clearLayout() {
  localStorage.removeItem(LAYOUT_KEY);
}

/** 弹出面板注册表：记录哪些面板当前处于独立窗口状态（重启后据此重建窗口） */
export function loadPopouts(): PanelId[] {
  try {
    const v = JSON.parse(localStorage.getItem(POPOUT_KEY) ?? "[]") as PanelId[];
    return v.filter((p): p is PanelId => p in PANEL_META);
  } catch {
    return [];
  }
}

export function savePopouts(popouts: PanelId[]) {
  localStorage.setItem(POPOUT_KEY, JSON.stringify(popouts));
}

// ── 默认布局（PyCharm 式：中央进程列表 + 右侧详情 + 底部端口/文件标签页） ──

export function buildDefaultLayout(api: DockviewApi) {
  api.addPanel({
    id: "process",
    component: PANEL_META.process.component,
    title: PANEL_META.process.title,
  });
  api.addPanel({
    id: "detail",
    component: PANEL_META.detail.component,
    title: PANEL_META.detail.title,
    position: { direction: "right", referencePanel: "process" },
    initialWidth: 360,
  });
  api.addPanel({
    id: "ports",
    component: PANEL_META.ports.component,
    title: PANEL_META.ports.title,
    position: { direction: "below", referencePanel: "process" },
    initialHeight: 220,
  });
  api.addPanel({
    id: "files",
    component: PANEL_META.files.component,
    title: PANEL_META.files.title,
    position: { direction: "within", referencePanel: "ports" },
  });
  api.addPanel({
    id: "services",
    component: PANEL_META.services.component,
    title: PANEL_META.services.title,
    position: { direction: "within", referencePanel: "files" },
  });
  api.addPanel({
    id: "startup",
    component: PANEL_META.startup.component,
    title: PANEL_META.startup.title,
    position: { direction: "within", referencePanel: "services" },
  });
  api.addPanel({
    id: "perf",
    component: PANEL_META.perf.component,
    title: PANEL_META.perf.title,
    position: { direction: "within", referencePanel: "startup" },
  });
}
