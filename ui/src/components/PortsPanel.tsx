import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { queryPorts } from "../api.ts";
import type { PortInfo } from "../api.ts";
import { refreshAll, selectPid, useAppState } from "../store.ts";

// 端口表面板：全系统端口占用表（后端 query_ports 命令），支持过滤、点击跳转详情。
type SortKey = "port" | "proto" | "state" | "pid" | "name" | "remote";
type Sort = { key: SortKey; dir: "asc" | "desc" } | null;

export default function PortsPanel() {
  const { refreshKey, selectedPid } = useAppState();
  const [rows, setRows] = useState<PortInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  // 默认按端口升序
  const [sort, setSort] = useState<Sort>({ key: "port", dir: "asc" });
  // 轮询防重入 + 首帧 loading 控制（之后静默更新不闪屏）
  const inflight = useRef(false);
  const firstLoad = useRef(true);

  const load = useCallback(async () => {
    if (inflight.current) return;
    inflight.current = true;
    try {
      setRows(await queryPorts());
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

  // 首载 + 手动/跨窗口刷新（refreshKey）→ 静默更新
  useEffect(() => {
    load();
  }, [refreshKey, load]);

  // 自动轮询（3s，静默更新）
  useEffect(() => {
    const t = setInterval(load, 3000);
    return () => clearInterval(t);
  }, [load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    let base = rows;
    if (q) {
      base = rows.filter(
        (r) =>
          String(r.port).includes(q) ||
          String(r.pid).includes(q) ||
          r.name.toLowerCase().includes(q) ||
          r.remote.toLowerCase().includes(q),
      );
    }
    if (!sort) return base;
    const { key, dir } = sort;
    const mul = dir === "asc" ? 1 : -1;
    return [...base].sort((a, b) => {
      let r: number;
      switch (key) {
        case "port":
          r = a.port - b.port;
          break;
        case "pid":
          r = a.pid - b.pid;
          break;
        case "remote":
          r = a.remote.localeCompare(b.remote, "zh");
          break;
        default:
          r = String(a[key]).localeCompare(String(b[key]), "zh");
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

  const sortInd = (k: SortKey) => (sort?.key === k ? (sort.dir === "asc" ? " ▲" : " ▼") : "");

  return (
    <div className="view">
      <div className="view-toolbar">
        <input
          className="search"
          placeholder="搜索端口 / PID / 进程名 / 远程地址"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="count">{filtered.length} 个端口</span>
        {notice && <span className="notice">{notice}</span>}
        <button className="btn ghost" onClick={() => refreshAll()}>
          刷新
        </button>
      </div>

      <div className="table-wrap">
        <table className="grid ports-grid">
          <thead>
            <tr>
              <th className="num sortable" onClick={() => onSort("port")}>
                端口{sortInd("port")}
              </th>
              <th className="sortable" onClick={() => onSort("proto")}>
                协议{sortInd("proto")}
              </th>
              <th className="sortable" onClick={() => onSort("state")}>
                状态{sortInd("state")}
              </th>
              <th className="num sortable" onClick={() => onSort("pid")}>
                PID{sortInd("pid")}
              </th>
              <th className="sortable" onClick={() => onSort("name")}>
                进程名{sortInd("name")}
              </th>
              <th className="sortable" onClick={() => onSort("remote")}>
                远程地址{sortInd("remote")}
              </th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={6} className="empty">
                  加载中…
                </td>
              </tr>
            ) : filtered.length === 0 ? (
              <tr>
                <td colSpan={6} className="empty">
                  没有匹配的端口
                </td>
              </tr>
            ) : (
              filtered.map((r, i) => (
                <tr
                  key={`${r.port}-${r.pid}-${i}`}
                  className={r.pid === selectedPid ? "row selected" : "row"}
                  onClick={() => selectPid(r.pid)}
                  title="点击在进程详情中查看"
                >
                  <td className="num port">{r.port}</td>
                  <td className="mono">{r.proto}</td>
                  <td>
                    <span className={"conn-state" + (r.state === "监听" ? " ok" : "")}>
                      {r.state}
                    </span>
                  </td>
                  <td className="num">{r.pid}</td>
                  <td className="name">{r.name || "-"}</td>
                  <td className="mono remote">{r.remote || "-"}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
