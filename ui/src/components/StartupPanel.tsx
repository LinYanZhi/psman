import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { listStartupItems, startupSetEnabled } from "../api.ts";
import type { StartupItem } from "../api.ts";
import { refreshAll, useAppState } from "../store.ts";

// 启动项面板：Run 注册表键 + 启动文件夹（后端 list_startup_items / startup_set_enabled）。
// 启用/禁用写 Explorer\StartupApproved 标记，与任务管理器「启动」页同机制，不删除原值。
export default function StartupPanel() {
  const { refreshKey } = useAppState();
  const [rows, setRows] = useState<StartupItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  // 按来源分组展示（运行键在前、文件夹在后）
  const [groupBy, setGroupBy] = useState(true);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const inflight = useRef(false);

  const load = useCallback(async () => {
    if (inflight.current) return;
    inflight.current = true;
    try {
      setRows(await listStartupItems());
    } catch (e) {
      setNotice(String(e));
    } finally {
      inflight.current = false;
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [refreshKey, load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        r.command.toLowerCase().includes(q) ||
        r.source.toLowerCase().includes(q),
    );
  }, [rows, query]);

  // 来源顺序稳定分组：按 rows 首次出现的顺序
  const groups = useMemo(() => {
    if (!groupBy) return null;
    const map = new Map<string, StartupItem[]>();
    for (const r of filtered) {
      const arr = map.get(r.source) ?? [];
      arr.push(r);
      map.set(r.source, arr);
    }
    return [...map.entries()];
  }, [filtered, groupBy]);

  const toggle = async (r: StartupItem) => {
    setBusyKey(`${r.source}|${r.name}`);
    try {
      await startupSetEnabled(r.source, r.name, !r.enabled);
      setNotice(`${r.name} 已${r.enabled ? "禁用" : "启用"}`);
      await load();
    } catch (e) {
      setNotice(String(e));
    }
    setBusyKey(null);
  };

  const renderRow = (r: StartupItem) => (
    <tr key={`${r.source}|${r.name}`}>
      <td className="name">
        {r.name}
        <span className={"badge mini" + (r.kind === "folder" ? " folder" : "")}>
          {r.kind === "folder" ? "文件夹" : "注册表"}
        </span>
      </td>
      <td className="mono cmd-cell" title={r.command}>
        {r.command || "-"}
      </td>
      <td>
        <span className={"conn-state" + (r.enabled ? " ok" : "")}>
          {r.enabled ? "启用" : "禁用"}
        </span>
      </td>
      <td className="st-actions">
        <button
          className={"btn ghost small" + (r.enabled ? " danger-text" : "")}
          disabled={busyKey === `${r.source}|${r.name}`}
          onClick={() => toggle(r)}
        >
          {busyKey === `${r.source}|${r.name}` ? "处理中…" : r.enabled ? "禁用" : "启用"}
        </button>
      </td>
    </tr>
  );

  return (
    <div className="view">
      <div className="view-toolbar">
        <input
          className="search"
          placeholder="搜索名称 / 命令 / 来源"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="count">{filtered.length} 个启动项</span>
        <button
          className={"btn ghost" + (groupBy ? " active" : "")}
          onClick={() => setGroupBy((v) => !v)}
          title="按来源分组展示"
        >
          分组
        </button>
        {notice && <span className="notice">{notice}</span>}
        <button className="btn ghost" onClick={() => refreshAll()}>
          刷新
        </button>
      </div>

      <div className="table-wrap">
        <table className="grid startup-grid">
          <thead>
            <tr>
              <th>名称</th>
              <th>命令</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={4} className="empty">
                  加载中…
                </td>
              </tr>
            ) : filtered.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty">
                  没有匹配的启动项
                </td>
              </tr>
            ) : groups ? (
              groups.flatMap(([source, items]) => [
                <tr className="group-head" key={source}>
                  <td colSpan={4}>
                    <span className="group-title">{source}</span>
                    <span className="dim">（{items.length}）</span>
                  </td>
                </tr>,
                ...items.map(renderRow),
              ])
            ) : (
              filtered.map(renderRow)
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
