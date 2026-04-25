import { MessageSquare, Store, Boxes, BookOpen, Image as ImageIcon, Settings as SettingsIcon, Plus, Trash2 } from "lucide-react";
import { useApp } from "../lib/store";
import { HardwareBadge } from "./HardwareBadge";
import { cn } from "../lib/util";

export function Sidebar() {
  const {
    view, setView, hardware, conversations, activeConvId, setActiveConv,
    createConversation, deleteConversation, activeModelId, llama,
  } = useApp();

  return (
    <aside className="w-[260px] shrink-0 h-full flex flex-col border-r border-[var(--color-border-soft)] bg-[var(--color-panel)]">
      <div className="px-4 pt-4 pb-3 flex items-center gap-2">
        <div className="w-8 h-8 rounded-lg gradient-accent grid place-items-center">
          <span className="text-white font-bold text-sm">L</span>
        </div>
        <div className="flex flex-col">
          <span className="font-semibold text-sm leading-tight">LocalMind</span>
          <span className="text-[10px] text-[var(--color-text-subtle)] leading-tight">v0.1 · local only</span>
        </div>
      </div>

      <nav className="px-2 py-1 flex flex-col gap-0.5">
        <NavItem icon={<MessageSquare size={15} />} label="Chat" active={view === "chat"} onClick={() => setView("chat")} />
        <NavItem icon={<Store size={15} />} label="Marketplace" active={view === "marketplace"} onClick={() => setView("marketplace")} />
        <NavItem icon={<Boxes size={15} />} label="My models" active={view === "models"} onClick={() => setView("models")} />
        <NavItem icon={<BookOpen size={15} />} label="Knowledge" active={view === "knowledge"} onClick={() => setView("knowledge")} />
        <NavItem icon={<ImageIcon size={15} />} label="Images" active={view === "image"} onClick={() => setView("image")} />
        <NavItem icon={<SettingsIcon size={15} />} label="Settings" active={view === "settings"} onClick={() => setView("settings")} />
      </nav>

      {view === "chat" && (
        <>
          <div className="px-3 mt-3 mb-1 flex items-center justify-between">
            <span className="text-[11px] uppercase tracking-wider text-[var(--color-text-subtle)]">Conversations</span>
            <button
              onClick={() => createConversation(activeModelId)}
              className="text-[var(--color-text-muted)] hover:text-[var(--color-text)] transition-colors"
              title="New chat"
            >
              <Plus size={14} />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto px-2 pb-2">
            {conversations.length === 0 && (
              <div className="text-xs text-[var(--color-text-subtle)] px-2 py-3">No conversations yet.</div>
            )}
            {conversations.map((c) => (
              <div
                key={c.id}
                onClick={() => setActiveConv(c.id)}
                className={cn(
                  "group flex items-center justify-between px-2 py-1.5 rounded-md text-sm cursor-pointer transition-colors",
                  c.id === activeConvId
                    ? "bg-[var(--color-panel-2)] text-[var(--color-text)]"
                    : "text-[var(--color-text-muted)] hover:bg-[var(--color-panel-2)]/60",
                )}
              >
                <span className="truncate">{c.title}</span>
                <button
                  onClick={(e) => { e.stopPropagation(); deleteConversation(c.id); }}
                  className="opacity-0 group-hover:opacity-100 text-[var(--color-text-subtle)] hover:text-[var(--color-danger)]"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
        </>
      )}

      {view !== "chat" && <div className="flex-1" />}

      <div className="px-3 py-3 border-t border-[var(--color-border-soft)] flex flex-col gap-2">
        <HardwareBadge hw={hardware} />
        <div className="flex items-center gap-1.5 text-[11px] text-[var(--color-text-muted)]">
          <span className={cn("w-1.5 h-1.5 rounded-full", llama.running ? "bg-[var(--color-success)]" : "bg-[var(--color-text-subtle)]")} />
          <span className="truncate">
            {llama.running ? `Running: ${llama.modelId ?? "model"}` : "Idle"}
          </span>
        </div>
      </div>
    </aside>
  );
}

function NavItem({
  icon, label, active, onClick,
}: { icon: React.ReactNode; label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-2 px-2.5 py-1.5 rounded-md text-sm transition-colors",
        active
          ? "bg-[var(--color-panel-2)] text-[var(--color-text)]"
          : "text-[var(--color-text-muted)] hover:bg-[var(--color-panel-2)]/60 hover:text-[var(--color-text)]",
      )}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
