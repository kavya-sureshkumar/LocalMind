import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Conversation, HardwareInfo, InstalledModel, LlamaStatus, RagDocument, SdImage } from "./types";
import { uid } from "./util";

const activeChatRequests = new Map<string, AbortController>();

export function registerChatRequest(convId: string, controller: AbortController) {
  activeChatRequests.get(convId)?.abort();
  activeChatRequests.set(convId, controller);
}

export function unregisterChatRequest(convId: string, controller: AbortController) {
  if (activeChatRequests.get(convId) === controller) {
    activeChatRequests.delete(convId);
  }
}

function abortChatRequest(convId: string) {
  const c = activeChatRequests.get(convId);
  if (c) {
    c.abort();
    activeChatRequests.delete(convId);
  }
}

type View = "chat" | "marketplace" | "models" | "knowledge" | "image" | "settings";

interface AppState {
  view: View;
  setView: (v: View) => void;

  hardware: HardwareInfo | null;
  setHardware: (h: HardwareInfo) => void;

  installed: InstalledModel[];
  setInstalled: (m: InstalledModel[]) => void;

  llama: LlamaStatus;
  setLlama: (s: LlamaStatus) => void;

  activeModelId: string | null;
  setActiveModelId: (id: string | null) => void;

  activeMmprojId: string | null;
  setActiveMmprojId: (id: string | null) => void;

  activeEmbeddingModelId: string | null;
  setActiveEmbeddingModelId: (id: string | null) => void;

  lanUrl: string | null;
  setLanUrl: (u: string | null) => void;

  conversations: Conversation[];
  activeConvId: string | null;
  createConversation: (modelId: string | null) => string;
  setActiveConv: (id: string) => void;
  updateConversation: (id: string, patch: Partial<Conversation>) => void;
  renameConversation: (id: string, title: string) => void;
  deleteConversation: (id: string) => void;
  toggleRagDoc: (convId: string, docId: string) => void;

  ragDocs: RagDocument[];
  setRagDocs: (d: RagDocument[]) => void;

  activeSdModelId: string | null;
  setActiveSdModelId: (id: string | null) => void;

  sdImages: SdImage[];
  addSdImage: (img: SdImage) => void;
  deleteSdImage: (id: string) => void;

  downloads: Record<string, { percent: number; stage: string; downloaded: number; total: number }>;
  setDownload: (id: string, d: AppState["downloads"][string]) => void;
  clearDownload: (id: string) => void;

  engineProgress: { stage: string; message: string; percent: number } | null;
  setEngineProgress: (p: AppState["engineProgress"]) => void;

  // Set on the phone/PWA: the desktop's LAN URL plus a paired bearer token.
  // null on desktop (Tauri) — desktop talks to its own backend via invoke().
  connection: { url: string; token: string } | null;
  setConnection: (c: { url: string; token: string } | null) => void;

  // Synapse: distributed inference across LAN machines (Phase 1: manual).
  // `workers` is a list of `host:port` strings the host appends as `--rpc`.
  // `workerEnabled` reflects whether *this* machine is running rpc-server.
  synapse: { workerEnabled: boolean; workerPort: number; workers: string[] };
  setSynapseWorkerEnabled: (enabled: boolean) => void;
  setSynapseWorkerPort: (port: number) => void;
  setSynapseWorkers: (workers: string[]) => void;
}

const emptyLlama: LlamaStatus = {
  running: false,
  port: 8181,
  modelId: null,
  mmprojId: null,
  pid: null,
  embeddingRunning: false,
  embeddingPort: 8182,
  embeddingModelId: null,
};

export const useApp = create<AppState>()(
  persist(
    (set) => ({
      view: "chat",
      setView: (v) => set({ view: v }),

      hardware: null,
      setHardware: (h) => set({ hardware: h }),

      installed: [],
      setInstalled: (m) => set({ installed: m }),

      llama: emptyLlama,
      setLlama: (s) => set({ llama: s }),

      activeModelId: null,
      setActiveModelId: (id) => set({ activeModelId: id }),

      activeMmprojId: null,
      setActiveMmprojId: (id) => set({ activeMmprojId: id }),

      activeEmbeddingModelId: null,
      setActiveEmbeddingModelId: (id) => set({ activeEmbeddingModelId: id }),

      lanUrl: null,
      setLanUrl: (u) => set({ lanUrl: u }),

      conversations: [],
      activeConvId: null,
      createConversation: (modelId) => {
        const id = uid();
        const conv: Conversation = {
          id,
          title: "New chat",
          messages: [],
          modelId,
          createdAt: Date.now(),
          updatedAt: Date.now(),
          ragDocIds: [],
        };
        set((s) => ({ conversations: [conv, ...s.conversations], activeConvId: id }));
        return id;
      },
      setActiveConv: (id) => set({ activeConvId: id }),
      updateConversation: (id, patch) =>
        set((s) => ({
          conversations: s.conversations.map((c) =>
            c.id === id ? { ...c, ...patch, updatedAt: Date.now() } : c,
          ),
        })),
      renameConversation: (id, title) =>
        set((s) => ({
          conversations: s.conversations.map((c) => (c.id === id ? { ...c, title } : c)),
        })),
      deleteConversation: (id) => {
        abortChatRequest(id);
        set((s) => {
          const left = s.conversations.filter((c) => c.id !== id);
          return {
            conversations: left,
            activeConvId: s.activeConvId === id ? left[0]?.id ?? null : s.activeConvId,
          };
        });
      },
      toggleRagDoc: (convId, docId) =>
        set((s) => ({
          conversations: s.conversations.map((c) => {
            if (c.id !== convId) return c;
            const has = c.ragDocIds.includes(docId);
            return {
              ...c,
              ragDocIds: has ? c.ragDocIds.filter((d) => d !== docId) : [...c.ragDocIds, docId],
            };
          }),
        })),

      ragDocs: [],
      setRagDocs: (d) => set({ ragDocs: d }),

      activeSdModelId: null,
      setActiveSdModelId: (id) => set({ activeSdModelId: id }),

      sdImages: [],
      addSdImage: (img) => set((s) => ({ sdImages: [img, ...s.sdImages].slice(0, 60) })),
      deleteSdImage: (id) =>
        set((s) => ({ sdImages: s.sdImages.filter((i) => i.id !== id) })),

      downloads: {},
      setDownload: (id, d) => set((s) => ({ downloads: { ...s.downloads, [id]: d } })),
      clearDownload: (id) =>
        set((s) => {
          const { [id]: _, ...rest } = s.downloads;
          return { downloads: rest };
        }),

      engineProgress: null,
      setEngineProgress: (p) => set({ engineProgress: p }),

      connection: null,
      setConnection: (c) => set({ connection: c }),

      synapse: { workerEnabled: false, workerPort: 50052, workers: [] },
      setSynapseWorkerEnabled: (workerEnabled) =>
        set((s) => ({ synapse: { ...s.synapse, workerEnabled } })),
      setSynapseWorkerPort: (workerPort) =>
        set((s) => ({ synapse: { ...s.synapse, workerPort } })),
      setSynapseWorkers: (workers) =>
        set((s) => ({ synapse: { ...s.synapse, workers } })),
    }),
    {
      name: "localmind-store",
      partialize: (s) => ({
        conversations: s.conversations,
        activeConvId: s.activeConvId,
        activeModelId: s.activeModelId,
        activeMmprojId: s.activeMmprojId,
        activeEmbeddingModelId: s.activeEmbeddingModelId,
        activeSdModelId: s.activeSdModelId,
        sdImages: s.sdImages,
        connection: s.connection,
        synapse: s.synapse,
      }),
    },
  ),
);
