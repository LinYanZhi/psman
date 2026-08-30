import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import {
  killProcesses,
  killTreeProcess,
  listProcesses,
  revealInFolder,
} from "../api.ts";
import type { ProcInfo } from "../api.ts";
import { formatSize, fmtTime } from "../utils.ts";
import { refreshAll, selectPid, useAppState } from "../store.ts";
import ConfirmDialog from "./ConfirmDialog.tsx";
import ProcessIcon from "./ProcessIcon.tsx";

type SortKey = "pid" | "name" | "cpu" | "mem" | "threads" | "status" | "ports" | "start";
type Sort = { key: SortKey; dir: "asc" | "desc" } | null;

function Th({
  k,
  s,
  onSort,
  className,
  children,
}: {
  k: SortKey;
  s: Sort;
  onSort: (k: SortKey) => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <th className={(className ?? "") + " sortable"} onClick={() => onSort(k)}>
      {children}
      {s?.key === k && <span className="sort-ind">{s.dir === "asc" ? "▲" : "▼"}</span>}
    </th>
  );
}

export default function ProcessListPanel() {
  const { selectedPid, refreshKey } = useAppState();
  const [procs, setProcs] = useState<ProcInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  // 默认按内存降序：打开即有信息量（任务管理器式）
  const [sort, setSort] = useState<Sort>({ key: "mem", dir: "desc" });
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Ctrl/Shift 多选（普通点击仍走 store 单选，联动详情面板）
  const [multi, setMulti] = useState<Set<number>>(new Set());
  const lastClick = useRef<number | null>(null);
  // 结束进程前二次确认（替代 window.confirm）
  const [confirm, setConfirm] = useState<{
    title: string;
    message: string;
    onOk: () => void;
  } | null>(null);
  // 右键菜单状态（鼠标位置 + 目标进程）
  const [ctx, setCtx] = useState<{ x: number; y: number; pid: number; name: string; exe: string } | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  // 树形视图：进程按父子关系缩进展示，可展开/折叠（搜索时自动拍平为普通列表）
  const [treeMode, setTreeMode] = useState(false);
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  // 同名进程分组视图：任务管理器式聚合（chrome 等几十个实例折叠为一行），与树形互斥
  const [groupMode, setGroupMode] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  // 轮询防重入 + 首帧 loading 控制（之后静默更新不闪屏）
  const inflight = useRef(false);
  const firstLoad = useRef(true);

  useEffect(() => {
    const close = () => setCtx(null);
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCtx(null);
    };
    window.addEventListener("click", close);
    window.addEventListener("contextmenu", close);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("contextmenu", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", esc);
    };
  }, []);

  // 菜单位置贴边自适应
  useLayoutEffect(() => {
    if (!ctx || !menuRef.current) return;
    const el = menuRef.current;
    const r = el.getBoundingClientRect();
    const pad = 4;
    let left = ctx.x;
    let top = ctx.y;
    if (left + r.width > window.innerWidth - pad) {
      left = Math.max(pad, window.innerWidth - r.width - pad);
    }
    if (top + r.height > window.innerHeight - pad) {
      top = Math.max(pad, window.innerHeight - r.height - pad);
    }
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  }, [ctx]);

  const load = useCallback(async () => {
    if (inflight.current) return;
    inflight.current = true;
    try {
      setProcs(await listProcesses());
    } catch (e) {
      setNotice(String(e));
    } finally {
      inflight.current = false;
      if (firstLoad.current) {
        firstLoad.current = false;
        setLoading(false);
      }
    }
  }, []);

  // 首载 + 手动/跨窗口刷新（refreshKey）→ 静默更新，不闪屏
  useEffect(() => {
    load();
  }, [refreshKey, load]);

  // 自动轮询（2.5s，静默更新）
  useEffect(() => {
    const t = setInterval(load, 2500);
    return () => clearInterval(t);
  }, [load]);

  // F5 刷新 / Ctrl+F 聚焦搜索
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F5") {
        e.preventDefault();
        load();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const base = q
      ? procs.filter(
          (p) =>
            p.pid.toString().includes(q) ||
            p.name.toLowerCase().includes(q) ||
            p.ports.some((pt) => String(pt).includes(q)),
        )
      : procs;
    if (!sort) return base;
    const { key, dir } = sort;
    const mul = dir === "asc" ? 1 : -1;
    return [...base].sort((a, b) => {
      let r: number;
      switch (key) {
        case "pid":
          r = a.pid - b.pid;
          break;
        case "name":
          r = a.name.localeCompare(b.name, "zh");
          break;
        case "cpu":
          r = a.cpu - b.cpu;
          break;
        case "mem":
          r = a.mem - b.mem;
          break;
        case "threads":
          r = a.threads - b.threads;
          break;
        case "status":
          r = a.status.localeCompare(b.status, "zh");
          break;
        case "ports":
          r = (a.ports[0] ?? -1) - (b.ports[0] ?? -1);
          break;
        case "start":
          r = a.start - b.start;
          break;
      }
      return r * mul;
    });
  }, [procs, query, sort]);

  // 父子映射（父 PID → 直接子进程），仅在树形模式下使用；子列表顺序跟随当前排序
  const childrenMap = useMemo(() => {
    const byPid = new Map<number, ProcInfo>();
    for (const p of procs) byPid.set(p.pid, p);
    const map = new Map<number, ProcInfo[]>();
    for (const p of procs) {
      if (p.parent !== 0 && byPid.has(p.parent)) {
        const arr = map.get(p.parent) ?? [];
        arr.push(p);
        map.set(p.parent, arr);
      }
    }
    return map;
  }, [procs]);

  // 树形行：roots（父进程不存在/为 0）起 DFS，按 expanded 展开；visited 防父子环死循环
  const treeRows = useMemo(() => {
    if (!treeMode || query.trim() !== "") return null;
    const byPid = new Map<number, ProcInfo>();
    for (const p of procs) byPid.set(p.pid, p);
    const roots = procs.filter((p) => p.parent === 0 || !byPid.has(p.parent));
    const out: { p: ProcInfo; depth: number }[] = [];
    const visited = new Set<number>();
    const visit = (pid: number, depth: number) => {
      if (visited.has(pid)) return;
      visited.add(pid);
      const node = byPid.get(pid);
      if (!node) return;
      out.push({ p: node, depth });
      if (expanded.has(pid)) {
        for (const c of childrenMap.get(pid) ?? []) visit(c.pid, depth + 1);
      }
    };
    for (const r of roots) visit(r.pid, 0);
    return out;
  }, [procs, query, treeMode, expanded, childrenMap]);

  const rows: { p: ProcInfo; depth: number }[] = treeRows ?? filtered.map((p) => ({ p, depth: 0 }));

  // 同名分组行：按名称聚合 CPU/内存/线程，展开后显示各实例（按 PID 升序）。
  // 聚合行排序跟随当前排序键，默认内存降序（与列表默认一致）
  const groupRows = useMemo(() => {
    if (!groupMode || query.trim() !== "") return null;
    const byName = new Map<string, ProcInfo[]>();
    for (const p of procs) {
      const arr = byName.get(p.name) ?? [];
      arr.push(p);
      byName.set(p.name, arr);
    }
    const list = [...byName.entries()].map(([name, items]) => ({
      name,
      count: items.length,
      mem: items.reduce((s, p) => s + p.mem, 0),
      cpu: items.reduce((s, p) => s + p.cpu, 0),
      threads: items.reduce((s, p) => s + p.threads, 0),
      items: [...items].sort((a, b) => a.pid - b.pid),
    }));
    list.sort((a, b) => {
      if (sort?.key === "name") {
        const r = a.name.localeCompare(b.name, "zh");
        return sort.dir === "asc" ? r : -r;
      }
      if (sort?.key === "cpu") return sort.dir === "asc" ? a.cpu - b.cpu : b.cpu - a.cpu;
      if (sort?.key === "threads") return sort.dir === "asc" ? a.threads - b.threads : b.threads - a.threads;
      // pid/status/ports/start 等键对聚合无意义，统一按内存
      return sort?.dir === "asc" ? a.mem - b.mem : b.mem - a.mem;
    });
    return list;
  }, [procs, query, groupMode, sort]);

  const grouped = groupRows !== null;

  const toggleExpand = (pid: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(pid)) next.delete(pid);
      else next.add(pid);
      return next;
    });
  };

  const toggleGroup = (name: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const expandAll = () => setExpanded(new Set(childrenMap.keys()));
  const collapseAll = () => setExpanded(new Set());
  const expandAllGroups = () => setExpandedGroups(new Set(groupRows?.map((g) => g.name) ?? []));
  const collapseAllGroups = () => setExpandedGroups(new Set());

  const portHint = useMemo(() => {
    const q = query.trim();
    if (!/^\d{1,5}$/.test(q)) return null;
    const port = parseInt(q, 10);
    if (port < 1 || port > 65535) return null;
    return { port, hits: procs.filter((p) => p.ports.includes(port)) };
  }, [procs, query]);

  const onSort = (k: SortKey) => {
    setSort((s) => {
      if (!s || s.key !== k) return { key: k, dir: "asc" };
      if (s.dir === "asc") return { key: k, dir: "desc" };
      return null;
    });
  };

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
      setMulti(new Set());
      selectPid(null);
      await load();
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

  // Del 结束选中（任务管理器式快捷键；输入框/菜单聚焦时不触发）。
  // 必须放在 kill 定义之后（独立 effect，引用已声明的 kill，避开 TDZ）
  useEffect(() => {
    const onDelete = (e: KeyboardEvent) => {
      if (e.key !== "Delete" || busy) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const pids = multi.size > 0 ? [...multi] : selectedPid != null ? [selectedPid] : [];
      if (pids.length === 0) return;
      e.preventDefault();
      const names = pids.map((pid) => procs.find((p) => p.pid === pid)?.name ?? "进程");
      kill(pids, pids.length === 1 ? names[0] : `${pids.length} 个选中进程`);
    };
    window.addEventListener("keydown", onDelete);
    return () => window.removeEventListener("keydown", onDelete);
  }, [busy, multi, selectedPid, procs, kill]);

  const cpuCls = (v: number) => (v >= 70 ? "hi" : v >= 30 ? "mid" : "");
  // 树形模式真正生效（未搜索拍平）时，展开按钮才可见可点
  const treeActive = treeRows !== null;

  // 单行渲染：树形缩进 / 分组实例共用（双击打开文件位置，Ctrl/Shift 多选同前）
  const procRow = (p: ProcInfo, depth: number) => {
    const hasChildren = (childrenMap.get(p.pid)?.length ?? 0) > 0;
    return (
      <tr
        key={p.pid}
        className={p.pid === selectedPid || multi.has(p.pid) ? "row selected" : "row"}
        title="单击选中 · Ctrl 点击多选 · Shift 连选 · 双击打开文件位置"
        onClick={(e) => {
          if (e.ctrlKey || e.metaKey) {
            lastClick.current = p.pid;
            setMulti((prev) => {
              const next = new Set(prev);
              if (next.has(p.pid)) next.delete(p.pid);
              else next.add(p.pid);
              return next;
            });
          } else if (e.shiftKey && lastClick.current != null) {
            const ids = grouped
              ? groupRows!.flatMap((g) => g.items.map((x) => x.pid))
              : rows.map((x) => x.p.pid);
            const a = ids.indexOf(lastClick.current);
            const b = ids.indexOf(p.pid);
            if (a >= 0 && b >= 0) {
              const [lo, hi] = a < b ? [a, b] : [b, a];
              setMulti(new Set(ids.slice(lo, hi + 1)));
            }
            lastClick.current = p.pid;
          } else {
            setMulti(new Set());
            lastClick.current = p.pid;
            selectPid(p.pid === selectedPid ? null : p.pid);
          }
        }}
        onDoubleClick={() => {
          if (p.exe) revealInFolder(p.exe).catch((e) => setNotice(String(e)));
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          setCtx({ x: e.clientX, y: e.clientY, pid: p.pid, name: p.name, exe: p.exe });
        }}
      >
        <td className="num">{p.pid}</td>
        <td className="name">
          <span className="tree-cell" style={{ paddingLeft: Math.min(depth, 12) * 18 }}>
            {treeActive && hasChildren && (
              <button
                className="tree-toggle"
                onClick={(e) => {
                  e.stopPropagation();
                  toggleExpand(p.pid);
                }}
                title={expanded.has(p.pid) ? "折叠" : "展开"}
              >
                {expanded.has(p.pid) ? "▾" : "▸"}
              </button>
            )}
            {p.exe ? <ProcessIcon path={p.exe} /> : null}
            {p.name}
          </span>
        </td>
        <td className="num">
          <span className="cpu-cell">
            <span className={"cpu-bar " + cpuCls(p.cpu)}>
              <i style={{ width: Math.min(100, p.cpu) + "%" }} />
            </span>
            <span className="cpu-val">{p.cpu.toFixed(1)}%</span>
          </span>
        </td>
        <td className="num">{formatSize(p.mem)}</td>
        <td className="num">{p.threads}</td>
        <td>{p.status}</td>
        <td className="ports">
          {p.ports.length > 3
            ? `${p.ports.slice(0, 3).join(", ")}…(+${p.ports.length - 3})`
            : p.ports.join(", ") || "-"}
        </td>
        <td className="time">{fmtTime(p.start)}</td>
      </tr>
    );
  };

  return (
    <div className="view process-view">
      <div className="view-toolbar">
        <input
          ref={searchRef}
          className="search"
          placeholder="搜索 PID / 进程名 / 端口"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="count">
          {groupRows
            ? `${groupRows.length} 个进程组`
            : treeRows
              ? `${treeRows.length} 个进程`
              : `${filtered.length} 个进程`}
        </span>
        <button
          className={"btn ghost" + (treeMode ? " active" : "")}
          onClick={() => {
            setTreeMode((v) => !v);
            if (!treeMode) setGroupMode(false);
          }}
          title="按父子关系缩进展示；搜索时自动拍平"
        >
          树形
        </button>
        <button
          className={"btn ghost" + (groupMode ? " active" : "")}
          onClick={() => {
            setGroupMode((v) => !v);
            if (!groupMode) setTreeMode(false);
          }}
          title="同名进程聚合为一行（任务管理器式），点击组行展开各实例"
        >
          分组
        </button>
        {(treeRows || groupRows) && (
          <>
            <button
              className="btn ghost"
              onClick={treeRows ? expandAll : expandAllGroups}
              title="展开全部"
            >
              全部展开
            </button>
            <button
              className="btn ghost"
              onClick={treeRows ? collapseAll : collapseAllGroups}
              title="收起全部"
            >
              全部收起
            </button>
          </>
        )}
        {portHint && (
          <span
            className={"port-hint " + (portHint.hits.length ? "busy" : "free")}
            title={
              portHint.hits.length
                ? portHint.hits.map((h) => `${h.name} (PID ${h.pid})`).join("\n")
                : "该端口当前未被任何进程绑定"
            }
          >
            {portHint.hits.length
              ? `端口 ${portHint.port} 被 ${portHint.hits.length} 个进程占用`
              : `端口 ${portHint.port} 空闲`}
          </span>
        )}
        {multi.size > 0 && (
          <>
            <span className="count">已选 {multi.size} 个</span>
            <button
              className="btn danger"
              disabled={busy}
              onClick={() => kill([...multi], `${multi.size} 个选中进程`)}
            >
              结束选中
            </button>
          </>
        )}
        {notice && <span className="notice">{notice}</span>}
        <button className="btn ghost" onClick={() => refreshAll()}>
          刷新
        </button>
      </div>

      <div className="table-wrap">
        <table className="grid">
          <thead>
            <tr>
              <Th k="pid" s={sort} onSort={onSort} className="num">
                PID
              </Th>
              <Th k="name" s={sort} onSort={onSort}>
                名称
              </Th>
              <Th k="cpu" s={sort} onSort={onSort} className="num">
                CPU
              </Th>
              <Th k="mem" s={sort} onSort={onSort} className="num">
                内存
              </Th>
              <Th k="threads" s={sort} onSort={onSort} className="num">
                线程
              </Th>
              <Th k="status" s={sort} onSort={onSort}>
                状态
              </Th>
              <Th k="ports" s={sort} onSort={onSort}>
                端口
              </Th>
              <Th k="start" s={sort} onSort={onSort}>
                启动时间
              </Th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={8} className="empty">
                  加载中…
                </td>
              </tr>
            ) : filtered.length === 0 ? (
              <tr>
                <td colSpan={8} className="empty">
                  没有匹配的进程
                </td>
              </tr>
            ) : grouped ? (
              groupRows!.flatMap((g) => {
                const open = expandedGroups.has(g.name);
                const head = (
                  <tr
                    key={"g:" + g.name}
                    className="row group-row"
                    onClick={() => toggleGroup(g.name)}
                    title="点击展开 / 折叠该进程组"
                  >
                    <td className="num">{g.count === 1 ? g.items[0].pid : ""}</td>
                    <td className="name">
                      <span className="tree-cell">
                        <button
                          className="tree-toggle"
                          onClick={(e) => {
                            e.stopPropagation();
                            toggleGroup(g.name);
                          }}
                          title={open ? "折叠" : "展开"}
                        >
                          {open ? "▾" : "▸"}
                        </button>
                        {g.name}
                        <span className="dim"> ×{g.count}</span>
                      </span>
                    </td>
                    <td className="num">
                      <span className="cpu-cell">
                        <span className={"cpu-bar " + cpuCls(g.cpu)}>
                          <i style={{ width: Math.min(100, g.cpu) + "%" }} />
                        </span>
                        <span className="cpu-val">{g.cpu.toFixed(1)}%</span>
                      </span>
                    </td>
                    <td className="num">{formatSize(g.mem)}</td>
                    <td className="num">{g.threads}</td>
                    <td className="dim">—</td>
                    <td className="ports">—</td>
                    <td className="time">—</td>
                  </tr>
                );
                const rows = open ? g.items.map((p) => procRow(p, 1)) : [];
                return [head, ...rows];
              })
            ) : (
              rows.map(({ p, depth }) => procRow(p, depth))
            )}
          </tbody>
        </table>
      </div>

      {ctx && (
        <div
          ref={menuRef}
          className="ctx-menu"
          style={{ left: ctx.x, top: ctx.y }}
          onContextMenu={(e) => e.preventDefault()}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="ctx-head">
            <span className="ctx-name" title={ctx.name}>
              {ctx.name}
            </span>
            <span className="ctx-pid">PID {ctx.pid}</span>
          </div>
          <button
            className="ctx-item danger"
            disabled={busy}
            onClick={() => {
              setCtx(null);
              kill([ctx.pid], ctx.name);
            }}
          >
            结束进程
          </button>
          <button
            className="ctx-item danger"
            disabled={busy}
            onClick={() => {
              setCtx(null);
              killTreeProcess(ctx.pid)
                .then((ids) => kill(ids, `${ctx.name} 及其子进程`))
                .catch((e) => setNotice(String(e)));
            }}
          >
            结束进程树
          </button>
          <div className="ctx-sep" />
          <button
            className="ctx-item"
            disabled={!ctx.exe}
            onClick={() => {
              setCtx(null);
              revealInFolder(ctx.exe).catch((e) => setNotice(String(e)));
            }}
          >
            打开文件位置
          </button>
          <button
            className="ctx-item"
            disabled={!ctx.exe}
            onClick={() => {
              setCtx(null);
              navigator.clipboard
                ?.writeText(ctx.exe)
                .then(() => setNotice(`已复制路径：${ctx.exe}`))
                .catch(() => {});
            }}
          >
            复制路径
          </button>
          <button
            className="ctx-item"
            onClick={() => {
              setCtx(null);
              navigator.clipboard
                ?.writeText(ctx.name)
                .then(() => setNotice(`已复制进程名：${ctx.name}`))
                .catch(() => {});
            }}
          >
            复制进程名
          </button>
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
