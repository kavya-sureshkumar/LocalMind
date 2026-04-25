import { Cpu, Zap } from "lucide-react";
import type { HardwareInfo } from "../lib/types";

export function HardwareBadge({ hw }: { hw: HardwareInfo | null }) {
  if (!hw) return <span className="text-xs text-[var(--color-text-subtle)]">Detecting…</span>;
  const label = describe(hw);
  const icon = hw.accelerator.type === "cpu" ? <Cpu size={12} /> : <Zap size={12} />;
  return (
    <div className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] text-xs text-[var(--color-text-muted)]">
      {icon}
      <span>{label}</span>
    </div>
  );
}

function describe(hw: HardwareInfo) {
  const a = hw.accelerator;
  switch (a.type) {
    case "appleSilicon": return `${a.chip} · Metal · ${a.unifiedMemoryGb.toFixed(0)}GB`;
    case "nvidia":       return `${a.name} · CUDA · ${a.vramGb.toFixed(0)}GB`;
    case "amd":          return `${a.name} · Vulkan`;
    case "intelArc":     return `${a.name} · Vulkan`;
    case "cpu":          return `${hw.cpuName.split("@")[0].trim()} · CPU`;
  }
}
