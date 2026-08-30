import { useEffect, useRef } from "react";
import type * as React from "react";

import { DockviewDefaultTab, DockviewReact } from "dockview-react";
import { themeDark, themeLight } from "dockview";
import type {
  AddPanelOptions,
  DockviewApi,
  DockviewReadyEvent,
  IDockviewHeaderActionsProps,
  IDockviewPanelHeaderProps,
  IDockviewPanelProps,
} from "dockview";
import { listen } from "@tauri-apps/api/event";

// dockview 样式随懒加载 chunk 一起加载：弹出窗口路径不渲染 dockview，无需加载该 CSS
import "dockview/dist/styles/dockview.css";

import { createPopout } from "../api.ts";
import {
  PANEL_META,
  type PanelId,
  buildDefaultLayout,
  loadLayout,
  loadPopouts,
  saveLayout,
  savePopouts,
} from "../panels.ts";
import { useAppState } from "../store.ts";
import DetailPanel from "./DetailPanel.tsx";
import FilesPanel from "./FilesPanel.tsx";
import PerfPanel from "./PerfPanel.tsx";
import PortsPanel from "./PortsPanel.tsx";
import ProcessListPanel from "./ProcessListPanel.tsx";
import ServicesPanel from "./ServicesPanel.tsx";
import StartupPanel from "./StartupPanel.tsx";

// dockview 面板组件注册表（模块级稳定引用，避免重渲染导致面板重建）
const COMPONENTS: Record<string, React.FunctionComponent<IDockviewPanelProps>> = {
  process: ProcessListPanel,
  detail: DetailPanel,
  files: FilesPanel,
  ports: PortsPanel,
  services: ServicesPanel,
  startup: StartupPanel,
  perf: PerfPanel,
};

// 弹出逻辑桥接：DockLayout 实例把 popOutPanel 挂到这里，供组头操作按钮调用
let popOutHandler: ((id: PanelId) => void) | null = null;

// 隐藏默认关闭按钮：面板模块常驻布局，只能弹出 / 停靠，不允许直接关闭
function PanelTab(props: IDockviewPanelHeaderProps) {
  return <DockviewDefaultTab {...props} hideClose />;
}

// 组头部右侧操作：把当前激活面板弹出为独立窗口
function PanelHeaderActions(props: IDockviewHeaderActionsProps) {
  const panel = props.activePanel;
  const meta = panel ? PANEL_META[panel.id as PanelId] : undefined;
  if (!meta) return null;
  return (
    <button
      className="popout-btn"
      title="弹出为独立窗口"
      onClick={() => popOutHandler?.(meta.id)}
    >
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
        <path d="M15 3h6v6" />
        <path d="M10 14 21 3" />
        <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
      </svg>
    </button>
  );
}

export default function DockLayout({
  onApiReady,
}: {
  onApiReady?: (api: DockviewApi) => void;
}) {
  const apiRef = useRef<DockviewApi | null>(null);
  const saveTimer = useRef<number | null>(null);
  const { theme } = useAppState();

  // 弹出为独立窗口：先建窗口（失败则留在 dock），再从 dock 移除并记录注册表
  const popOutPanel = async (id: PanelId) => {
    const api = apiRef.current;
    if (!api) return;
    const panel = api.getPanel(id);
    if (!panel) return;
    const meta = PANEL_META[id];
    try {
      await createPopout(id, meta.windowTitle);
    } catch (e) {
      console.error("创建弹出窗口失败:", e);
      return;
    }
    const popouts = loadPopouts();
    if (!popouts.includes(id)) savePopouts([...popouts, id]);
    panel.api.close();
  };
  popOutHandler = popOutPanel;

  // 钉回主窗口：把面板放回 dock（默认以进程列表为锚），并从注册表移除
  const dockBackPanel = (id: PanelId) => {
    const api = apiRef.current;
    if (!api) return;
    if (api.getPanel(id)) return;
    const meta = PANEL_META[id];
    const opts: AddPanelOptions = {
      id: meta.id,
      component: meta.component,
      title: meta.title,
    };
    if (id === "detail") {
      opts.position = { direction: "right", referencePanel: "process" };
    } else if (id !== "process") {
      opts.position = { direction: "below", referencePanel: "process" };
    }
    api.addPanel(opts);
    savePopouts(loadPopouts().filter((p) => p !== id));
  };

  const restorePopouts = () => {
    for (const id of loadPopouts()) {
      const meta = PANEL_META[id];
      createPopout(id, meta.windowTitle).catch((e) =>
        console.error("恢复弹出窗口失败:", e),
      );
    }
  };

  const handleReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    apiRef.current = api;
    onApiReady?.(api);

    // 布局变更（停靠/拖动/弹出/钉回）→ 防抖保存
    api.onDidLayoutChange(() => {
      if (saveTimer.current != null) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        if (apiRef.current) saveLayout(JSON.stringify(apiRef.current.toJSON()));
      }, 600);
    });

    const saved = loadLayout();
    if (saved) {
      try {
        api.fromJSON(JSON.parse(saved));
        // 布局损坏或全空（全部面板都被弹出）时兜底
        if (api.panels.length === 0) {
          api.addPanel({
            id: "process",
            component: PANEL_META.process.component,
            title: PANEL_META.process.title,
          });
        }
      } catch {
        buildDefaultLayout(api);
      }
    } else {
      buildDefaultLayout(api);
    }
    restorePopouts();
  };

  // 弹出窗口关闭/钉回事件（由后端 dock_back 命令或 on_window_event 统一转发）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<PanelId>("psman:popout-closed", (e) => {
      dockBackPanel(e.payload);
    }).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
  }, []);

  return (
    <DockviewReact
      className="dock-layout"
      components={COMPONENTS}
      defaultTabComponent={PanelTab}
      rightHeaderActionsComponent={PanelHeaderActions}
      theme={theme === "light" ? themeLight : themeDark}
      onReady={handleReady}
    />
  );
}
