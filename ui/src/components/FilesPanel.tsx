import { useState } from "react";

import { fileHolders, killProcesses } from "../api.ts";
import type { FileHolder } from "../api.ts";
import ConfirmDialog from "./ConfirmDialog.tsx";

export default function FilesPanel() {
  const [path, setPath] = useState("");
  const [holders, setHolders] = useState<FileHolder[] | null>(null);
  const [queriedPath, setQueriedPath] = useState("");
  const [loading, setLoading] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 结束占用进程前二次确认（替代 window.confirm）
  const [confirm, setConfirm] = useState<{
    title: string;
    message: string;
    onOk: () => void;
  } | null>(null);

  const query = async (p?: string) => {
    const target = (p ?? path).trim();
    if (!target) return;
    setLoading(true);
    setNotice(null);
    try {
      const res = await fileHolders(target);
      setHolders(res);
      setQueriedPath(target);
    } catch (e) {
      setHolders(null);
      setQueriedPath("");
      setNotice(String(e));
    }
    setLoading(false);
  };

  const doKill = async (pids: number[]) => {
    setBusy(true);
    try {
      const res = await killProcesses(pids);
      const failed = res.filter((r) => !r.ok);
      setNotice(
        failed.length === 0
          ? `已结束 ${res.length} 个占用进程，请重试操作文件`
          : `部分失败（${failed.length} 个）：${failed
              .map((r) => `${r.name}(${r.pid})${r.error ? " " + r.error : ""}`)
              .join("；")}`,
      );
      await query(queriedPath);
    } catch (e) {
      setNotice(String(e));
    }
    setBusy(false);
  };

  // 先弹应用内确认框，确认后再执行（替代 window.confirm）
  const kill = (pids: number[], label: string) => {
    setConfirm({
      title: "结束占用进程",
      message: `确定结束${label}（${pids.length} 个进程）以解锁文件？`,
      onOk: () => {
        setConfirm(null);
        doKill(pids);
      },
    });
  };

  const appTypeText = (t: number) => {
    switch (t) {
      case 0:
        return "未知";
      case 1:
        return "主窗口";
      case 2:
        return "其他窗口";
      case 3:
        return "服务";
      case 4:
        return "资源管理器";
      case 5:
        return "控制台";
      case 1000:
        return "系统关键";
      default:
        return `类型${t}`;
    }
  };

  const statusText = (s: number) => {
    switch (s) {
      case 1:
        return "运行中";
      case 2:
        return "已停止";
      case 8:
        return "已重启";
      case 16:
        return "停止出错";
      default:
        return `状态${s}`;
    }
  };

  return (
    <div className="view">
      <div className="view-toolbar">
        <input
          className="search grow"
          placeholder="输入文件或目录路径，如 C:\Users\test\file.txt"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && query()}
        />
        <button className="btn" disabled={loading} onClick={() => query()}>
          {loading ? "查询中…" : "查询占用"}
        </button>
      </div>

      {notice && <div className="notice block">{notice}</div>}

      {holders !== null && (
        <div className="holders">
          <div className="holders-title">
            路径 <span className="mono">{queriedPath}</span>
          </div>
          {holders.length === 0 ? (
            <div className="empty-box">✓ 没有进程占用该文件</div>
          ) : (
            <>
              <div className="holders-list">
                {holders.map((h, i) => (
                  <div className="holder" key={`${h.pid}-${i}`}>
                    <div className="holder-main">
                      <span className="holder-name">{h.name || "未知进程"}</span>
                      <span className="dim">PID {h.pid}</span>
                      <span className="badge">{appTypeText(h.app_type)}</span>
                      <span className={"badge " + (h.status === 1 ? "ok" : "")}>
                        {statusText(h.status)}
                      </span>
                      {!h.restartable && <span className="badge warn">不可重启</span>}
                    </div>
                    <button
                      className="btn danger small"
                      disabled={busy}
                      onClick={() => kill([h.pid], h.name || `PID ${h.pid}`)}
                    >
                      结束
                    </button>
                  </div>
                ))}
              </div>
              <button
                className="btn danger"
                disabled={busy}
                onClick={() =>
                  kill(
                    holders.map((h) => h.pid),
                    "全部占用进程",
                  )
                }
              >
                结束全部占用进程
              </button>
            </>
          )}
        </div>
      )}

      {confirm && (
        <ConfirmDialog
          title={confirm.title}
          message={confirm.message}
          confirmText="结束"
          danger
          busy={busy}
          onConfirm={confirm.onOk}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}
