export type AcceleratorType = "appleSilicon" | "nvidia" | "amd" | "intelArc" | "cpu";

export type Accelerator =
  | { type: "appleSilicon"; chip: string; unifiedMemoryGb: number }
  | { type: "nvidia"; name: string; vramGb: number; cudaVersion: string | null }
  | { type: "amd"; name: string; vramGb: number }
  | { type: "intelArc"; name: string }
  | { type: "cpu" };

export interface HardwareInfo {
  os: string;
  arch: string;
  cpuName: string;
  cpuCores: number;
  totalMemoryGb: number;
  accelerator: Accelerator;
  recommendedBackend: string;
  recommendedNGpuLayers: number;
}

export interface ModelFile {
  filename: string;
  sizeBytes: number;
  quantization: string;
  downloadUrl: string;
}

export interface ModelListing {
  id: string;
  name: string;
  author: string;
  downloads: number;
  likes: number;
  tags: string[];
  updated: string | null;
  description: string | null;
  files: ModelFile[];
}

export type ModelKind = "llm" | "vision" | "mmproj" | "embedding" | "whisper" | "sd";

export interface InstalledModel {
  id: string;
  filename: string;
  repo: string;
  sizeBytes: number;
  path: string;
  kind: ModelKind;
}

export interface LlamaStatus {
  running: boolean;
  port: number;
  modelId: string | null;
  mmprojId: string | null;
  pid: number | null;
  embeddingRunning: boolean;
  embeddingPort: number;
  embeddingModelId: string | null;
}

export interface LlamaSettings {
  modelId: string;
  contextSize?: number;
  nGpuLayers?: number;
  threads?: number;
  port?: number;
  mmprojId?: string;
  flashAttn?: boolean;
  /** Synapse: comma-separated `host:port` workers to pipeline-shard layers across. */
  synapseWorkers?: string[];
}

export interface SynapseWorkerStatus {
  running: boolean;
  port: number;
  pid: number | null;
}

export interface ModelDownloadProgress {
  id: string;
  downloaded: number;
  total: number;
  percent: number;
  stage: string;
}

export interface BinaryProgress {
  stage: string;
  downloaded: number;
  total: number;
  message: string;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  createdAt: number;
  pending?: boolean;
  images?: string[]; // base64 data URLs
  sources?: RetrievedChunk[];
}

export interface Conversation {
  id: string;
  title: string;
  messages: ChatMessage[];
  modelId: string | null;
  createdAt: number;
  updatedAt: number;
  ragDocIds: string[]; // documents enabled as context in this conversation
}

export interface RagDocument {
  id: string;
  name: string;
  sourcePath: string | null;
  createdAt: number;
  chunkCount: number;
  bytes: number;
}

export interface RagChunk {
  id: string;
  docId: string;
  docName: string;
  content: string;
  ordinal: number;
}

export interface RetrievedChunk {
  chunk: RagChunk;
  score: number;
}

export interface SdRequest {
  modelId: string;
  prompt: string;
  negativePrompt?: string;
  width?: number;
  height?: number;
  steps?: number;
  cfgScale?: number;
  seed?: number;
  sampler?: string;
}

export interface SdImage {
  id: string;
  path: string;
  prompt: string;
  modelId: string;
  width: number;
  height: number;
  seed: number;
  createdAt: number;
}

export interface SdProgress {
  id: string;
  stage: string;
  step: number;
  total: number;
  message: string;
}
