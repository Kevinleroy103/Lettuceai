import { Sparkles } from "lucide-react";
import type { CompanionStateNode } from "../../../../../core/storage/chatWidgetSchemas";
import { cn } from "../../../../design-tokens";
import { useWidgetContext } from "./WidgetContext";
import { widgetCardClass } from "./widgetSurface";

const RELATIONSHIP_METERS: { key: string; label: string }[] = [
  { key: "closeness", label: "Closeness" },
  { key: "trust", label: "Trust" },
  { key: "affection", label: "Affection" },
  { key: "tension", label: "Tension" },
  { key: "stability", label: "Stability" },
];

function Meter({ label, value }: { label: string; value: number }) {
  const pct = Math.round(Math.max(0, Math.min(1, value)) * 100);
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between text-[11px]">
        <span className="text-fg/55">{label}</span>
        <span className="tabular-nums text-fg/45">{pct}%</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-fg/10">
        <div className="h-full rounded-full bg-accent/70" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

export function WidgetCompanionState({ node }: { node: CompanionStateNode }) {
  const { character, session, hasBackground } = useWidgetContext();
  const isCompanion = character?.mode === "companion";
  const relationship = session?.companionState?.relationshipState;

  return (
    <section
      className={cn(
        "flex flex-col gap-2.5 rounded-xl px-3 py-3",
        widgetCardClass(hasBackground, node.design),
      )}
    >
      <header className="flex items-center gap-2">
        <Sparkles size={14} className="text-fg/50" />
        <h3 className="text-sm font-semibold text-fg/75">
          {node.title || "Companion"}
        </h3>
      </header>
      {!isCompanion ? (
        <p className="text-[12px] italic text-fg/40">
          Only available for companion characters.
        </p>
      ) : !relationship ? (
        <p className="text-[12px] italic text-fg/40">No relationship data yet.</p>
      ) : (
        <div className="flex flex-col gap-2">
          {RELATIONSHIP_METERS.map((m) => (
            <Meter
              key={m.key}
              label={m.label}
              value={(relationship as Record<string, number>)[m.key] ?? 0}
            />
          ))}
        </div>
      )}
    </section>
  );
}
