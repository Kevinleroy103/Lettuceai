import type { ReactNode } from "react";
import type { ChatWidgetLayout } from "../utils/chatWidgetLayout";

interface WidgetAreaPlaceholderProps {
  side: "left" | "right";
}

function WidgetAreaPlaceholder({ side }: WidgetAreaPlaceholderProps) {
  return (
    <aside
      className="relative z-10 flex flex-1 basis-0 flex-col items-center justify-center self-stretch overflow-y-auto bg-fg/3 px-2 py-6 text-center text-xs text-fg/40"
      style={{ minWidth: 0 }}
    >
      <span className="text-[10px] uppercase tracking-[0.25em] text-fg/30">
        Widget area
      </span>
      <span className="mt-1 text-fg/45">{side === "left" ? "Left" : "Right"}</span>
    </aside>
  );
}

interface ChatWidgetAreaProps {
  widgetLayout: ChatWidgetLayout;
  children: ReactNode;
}

export function ChatWidgetArea({ widgetLayout, children }: ChatWidgetAreaProps) {
  if (!widgetLayout.enabled || widgetLayout.columnPx == null) {
    return <>{children}</>;
  }
  return (
    <div className="relative z-10 flex min-h-0 flex-1 flex-row">
      {widgetLayout.showLeft && (
        <WidgetAreaPlaceholder side="left" />
      )}
      <div
        className="flex shrink-0 flex-col"
        style={{ width: widgetLayout.columnPx, maxWidth: "100%" }}
      >
        {children}
      </div>
      {widgetLayout.showRight && (
        <WidgetAreaPlaceholder side="right" />
      )}
    </div>
  );
}
