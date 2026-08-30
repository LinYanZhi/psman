import { useEffect, useRef } from "react";

// 应用内确认弹窗（替代 window.confirm，样式跟随主题）。
// 用于结束进程等危险操作：Enter 确认、Esc 取消，点遮罩取消。
export default function ConfirmDialog({
  title,
  message,
  confirmText = "确认",
  danger,
  busy,
  onConfirm,
  onCancel,
}: {
  title: string;
  message: string;
  confirmText?: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const okRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    okRef.current?.focus();
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onCancel]);

  return (
    <div className="modal-overlay" onClick={busy ? undefined : onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">{title}</div>
        <div className="modal-msg">{message}</div>
        <div className="modal-actions">
          <button className="btn ghost" disabled={busy} onClick={onCancel}>
            取消
          </button>
          <button
            ref={okRef}
            className={"btn" + (danger ? " danger" : "")}
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? "处理中…" : confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}
