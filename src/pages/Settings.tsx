import { useEffect, useState } from "react";
import { Cpu, Wifi, Smartphone, Copy, Check } from "lucide-react";
import { useApp } from "../lib/store";
import { api } from "../lib/api";
import type { BinaryProgress } from "../lib/types";
import { listen } from "../lib/api";

export function Settings() {
  const { hardware, lanUrl } = useApp();
  const [engineStatus, setEngineStatus] = useState<string>("not checked");
  const [engineProgress, setEngineProgress] = useState<BinaryProgress | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    listen<BinaryProgress>("binary:progress", (p) => setEngineProgress(p));
  }, []);

  async function installEngine() {
    setEngineStatus("installing");
    try {
      await api.ensureEngine();
      setEngineStatus("ready");
    } catch (e: any) {
      setEngineStatus("error: " + (e?.message ?? (typeof e === "string" ? e : JSON.stringify(e))));
    }
  }

  function copyLan() {
    if (!lanUrl) return;
    navigator.clipboard.writeText(lanUrl);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="flex flex-col h-full">
      <header className="px-5 py-4 border-b border-[var(--color-border-soft)]">
        <h1 className="font-semibold text-[17px]">Settings</h1>
        <p className="text-[var(--color-text-muted)] text-sm">Hardware, engine, and network access.</p>
      </header>

      <div className="flex-1 overflow-y-auto px-5 py-5 max-w-3xl">
        <Section title="Hardware" icon={<Cpu size={15} />}>
          {hardware ? (
            <div className="grid grid-cols-2 gap-y-2 text-sm">
              <Row k="Operating system" v={`${hardware.os} (${hardware.arch})`} />
              <Row k="CPU" v={`${hardware.cpuName.trim()} · ${hardware.cpuCores} cores`} />
              <Row k="Memory" v={`${hardware.totalMemoryGb.toFixed(1)} GB`} />
              <Row k="Accelerator" v={describeAcc(hardware.accelerator)} />
              <Row k="Recommended backend" v={hardware.recommendedBackend} />
              <Row k="GPU layers" v={hardware.recommendedNGpuLayers === -1 ? "all (offload)" : String(hardware.recommendedNGpuLayers)} />
            </div>
          ) : (
            <p className="text-[var(--color-text-muted)] text-sm">Detecting…</p>
          )}
        </Section>

        <Section title="Inference engine (llama.cpp)" icon={<Cpu size={15} />}>
          <p className="text-sm text-[var(--color-text-muted)] mb-3">
            LocalMind downloads a prebuilt llama.cpp for your hardware. This happens automatically the first time you load a model.
          </p>
          <button
            onClick={installEngine}
            className="text-sm px-3 py-1.5 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] hover:border-[var(--color-accent)]/60"
          >
            Reinstall engine
          </button>
          {engineProgress && (
            <div className="mt-3 text-xs text-[var(--color-text-muted)]">
              {engineProgress.message} {engineProgress.total > 0 && `· ${Math.floor((engineProgress.downloaded / engineProgress.total) * 100)}%`}
            </div>
          )}
          <div className="text-xs text-[var(--color-text-subtle)] mt-2">Status: {engineStatus}</div>
        </Section>

        <Section title="Access from other devices" icon={<Wifi size={15} />}>
          <p className="text-sm text-[var(--color-text-muted)] mb-3">
            Open this URL on your phone or tablet while on the same Wi-Fi network.
          </p>
          {lanUrl ? (
            <div className="flex items-center gap-2 rounded-md border border-[var(--color-border)] bg-[var(--color-panel)] px-3 py-2">
              <Smartphone size={14} className="text-[var(--color-text-muted)]" />
              <code className="flex-1 text-sm font-mono">{lanUrl}</code>
              <button
                onClick={copyLan}
                className="text-[var(--color-text-muted)] hover:text-[var(--color-text)]"
              >
                {copied ? <Check size={14} className="text-[var(--color-success)]" /> : <Copy size={14} />}
              </button>
            </div>
          ) : (
            <p className="text-sm text-[var(--color-text-muted)]">Starting LAN server…</p>
          )}
        </Section>
      </div>
    </div>
  );
}

function Section({ title, icon, children }: { title: string; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="mb-6 rounded-lg border border-[var(--color-border)] bg-[var(--color-panel)] p-4">
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[var(--color-text-muted)]">{icon}</span>
        <h2 className="font-semibold text-sm">{title}</h2>
      </div>
      {children}
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <>
      <div className="text-[var(--color-text-muted)]">{k}</div>
      <div className="text-right">{v}</div>
    </>
  );
}

function describeAcc(a: any): string {
  switch (a.type) {
    case "appleSilicon": return `${a.chip} (Apple Silicon, Metal)`;
    case "nvidia": return `${a.name} · ${a.vramGb.toFixed(1)}GB VRAM (CUDA)`;
    case "amd": return `${a.name} (AMD, Vulkan)`;
    case "intelArc": return `${a.name} (Intel Arc)`;
    case "cpu": return "CPU only";
    default: return "Unknown";
  }
}
