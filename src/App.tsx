import { useEffect } from "react";
import { api, listen } from "./lib/api";
import { useApp } from "./lib/store";
import { Sidebar } from "./components/Sidebar";
import { Chat } from "./pages/Chat";
import { Marketplace } from "./pages/Marketplace";
import { Models } from "./pages/Models";
import { Knowledge } from "./pages/Knowledge";
import { ImageGen } from "./pages/ImageGen";
import { Settings } from "./pages/Settings";

function App() {
  const { view, setHardware, setInstalled, setLanUrl, setLlama } = useApp();

  useEffect(() => {
    api.detectHardware().then(setHardware).catch(console.error);
    api.listInstalledModels().then(setInstalled).catch(() => {});
    api.llamaStatus().then(setLlama).catch(() => {});
    api.getLanUrl().then((u) => u && setLanUrl(u)).catch(() => {});

    const unlistenLan = listen<string>("lan:ready", (url) => setLanUrl(url));
    const unlistenReady = listen<{ port: number; modelId: string }>("llama:ready", () => {
      api.llamaStatus().then(setLlama).catch(() => {});
    });

    return () => {
      Promise.all([unlistenLan, unlistenReady]).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [setHardware, setInstalled, setLanUrl, setLlama]);

  return (
    <div className="h-screen w-screen flex">
      <Sidebar />
      <main className="flex-1 min-w-0 bg-[var(--color-bg)]">
        {view === "chat" && <Chat />}
        {view === "marketplace" && <Marketplace />}
        {view === "models" && <Models />}
        {view === "knowledge" && <Knowledge />}
        {view === "image" && <ImageGen />}
        {view === "settings" && <Settings />}
      </main>
    </div>
  );
}

export default App;
