import { useEffect, useState } from "react";

import { systemOverview } from "../api.ts";
import { formatSize } from "../utils.ts";

// 性能趋势面板：1s 采样一次系统 CPU/内存（复用 system_overview，无独立后端命令），
// 环形缓冲保留最近 120 点（约 2 分钟），SVG 折线绘制双曲线。
const MAX = 120;
const W = 600;
const H = 160;

export default function PerfPanel() {
  const [cpu, setCpu] = useState(0);
  const [memPct, setMemPct] = useState(0);
  const [memUsed, setMemUsed] = useState(0);
  const [memTotal, setMemTotal] = useState(0);
  const [hist, setHist] = useState<{ cpu: number; mem: number }[]>([]);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const o = await systemOverview();
        if (cancelled) return;
        const m = o.mem_total > 0 ? (o.mem_used / o.mem_total) * 100 : 0;
        setCpu(o.cpu);
        setMemPct(m);
        setMemUsed(o.mem_used);
        setMemTotal(o.mem_total);
        setHist((h) => [...h, { cpu: o.cpu, mem: m }].slice(-MAX));
      } catch {
        /* 采样失败静默，下个周期重试 */
      }
    };
    tick();
    const t = setInterval(tick, 1000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, []);

  const x = (i: number) => (i / (MAX - 1)) * W;
  const y = (v: number) => H - (Math.min(100, Math.max(0, v)) / 100) * H;
  const cpuPts = hist.map((p, i) => `${x(i).toFixed(1)},${y(p.cpu).toFixed(1)}`).join(" ");
  const memPts = hist.map((p, i) => `${x(i).toFixed(1)},${y(p.mem).toFixed(1)}`).join(" ");

  return (
    <div className="view perf-view">
      <div className="perf-readout">
        <span className="perf-item">
          <i className="dot cpu" />
          CPU <b>{cpu.toFixed(1)}%</b>
        </span>
        <span className="perf-item">
          <i className="dot mem" />
          内存 <b>{memPct.toFixed(1)}%</b>
          <span className="dim">
            （{formatSize(memUsed)} / {formatSize(memTotal)}）
          </span>
        </span>
      </div>
      <div className="perf-chart">
        <span className="perf-y top">100%</span>
        <span className="perf-y mid">50%</span>
        <span className="perf-y zero">0%</span>
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none">
          {[0, 0.25, 0.5, 0.75, 1].map((g) => (
            <line key={g} x1="0" x2={W} y1={g * H} y2={g * H} className="gridline" />
          ))}
          <polyline points={memPts} className="perf-line mem" />
          <polyline points={cpuPts} className="perf-line cpu" />
        </svg>
      </div>
      <div className="perf-foot">
        <span className="dim">最近 {Math.min(hist.length, MAX)} 秒</span>
        <span className="dim">
          峰值 CPU {hist.length ? Math.max(...hist.map((p) => p.cpu)).toFixed(1) : 0}% · 内存{" "}
          {hist.length ? Math.max(...hist.map((p) => p.mem)).toFixed(1) : 0}%
        </span>
      </div>
    </div>
  );
}
