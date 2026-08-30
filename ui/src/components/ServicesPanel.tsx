import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import { listServices, serviceAction } from "../api.ts";
import type { ServiceInfo } from "../api.ts";
import { refreshAll, useAppState } from "../store.ts";

type SortKey = "name" | "display" | "state" | "start_type" | "pid" | "path";
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

// Windows 服务面板：全量服务列表 + 启动/停止/重启（后端 list_services / service_action）。
// 停止/启动请求交给 SCM 异步执行，这里仅提示请求结果；权限不足（需管理员）返回错误。
export default function ServicesPanel() {
  const { refreshKey } = useAppState();
  const [rows, setRows] = useState<ServiceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // 操作锁：同一时间只允许一个启停请求（按服务名记）
  const [busyName, setBusyName] = useState<string | null>(null);
  const inflight = useRef(false);

  const load = useCallback(async () => {
    if (inflight.current) return;
    inflight.current = true;
    try {
      setRows(await listServices());
    } catch (e) {
      setNotice(String(e));
    } finally {
      inflight.current = false;
      setLoading(false);
    }
  }, []);

  // 首载 + 手动/跨窗口刷新
  useEffect(() => {
    load();
  }, [refreshKey, load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    let list = !q
      ? rows
      : rows.filter(
          (r) =>
            r.name.toLowerCase().includes(q) ||
            r.display.toLowerCase().includes(q) ||
            r.path.toLowerCase().includes(q),
        );
    if (!sort) return list;
    const mul = sort.dir === "asc" ? 1 : -1;
    return [...list].sort((a, b) => {
      let r: number;
      switch (sort.key) {
        case "name":
          r = a.name.localeCompare(b.name, "zh");
          break;
        case "display":
          r = a.display.localeCompare(b.display, "zh");
          break;
        case "state":
          // 按原始状态码排：已停止(1) < 运行中(4) < 已暂停(7) < 过渡态
          r = a.state_code - b.state_code;
          break;
        case "start_type":
          r = a.start_type.localeCompare(b.start_type, "zh");
          break;
        case "pid":
          r = a.pid - b.pid;
          break;
        case "path":
          r = a.path.localeCompare(b.path, "zh");
          break;
      }
      return r * mul;
    });
  }, [rows, query, sort]);

  const onSort = (k: SortKey) => {
    setSort((s) => {
      if (!s || s.key !== k) return { key: k, dir: "asc" };
      if (s.dir === "asc") return { key: k, dir: "desc" };
      return null;
    });
  };

  // 服务状态码：1 已停止，4 运行中，7 已暂停；其余为过渡状态
  const running = (c: number) => c === 4 || c === 7;
  const transitional = (c: number) => c === 2 || c === 3 || c === 5 || c === 6;

  const act = async (s: ServiceInfo, action: "start" | "stop" | "restart") => {
    setBusyName(s.name);
    try {
      await serviceAction(s.name, action);
      const label = action === "start" ? "启动" : action === "stop" ? "停止" : "重启";
      setNotice(`${s.display || s.name} 已${label}（状态可能异步生效）`);
      // 立即拉一次，状态过渡期由用户手动刷新或等待
      await load();
    } catch (e) {
      setNotice(String(e));
    }
    setBusyName(null);
  };

  return (
    <div className="view">
      <div className="view-toolbar">
        <input
          className="search"
          placeholder="搜索服务名 / 显示名 / 路径"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="count">{filtered.length} 个服务</span>
        {notice && <span className="notice">{notice}</span>}
        <button className="btn ghost" onClick={() => refreshAll()}>
          刷新
        </button>
      </div>

      <div className="table-wrap">
        <table className="grid services-grid">
          <thead>
            <tr>
              <Th k="name" s={sort} onSort={onSort}>
                服务名
              </Th>
              <Th k="display" s={sort} onSort={onSort}>
                显示名
              </Th>
              <Th k="state" s={sort} onSort={onSort}>
                状态
              </Th>
              <Th k="start_type" s={sort} onSort={onSort}>
                启动类型
              </Th>
              <Th k="pid" s={sort} onSort={onSort} className="num">
                PID
              </Th>
              <Th k="path" s={sort} onSort={onSort}>
                可执行文件
              </Th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={7} className="empty">
                  加载中…
                </td>
              </tr>
            ) : filtered.length === 0 ? (
              <tr>
                <td colSpan={7} className="empty">
                  没有匹配的服务
                </td>
              </tr>
            ) : (
              filtered.map((s) => (
                <tr key={s.name}>
                  <td className="mono">{s.name}</td>
                  <td className="name">{s.display || "-"}</td>
                  <td>
                    <span className={"conn-state" + (running(s.state_code) ? " ok" : "")}>
                      {s.state}
                    </span>
                  </td>
                  <td>{s.start_type}</td>
                  <td className="num">{s.pid || "-"}</td>
                  <td className="mono path-cell" title={s.path}>
                    {s.path || "-"}
                  </td>
                  <td className="svc-actions">
                    {!running(s.state_code) && (
                      <button
                        className="btn ghost small"
                        disabled={busyName === s.name || transitional(s.state_code)}
                        onClick={() => act(s, "start")}
                      >
                        {busyName === s.name ? "请求中…" : "启动"}
                      </button>
                    )}
                    {running(s.state_code) && (
                      <>
                        <button
                          className="btn ghost small danger-text"
                          disabled={busyName === s.name}
                          onClick={() => act(s, "stop")}
                        >
                          {busyName === s.name ? "请求中…" : "停止"}
                        </button>
                        <button
                          className="btn ghost small"
                          disabled={busyName === s.name}
                          onClick={() => act(s, "restart")}
                          title="停止并重新启动该服务"
                        >
                          {busyName === s.name ? "请求中…" : "重启"}
                        </button>
                      </>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
