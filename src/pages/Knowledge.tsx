import { useEffect, useMemo, useState } from "react";
import { FileText, Upload, Trash2, AlertCircle, Cpu, CheckCircle2, Loader2 } from "lucide-react";
import { api } from "../lib/api";
import { useApp } from "../lib/store";
import { formatBytes, isTauri } from "../lib/util";

export function Knowledge() {
  const { ragDocs, setRagDocs, installed, llama, setLlama, activeEmbeddingModelId, setActiveEmbeddingModelId } = useApp();
  const [ingesting, setIngesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [startingEmbed, setStartingEmbed] = useState(false);

  const embeddingModels = useMemo(
    () => installed.filter((m) => m.kind === "embedding" || /embed|bge-|nomic/i.test(m.filename)),
    [installed],
  );

  useEffect(() => {
    if (isTauri()) {
      api.ragList().then(setRagDocs).catch(console.error);
      api.llamaStatus().then(setLlama).catch(() => {});
    }
  }, [setRagDocs, setLlama]);

  async function startEmbedding(modelId: string) {
    setStartingEmbed(true);
    setError(null);
    try {
      setActiveEmbeddingModelId(modelId);
      const s = await api.startEmbeddingServer(modelId);
      setLlama(s);
    } catch (e: any) {
      setError(e.message ?? String(e));
    } finally {
      setStartingEmbed(false);
    }
  }

  async function pickAndIngest() {
    if (!isTauri()) {
      setError("Document upload is only available in the desktop app.");
      return;
    }
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const sel = await open({
        multiple: true,
        filters: [{ name: "Text", extensions: ["txt", "md", "markdown", "csv", "json", "log", "html", "xml"] }],
      });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      setIngesting(true);
      setError(null);
      for (const p of paths) {
        await api.ragIngest(p);
      }
      const docs = await api.ragList();
      setRagDocs(docs);
    } catch (e: any) {
      setError(e.message ?? String(e));
    } finally {
      setIngesting(false);
    }
  }

  async function remove(id: string) {
    if (!confirm("Remove this document and its embeddings?")) return;
    await api.ragDelete(id);
    const docs = await api.ragList();
    setRagDocs(docs);
  }

  return (
    <div className="flex flex-col h-full">
      <header className="px-5 py-4 border-b border-[var(--color-border-soft)]">
        <h1 className="font-semibold text-[17px]">Knowledge</h1>
        <p className="text-[var(--color-text-muted)] text-sm">
          Upload documents. LocalMind will search them to answer your questions.
        </p>
      </header>

      <div className="flex-1 overflow-y-auto px-5 py-4 max-w-3xl">
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-panel)] p-4 mb-5">
          <div className="flex items-center gap-2 mb-3">
            <Cpu size={15} className="text-[var(--color-text-muted)]" />
            <h2 className="font-semibold text-sm">Embedding model</h2>
            {llama.embeddingRunning && (
              <span className="ml-auto text-[11px] text-[var(--color-success)] flex items-center gap-1">
                <CheckCircle2 size={12} /> running
              </span>
            )}
          </div>
          <p className="text-xs text-[var(--color-text-muted)] mb-3">
            Turns documents into searchable vectors. Download a small one like <code className="text-[var(--color-text)] bg-[var(--color-panel-2)] px-1 rounded">nomic-embed-text</code> or <code className="text-[var(--color-text)] bg-[var(--color-panel-2)] px-1 rounded">bge-small-en</code> first.
          </p>
          {embeddingModels.length === 0 ? (
            <p className="text-sm text-[var(--color-text-subtle)]">No embedding models installed. Open the marketplace and search for <span className="text-[var(--color-text)]">nomic-embed</span>.</p>
          ) : (
            <div className="flex items-center gap-2">
              <select
                value={activeEmbeddingModelId ?? llama.embeddingModelId ?? ""}
                onChange={(e) => setActiveEmbeddingModelId(e.target.value)}
                className="flex-1 text-sm px-3 py-1.5 rounded-md bg-[var(--color-panel-2)] border border-[var(--color-border)] outline-none"
              >
                <option value="">Select embedding model…</option>
                {embeddingModels.map((m) => (
                  <option key={m.id} value={m.id}>{m.filename}</option>
                ))}
              </select>
              <button
                onClick={() => {
                  const id = activeEmbeddingModelId ?? embeddingModels[0].id;
                  if (id) startEmbedding(id);
                }}
                disabled={startingEmbed}
                className="text-sm px-3 py-1.5 rounded-md gradient-accent text-white disabled:opacity-50"
              >
                {startingEmbed ? <Loader2 size={14} className="animate-spin" /> : llama.embeddingRunning ? "Restart" : "Start"}
              </button>
            </div>
          )}
        </section>

        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-panel)] p-4">
          <div className="flex items-center justify-between mb-3">
            <h2 className="font-semibold text-sm flex items-center gap-2"><FileText size={15} /> Documents</h2>
            <button
              onClick={pickAndIngest}
              disabled={ingesting || !llama.embeddingRunning}
              className="text-sm px-3 py-1.5 rounded-md gradient-accent text-white flex items-center gap-1.5 disabled:opacity-40"
            >
              {ingesting ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />}
              Upload
            </button>
          </div>

          {!llama.embeddingRunning && (
            <div className="flex gap-2 text-xs bg-[var(--color-panel-2)] border border-[var(--color-border)] rounded-md p-3 mb-3">
              <AlertCircle size={14} className="text-[var(--color-text-muted)] shrink-0 mt-0.5" />
              <span className="text-[var(--color-text-muted)]">Start the embedding model above before uploading.</span>
            </div>
          )}

          {error && (
            <div className="text-xs text-[var(--color-danger)] mb-3">{error}</div>
          )}

          {ragDocs.length === 0 ? (
            <div className="text-center py-10 text-sm text-[var(--color-text-subtle)]">
              No documents yet. Supported: .txt, .md, .csv, .json, .html, .log
            </div>
          ) : (
            <div className="flex flex-col divide-y divide-[var(--color-border-soft)]">
              {ragDocs.map((d) => (
                <div key={d.id} className="flex items-center gap-3 py-2.5">
                  <FileText size={15} className="text-[var(--color-text-muted)] shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm truncate">{d.name}</div>
                    <div className="text-xs text-[var(--color-text-subtle)]">
                      {d.chunkCount} chunk{d.chunkCount === 1 ? "" : "s"} · {formatBytes(d.bytes)}
                    </div>
                  </div>
                  <button
                    onClick={() => remove(d.id)}
                    className="text-[var(--color-text-subtle)] hover:text-[var(--color-danger)] p-1"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>

        <p className="text-xs text-[var(--color-text-subtle)] mt-4">
          In any chat, click <span className="text-[var(--color-text-muted)]">Sources</span> to choose which documents that conversation can see.
        </p>
      </div>
    </div>
  );
}
