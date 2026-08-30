import { useEffect, useState } from "react";

import { processIcon } from "../api.ts";

// 模块级缓存：同一 exe 路径只查询一次，刷新列表不重复 invoke
const cache = new Map<string, string>();

/** 进程小图标：按 exe 路径懒加载（data:image/x-icon），失败/空路径不渲染 */
export default function ProcessIcon({ path }: { path: string }) {
  const [src, setSrc] = useState<string | null>(() => cache.get(path) ?? null);

  useEffect(() => {
    if (!path || src) return;
    let cancelled = false;
    processIcon(path)
      .then((s) => {
        if (!cancelled && s) {
          cache.set(path, s);
          setSrc(s);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [path]);

  if (!src) return null;
  return (
    <img
      className="proc-icon"
      src={src}
      alt=""
      width={16}
      height={16}
      draggable={false}
    />
  );
}
