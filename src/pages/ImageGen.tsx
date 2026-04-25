import { useEffect, useMemo, useRef, useState } from "react";
import { Image as ImageIcon, Sparkles, Trash2, Download, Loader2, AlertCircle } from "lucide-react";
import { api, listen } from "../lib/api";
import { useApp } from "../lib/store";
import { isTauri } from "../lib/util";
import type { SdImage, SdProgress } from "../lib/types";

function imageUrl(img: SdImage): string {
  const filename = `${img.id}.png`;
  if (isTauri()) return `http://127.0.0.1:3939/sd-images/${filename}`;
  return `/sd-images/${filename}`;
}

export function ImageGen() {
  const { installed, activeSdModelId, setActiveSdModelId, sdImages, addSdImage, deleteSdImage } = useApp();

  const [prompt, setPrompt] = useState("");
  const [negative, setNegative] = useState("");
  const [width, setWidth] = useState(512);
  const [height, setHeight] = useState(512);
  const [steps, setSteps] = useState(20);
  const [cfg, setCfg] = useState(7);
  const [seed, setSeed] = useState<number | "">("");
  const [sampler, setSampler] = useState("euler_a");

  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<SdProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const sdModels = useMemo(
    () => installed.filter((m) => m.kind === "sd" || /sd|stable|flux|sdxl|diffus/i.test(m.filename)),
    [installed],
  );

  const unlistenRef = useRef<null | (() => void)>(null);

  useEffect(() => {
    let active = true;
    listen<SdProgress>("sd:progress", (p) => {
      if (active) setProgress(p);
    }).then((fn) => {
      if (active) unlistenRef.current = fn;
      else fn();
    });
    return () => {
      active = false;
      unlistenRef.current?.();
    };
  }, []);

  async function generate() {
    if (!prompt.trim()) {
      setError("Enter a prompt first.");
      return;
    }
    if (!activeSdModelId) {
      setError("Pick a model first.");
      return;
    }
    setError(null);
    setBusy(true);
    setProgress(null);
    try {
      const img = await api.sdGenerate({
        modelId: activeSdModelId,
        prompt: prompt.trim(),
        negativePrompt: negative.trim() || undefined,
        width,
        height,
        steps,
        cfgScale: cfg,
        seed: seed === "" ? -1 : Number(seed),
        sampler,
      });
      addSdImage(img);
    } catch (e: any) {
      setError(e.message ?? String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  async function ensureBinary() {
    setError(null);
    try {
      await api.ensureSd();
    } catch (e: any) {
      setError(e.message ?? String(e));
    }
  }

  const pct = progress ? Math.round((progress.step / Math.max(1, progress.total)) * 100) : 0;

  return (
    <div className="flex flex-col h-full">
      <header className="px-5 py-4 border-b border-[var(--color-border-soft)]">
        <h1 className="font-semibold text-[17px]">Images</h1>
        <p className="text-[var(--color-text-muted)] text-sm">
          Generate images locally with Stable Diffusion.
        </p>
      </header>

      <div className="flex-1 overflow-y-auto px-5 py-4 grid gap-5 md:grid-cols-[380px_minmax(0,1fr)]">
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-panel)] p-4 flex flex-col gap-3 h-fit">
          <label className="text-xs text-[var(--color-text-muted)]">Model</label>
          {sdModels.length === 0 ? (
            <div className="text-xs text-[var(--color-text-subtle)] bg-[var(--color-panel-2)] rounded-md p-2.5">
              No diffusion models installed. Search the marketplace for <span className="text-[var(--color-text)]">SDXL</span>, <span className="text-[var(--color-text)]">FLUX</span>, or <span className="text-[var(--color-text)]">SD 1.5</span> GGUF builds.
            </div>
          ) : (
            <select
              value={activeSdModelId ?? ""}
              onChange={(e) => setActiveSdModelId(e.target.value || null)}
              className="text-sm px-3 py-1.5 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none"
            >
              <option value="">Select model…</option>
              {sdModels.map((m) => (
                <option key={m.id} value={m.id}>{m.filename}</option>
              ))}
            </select>
          )}

          <label className="text-xs text-[var(--color-text-muted)] mt-1">Prompt</label>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={4}
            placeholder="A watercolor painting of a misty mountain lake at dawn"
            className="text-sm px-3 py-2 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none resize-none"
          />

          <label className="text-xs text-[var(--color-text-muted)]">Negative prompt (optional)</label>
          <textarea
            value={negative}
            onChange={(e) => setNegative(e.target.value)}
            rows={2}
            placeholder="blurry, low quality, watermark"
            className="text-sm px-3 py-2 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none resize-none"
          />

          <div className="grid grid-cols-2 gap-2">
            <NumField label="Width" value={width} onChange={setWidth} step={64} min={64} max={2048} />
            <NumField label="Height" value={height} onChange={setHeight} step={64} min={64} max={2048} />
            <NumField label="Steps" value={steps} onChange={setSteps} step={1} min={1} max={150} />
            <NumField label="CFG" value={cfg} onChange={setCfg} step={0.5} min={1} max={20} />
          </div>

          <div className="grid grid-cols-2 gap-2">
            <label className="flex flex-col gap-1">
              <span className="text-[11px] text-[var(--color-text-muted)]">Seed (empty = random)</span>
              <input
                type="number"
                value={seed}
                onChange={(e) => setSeed(e.target.value === "" ? "" : Number(e.target.value))}
                className="text-sm px-2 py-1.5 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none"
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-[11px] text-[var(--color-text-muted)]">Sampler</span>
              <select
                value={sampler}
                onChange={(e) => setSampler(e.target.value)}
                className="text-sm px-2 py-1.5 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none"
              >
                <option value="euler_a">euler_a</option>
                <option value="euler">euler</option>
                <option value="heun">heun</option>
                <option value="dpm2">dpm2</option>
                <option value="dpm++2m">dpm++2m</option>
                <option value="dpm++2s_a">dpm++2s_a</option>
                <option value="lcm">lcm</option>
              </select>
            </label>
          </div>

          <button
            onClick={generate}
            disabled={busy || !activeSdModelId || !prompt.trim()}
            className="mt-1 text-sm px-3 py-2 rounded-md gradient-accent text-white font-medium flex items-center justify-center gap-1.5 disabled:opacity-40"
          >
            {busy ? <Loader2 size={14} className="animate-spin" /> : <Sparkles size={14} />}
            {busy ? "Generating…" : "Generate"}
          </button>

          {busy && progress && (
            <div className="text-[11px] text-[var(--color-text-muted)]">
              <div className="h-1 bg-[var(--color-panel-2)] rounded overflow-hidden mb-1">
                <div className="h-full gradient-accent" style={{ width: `${pct}%` }} />
              </div>
              {progress.stage === "sampling" ? `Step ${progress.step}/${progress.total}` : progress.message || progress.stage}
            </div>
          )}

          <button
            onClick={ensureBinary}
            className="text-[11px] text-[var(--color-text-subtle)] hover:text-[var(--color-text-muted)] underline underline-offset-2 self-start"
          >
            Download stable-diffusion.cpp engine
          </button>

          {error && (
            <div className="flex gap-1.5 text-xs text-[var(--color-danger)] bg-[var(--color-panel-2)] rounded-md p-2">
              <AlertCircle size={13} className="shrink-0 mt-0.5" />
              <span>{error}</span>
            </div>
          )}
        </section>

        <section>
          {sdImages.length === 0 ? (
            <div className="border border-dashed border-[var(--color-border)] rounded-lg h-64 grid place-items-center text-sm text-[var(--color-text-subtle)]">
              <div className="flex flex-col items-center gap-2">
                <ImageIcon size={28} className="text-[var(--color-text-subtle)]" />
                <span>No images yet. Write a prompt and click Generate.</span>
              </div>
            </div>
          ) : (
            <div className="grid grid-cols-2 lg:grid-cols-3 gap-3">
              {sdImages.map((img) => (
                <div key={img.id} className="group relative rounded-lg overflow-hidden border border-[var(--color-border)] bg-[var(--color-panel)]">
                  <img src={imageUrl(img)} alt={img.prompt} className="w-full aspect-square object-cover" />
                  <div className="absolute inset-x-0 bottom-0 p-2 bg-gradient-to-t from-black/70 to-transparent opacity-0 group-hover:opacity-100 transition-opacity">
                    <p className="text-[11px] text-white line-clamp-2">{img.prompt}</p>
                    <div className="flex items-center gap-1.5 mt-1">
                      <a
                        href={imageUrl(img)}
                        download={`${img.id}.png`}
                        className="text-[10px] text-white/90 bg-white/10 hover:bg-white/20 rounded px-1.5 py-0.5 flex items-center gap-1"
                      >
                        <Download size={10} /> save
                      </a>
                      <button
                        onClick={() => deleteSdImage(img.id)}
                        className="text-[10px] text-white/90 bg-white/10 hover:bg-[var(--color-danger)]/70 rounded px-1.5 py-0.5 flex items-center gap-1"
                      >
                        <Trash2 size={10} /> remove
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

function NumField({
  label, value, onChange, step, min, max,
}: { label: string; value: number; onChange: (n: number) => void; step: number; min: number; max: number }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] text-[var(--color-text-muted)]">{label}</span>
      <input
        type="number"
        value={value}
        step={step}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.target.value))}
        className="text-sm px-2 py-1.5 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none"
      />
    </label>
  );
}
