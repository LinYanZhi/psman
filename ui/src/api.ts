import { invoke } from "@tauri-apps/api/core";

export interface ProcInfo {
  pid: number;
  name: string;
  mem: number;
  threads: number;
  status: string;
  start: number;
  ports: number[];
  parent: number;
  exe: string;
  /** CPU 使用率（%）——后端复用全局 System 连续采样，首次加载为 0，之后为相邻采样均值 */
  cpu: number;
}

export interface ConnInfo {
  port: number;
  proto: string;
  state: string;
  /** 远程地址 "ip:port"（监听为空串） */
  remote: string;
}

export interface ProcThread {
  tid: number;
  base_prio: number;
  delta_prio: number;
}

export interface ProcDetail {
  pid: number;
  name: string;
  path: string;
  cmd: string;
  mem: number;
  threads: number;
  status: string;
  start: number;
  ports: string[];
  parent: number;
  children: number[];
  conns: ConnInfo[];
  thread_list: ProcThread[];
  /** 句柄总数 */
  handles: number;
  /** 优先级类常量（GetPriorityClass） */
  priority: number;
}

export interface FileHolder {
  pid: number;
  name: string;
  app_type: number;
  status: number;
  restartable: boolean;
}

export interface OpenFile {
  path: string;
  count: number;
}

export interface KillResult {
  pid: number;
  ok: boolean;
  name: string;
  error: string | null;
}

export interface SystemOverview {
  cpu: number;
  mem_total: number;
  mem_used: number;
  disk_total: number;
  disk_free: number;
}

export interface PortInfo {
  port: number;
  proto: string;
  state: string;
  pid: number;
  name: string;
  /** 远程地址 "ip:port"（监听/UDP 为空串） */
  remote: string;
}

export interface ServiceInfo {
  name: string;
  display: string;
  state: string;
  state_code: number;
  start_type: string;
  pid: number;
  path: string;
}

export interface StartupItem {
  name: string;
  command: string;
  source: string;
  kind: "reg" | "folder";
  enabled: boolean;
  path: string;
}

export const listProcesses = () => invoke<ProcInfo[]>("list_processes");
export const getProcessDetail = (pid: number) =>
  invoke<ProcDetail | null>("get_process_detail", { pid });
export const fileHolders = (path: string) =>
  invoke<FileHolder[]>("file_holders", { path });
export const processOpenFiles = (pid: number) =>
  invoke<OpenFile[]>("process_open_files", { pid });
export const killProcesses = (pids: number[]) =>
  invoke<KillResult[]>("kill_processes", { pids });
export const killTreeProcess = (pid: number) =>
  invoke<number[]>("kill_tree_process", { pid });
export const relaunchAsAdmin = () => invoke<void>("relaunch_as_admin");
export const systemOverview = () => invoke<SystemOverview>("system_overview");
export const queryPorts = () => invoke<PortInfo[]>("query_ports");
export const createPopout = (label: string, title: string) =>
  invoke<void>("create_popout", { label, title });
export const processIcon = (path: string) => invoke<string | null>("process_icon", { path });
export const revealInFolder = (path: string) => invoke<void>("reveal_in_folder", { path });
export const autostartEnabled = () => invoke<boolean>("autostart_enabled");
export const autostartSet = (enabled: boolean) => invoke<void>("autostart_set", { enabled });
export const listServices = () => invoke<ServiceInfo[]>("list_services");
export const serviceAction = (name: string, action: "start" | "stop" | "restart") =>
  invoke<void>("service_action", { name, action });
export const listStartupItems = () => invoke<StartupItem[]>("list_startup_items");
export const startupSetEnabled = (source: string, name: string, enabled: boolean) =>
  invoke<void>("startup_set_enabled", { source, name, enabled });
export const processPriority = (pid: number, classValue: number) =>
  invoke<void>("process_priority", { pid, class: classValue });
export const processSuspend = (pid: number) => invoke<void>("process_suspend", { pid });
export const processResume = (pid: number) => invoke<void>("process_resume", { pid });
