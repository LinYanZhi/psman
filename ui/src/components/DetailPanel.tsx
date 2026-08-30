import { useEffect, useMemo, useRef, useState } from "react";

import {
  getProcessDetail,
  killProcesses,
  killTreeProcess,
  processOpenFiles,
  processPriority,
  processResume,
  processSuspend,
} from "../api.ts";
import type { OpenFile, ProcDetail } from "../api.ts";
import { formatSize, fmtTime } from "../utils.ts";
import { refreshAll, selectPid, useAppState } from "../store.ts";
import ConfirmDialog from "./ConfirmDialog.tsx";

// 优先级类（SetPriorityClass 常量值 → 中文）
const PRIO_OPTS: { v: number; label: string }[] = [
  { v: 64, label: "空闲" },
  { v: 16384, label: "低于正常" },
  { v: 32, label: "正常" },
  { v: 32768, label: "高于正常" },
  { v: 128, label: "高" },
  { v: 256, label: "实时" },
];
const prioLabel = (v: number) =>
  PRIO_OPTS.find((o) => o.v === v)?.label ?? `未知(${v})`;

// 进程详情面板：展示当前选中进程（store.selectedPid）的完整信息。
// 独立窗口 / dock 面板共用同一组件，状态经 store 跨窗口同步。
export default function DetailPanel() {
  const { selectedPid, refreshKey } = useAppState();
  const [detail, setDetail] = useState<ProcDetail | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 暂停/恢复 与优先级调整共用操作锁（操作后刷新详情）
  const [opBusy, setOpBusy] = useState(false);
  // 结束进程前二次确认（替代 window.confirm）
  const [confirm, setConfirm] = useState<{
    title: string;
    message: string;
    onOk: () => void;
  } | null>(null);
  // 线程 / 打开的文件列表：默认只显示前 20 条，可展开全部
  const [showAllThreads, setShowAllThreads] = useState(false);
  const [showAllFiles, setShowAllFiles] = useState(false);
  const [openFiles, setOpenFiles] = useState<OpenFile[] | null>(null);
  const [filesLoading, setFilesLoading] = useState(false);
  // 进程详情缓存（TTL 5s，键含 refreshKey）：频繁切换不重复发 invoke，
  // 手动刷新后旧缓存失效重新拉取
  const detailCache = useRef<Map<number, { data: ProcDetail; ts: number; rk: number }>>(new Map());
  // 打开的文件缓存（TTL 5s，句柄枚举较重）
  const filesCache = useRef<Map<number, { data: OpenFile[]; ts: number; rk: number }>>(new Map());

  // 未选中进程 → 空状态（无滑出动画需求，dock 面板直接卸载）
  useEffect(() => {
    if (selectedPid == null) {
      setDetail(null);
      setOpenFiles(null);
      return;
    }
    const hit = detailCache.current.get(selectedPid);
    if (hit && hit.rk === refreshKey && Date.now() - hit.ts < 5000) {
      setDetail(hit.data);
      return;
    }
    let cancelled = false;
    getProcessDetail(selectedPid)
      .then((d) => {
        if (!cancelled && d) {
          detailCache.current.set(selectedPid, { data: d, ts: Date.now(), rk: refreshKey });
          setDetail(d);
        }
      })
      .catch((e) => setNotice(String(e)));
    return () => {
      cancelled = true;
    };
  }, [selectedPid, refreshKey]);

  // 打开的文件：与详情并行懒加载
  useEffect(() => {
    if (selectedPid == null) return;
    setOpenFiles(null);
    const hit = filesCache.current.get(selectedPid);
    if (hit && hit.rk === refreshKey && Date.now() - hit.ts < 5000) {
      setOpenFiles(hit.data);
      return;
    }
    let cancelled = false;
    setFilesLoading(true);
    processOpenFiles(selectedPid)
      .then((d) => {
        if (cancelled) return;
        filesCache.current.set(selectedPid, { data: d, ts: Date.now(), rk: refreshKey });
        setOpenFiles(d);
      })
      .catch((e) => {
        if (!cancelled) {
          setOpenFiles([]);
          setNotice(String(e));
        }
      })
      .finally(() => {
        if (!cancelled) setFilesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedPid, refreshKey]);

  // 复制文本到剪贴板并提示
  const copyText = useMemo(
    () => async (t: string, label?: string) => {
      try {
        await navigator.clipboard?.writeText(t);
        setNotice(label ? `${label}已复制` : "已复制");
      } catch {
        /* 剪贴板不可用静默 */
      }
    },
    [],
  );

  const doKill = async (pids: number[]) => {
    setBusy(true);
    try {
      const res = await killProcesses(pids);
      const failed = res.filter((r) => !r.ok);
      setNotice(
        res.length === 0
          ? "无进程被结束"
          : failed.length === 0
            ? `已结束 ${res.length} 个进程`
            : `已结束 ${res.length - failed.length} 个，失败 ${failed.length} 个：${failed
                .map((r) => `${r.name}(${r.pid})${r.error ? " " + r.error : ""}`)
                .join("；")}`,
      );
      selectPid(null);
      refreshAll();
    } catch (e) {
      setNotice(String(e));
    }
    setBusy(false);
  };

  // 先弹应用内确认框，确认后再执行（替代 window.confirm）
  const kill = (pids: number[], label: string) => {
    setConfirm({
      title: "结束进程",
      message: `确定结束${label}（${pids.length} 个进程）？`,
      onOk: () => {
        setConfirm(null);
        doKill(pids);
      },
    });
  };

  // 暂停/恢复进程（NtSuspendProcess / NtResumeProcess），成功后刷新详情与列表
  const toggleSuspend = async (suspend: boolean) => {
    if (!detail || opBusy) return;
    setOpBusy(true);
    try {
      if (suspend) {
        await processSuspend(detail.pid);
        setNotice("进程已暂停（全部线程挂起）");
      } else {
        await processResume(detail.pid);
        setNotice("进程已恢复");
      }
      refreshAll();
    } catch (e) {
      setNotice(String(e));
    }
    setOpBusy(false);
  };

  // 设置优先级类（SetPriorityClass）
  const applyPriority = async (v: number) => {
    if (!detail || opBusy || v === detail.priority) return;
    setOpBusy(true);
    try {
      await processPriority(detail.pid, v);
      setNotice(`优先级已设为「${prioLabel(v)}」`);
      refreshAll();
    } catch (e) {
      setNotice(String(e));
    }
    setOpBusy(false);
  };

  if (selectedPid == null) {
    return (
      <div className="detail-panel">
        <div className="detail-empty">在进程列表中选择进程查看详情</div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="detail-panel">
        <div className="detail-empty">加载中…</div>
      </div>
    );
  }

  return (
    <div className="detail-panel">
      <div className="detail-head">
        <span className="detail-title">
          {detail.name}
          <span className="dim"> (PID {detail.pid})</span>
        </span>
        <span className={"badge " + (detail.status === "运行中" ? "ok" : "")}>
          {detail.status}
        </span>
      </div>

      {notice && <div className="notice">{notice}</div>}

      <div className="detail-scroll">
        {/* 基本信息 */}
        <div className="detail-sec">
          <div className="detail-sec-title">基本信息</div>
          <dl className="detail-fields">
            <dt>内存</dt>
            <dd>{formatSize(detail.mem)}</dd>
            <dt>线程数</dt>
            <dd>{detail.threads}</dd>
            <dt>句柄数</dt>
            <dd>{detail.handles}</dd>
            <dt>优先级</dt>
            <dd>
              <span className="jump" title="点击修改优先级">
                {prioLabel(detail.priority)}
              </span>
            </dd>
            <dt>启动时间</dt>
            <dd>{fmtTime(detail.start)}</dd>
          </dl>
        </div>

        {/* 进程控制 */}
        <div className="detail-sec">
          <div className="detail-sec-title">进程控制</div>
          <div className="detail-controls">
            <button
              className="btn ghost small"
              disabled={opBusy}
              onClick={() => toggleSuspend(true)}
              title="挂起进程全部线程（系统进程可能失败）"
            >
              暂停
            </button>
            <button
              className="btn ghost small"
              disabled={opBusy}
              onClick={() => toggleSuspend(false)}
              title="恢复被暂停的进程"
            >
              恢复
            </button>
            <select
              className="prio-select"
              value={detail.priority}
              disabled={opBusy}
              onChange={(e) => applyPriority(Number(e.target.value))}
              title="设置进程优先级类"
            >
              {PRIO_OPTS.map((o) => (
                <option key={o.v} value={o.v}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* 路径与命令行 */}
        <div className="detail-sec">
          <div className="detail-sec-title">
            路径与命令行
            {detail.path && detail.cmd && (
              <button
                className="btn ghost small"
                onClick={() => copyText(`${detail.path}\n${detail.cmd}`)}
              >
                全部复制
              </button>
            )}
          </div>
          <dl className="detail-fields">
            <dt>路径</dt>
            <dd className="mono wrap">
              {detail.path || "-"}
              {detail.path && (
                <button
                  className="copy-mini"
                  onClick={() => copyText(detail.path, "路径 ")}
                >
                  复制
                </button>
              )}
            </dd>
            <dt>命令行</dt>
            <dd className="mono wrap">
              {detail.cmd || "-"}
              {detail.cmd && (
                <button
                  className="copy-mini"
                  onClick={() => copyText(detail.cmd, "命令行 ")}
                >
                  复制
                </button>
              )}
            </dd>
          </dl>
        </div>

        {/* 打开的文件 */}
        <div className="detail-sec">
          <div className="detail-sec-title">
            打开的文件{openFiles ? `（${openFiles.length}）` : ""}
            {openFiles && openFiles.length > 20 && (
              <button
                className="btn ghost small"
                onClick={() => setShowAllFiles((v) => !v)}
              >
                {showAllFiles ? "收起" : "全部"}
              </button>
            )}
          </div>
          {filesLoading ? (
            <div className="dim sec-empty">加载中…</div>
          ) : openFiles === null || openFiles.length === 0 ? (
            <div className="dim sec-empty">无（提权后可见系统进程的文件）</div>
          ) : (
            <div className="file-list">
              {(showAllFiles ? openFiles : openFiles.slice(0, 20)).map((f) => (
                <div
                  className="file-row"
                  key={f.path}
                  title="点击复制路径"
                  onClick={() => copyText(f.path, "路径 ")}
                >
                  <span className="file-path">{f.path}</span>
                  {f.count > 1 && <span className="file-count">×{f.count}</span>}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 网络连接 */}
        <div className="detail-sec">
          <div className="detail-sec-title">网络连接（{detail.conns.length}）</div>
          {detail.conns.length === 0 ? (
            <div className="dim sec-empty">无</div>
          ) : (
            <div className="conn-list">
              {detail.conns.map((c, i) => (
                <div
                  className="conn-row"
                  key={i}
                  title={c.remote ? `远程 ${c.remote}（点击复制）` : "点击复制端口号"}
                  onClick={() => copyText(String(c.port), `端口 ${c.port} `)}
                >
                  <span className="conn-port">{c.port}</span>
                  <span className="conn-proto">{c.proto}</span>
                  <span className={"conn-state" + (c.state === "监听" ? " ok" : "")}>
                    {c.state}
                  </span>
                  {c.remote && <span className="conn-remote">{c.remote}</span>}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 线程 */}
        <div className="detail-sec">
          <div className="detail-sec-title">
            线程（{detail.thread_list.length}）
            {detail.thread_list.length > 20 && (
              <button
                className="btn ghost small"
                onClick={() => setShowAllThreads((v) => !v)}
              >
                {showAllThreads ? "收起" : "全部"}
              </button>
            )}
          </div>
          {detail.thread_list.length === 0 ? (
            <div className="dim sec-empty">无</div>
          ) : (
            <div className="thread-list">
              {(showAllThreads
                ? detail.thread_list
                : detail.thread_list.slice(0, 20)
              ).map((t) => (
                <div className="thread-row" key={t.tid}>
                  <span className="thread-tid">TID {t.tid}</span>
                  <span className="thread-prio">
                    优先级 {t.base_prio}
                    {t.delta_prio ? `（当前 ±${t.delta_prio}）` : ""}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* 进程关系 */}
        <div className="detail-sec">
          <div className="detail-sec-title">进程关系</div>
          <dl className="detail-fields">
            <dt>父进程</dt>
            <dd>
              {detail.parent ? (
                <span
                  className="jump"
                  onClick={() => selectPid(detail.parent)}
                  title="点击查看父进程"
                >
                  PID {detail.parent}
                </span>
              ) : (
                "-"
              )}
            </dd>
            <dt>子进程</dt>
            <dd>
              {detail.children.length
                ? detail.children.map((c) => (
                    <span
                      key={c}
                      className="jump"
                      onClick={() => selectPid(c)}
                      title="点击查看子进程"
                    >
                      PID {c}
                    </span>
                  ))
                : "无"}
            </dd>
          </dl>
        </div>
      </div>

      <div className="detail-actions">
        <button
          className="btn danger"
          disabled={busy}
          onClick={() => kill([detail.pid], detail.name)}
        >
          结束进程
        </button>
        <button
          className="btn danger ghost"
          disabled={busy}
          onClick={() =>
            killTreeProcess(detail.pid)
              .then((ids) => kill(ids, `${detail.name} 及其子进程`))
              .catch((e) => setNotice(String(e)))
          }
        >
          结束进程树
        </button>
      </div>

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
