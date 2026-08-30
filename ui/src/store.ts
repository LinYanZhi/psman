import { useSyncExternalStore } from "react";

import { emit, listen } from "@tauri-apps/api/event";

export type Theme = "dark" | "light";

export interface AppState {
  /** 当前选中的进程 PID（null = 未选中）；进程列表 / 详情面板 / 端口表联动 */
  selectedPid: number | null;
  /** 刷新计数：进程列表 / 端口表监听变化后重新拉取 */
  refreshKey: number;
  theme: Theme;
}

// 模块级单一数据源（主窗口与弹出窗口各自持有本地副本，经事件同步）
// 初始主题直接读 localStorage：窗口首帧即可用正确主题渲染（本地存储跨窗口共享）
let state: AppState = {
  selectedPid: null,
  refreshKey: 0,
  theme: localStorage.getItem("psman-theme") === "light" ? "light" : "dark",
};
const listeners = new Set<() => void>();

export function getAppState(): AppState {
  return state;
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export function useAppState(): AppState {
  return useSyncExternalStore(subscribe, getAppState, getAppState);
}

function setState(patch: Partial<AppState>) {
  state = { ...state, ...patch };
  for (const l of listeners) l();
}

let initialized = false;

/** 初始化跨窗口事件监听（每个窗口各自调用一次） */
export function initStore() {
  if (initialized) return;
  initialized = true;
  listen<number | null>("psman:selected", (e) => {
    setState({ selectedPid: e.payload });
  }).catch(() => {});
  listen("psman:refresh", () => {
    setState({ refreshKey: state.refreshKey + 1 });
  }).catch(() => {});
  listen<Theme>("psman:theme", (e) => {
    setState({ theme: e.payload });
  }).catch(() => {});
}

/** 选中进程（进程列表行点击 / 详情面板父子跳转），并广播给弹出窗口 */
export function selectPid(pid: number | null) {
  setState({ selectedPid: pid });
  emit("psman:selected", pid).catch(() => {});
}

/** 全局刷新：进程列表 / 端口表重新拉取，广播给所有窗口 */
export function refreshAll() {
  setState({ refreshKey: state.refreshKey + 1 });
  emit("psman:refresh", {}).catch(() => {});
}

/** 切换主题并广播 */
export function setTheme(theme: Theme) {
  setState({ theme });
  localStorage.setItem("psman-theme", theme);
  emit("psman:theme", theme).catch(() => {});
}
