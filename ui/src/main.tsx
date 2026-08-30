import { createRoot } from "react-dom/client";

import App from "./App.tsx";
import "./styles.css";

// 注意：不用 StrictMode —— dockview 的 onReady 会重建布局，
// StrictMode 双挂载会导致面板重复创建
createRoot(document.getElementById("root")!).render(<App />);
