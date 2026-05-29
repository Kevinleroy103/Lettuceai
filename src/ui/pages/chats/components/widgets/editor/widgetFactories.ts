import { uuidv4 } from "../../../../../../core/storage/repo";
import type { WidgetNode } from "../../../../../../core/storage/chatWidgetSchemas";

export type WidgetType = WidgetNode["type"];

export const WIDGET_TYPE_LABEL: Record<WidgetType, string> = {
  divider: "Divider",
  box: "Box",
  character_info: "Character info",
  persona_info: "Persona info",
  scratch_pad: "Scratch pad",
  image: "Image",
  selector: "Selector",
  button: "Button",
  stat_tracker: "Stat tracker",
  quick_snippets: "Quick snippets",
  dice: "Dice roller",
  memory: "Memories",
  companion_state: "Companion state",
  session_info: "Session info",
};

export const WIDGET_TYPE_DESC: Record<WidgetType, string> = {
  divider: "A line or space between widgets.",
  box: "Group widgets with an optional title and color.",
  character_info: "Avatar, name, and description of the current character.",
  persona_info: "Avatar, name, and description of the current persona.",
  scratch_pad: "Markdown notes that travel with the chat.",
  image: "Picture from character, persona, library, or upload.",
  selector: "Pick persona, model, or fallback model.",
  button: "Trigger an action like regenerate or swap places.",
  stat_tracker: "Editable numbers like affection, HP, or gold.",
  quick_snippets: "Buttons that insert preset text into the composer.",
  dice: "Roll dice with custom notation.",
  memory: "What this chat remembers.",
  companion_state: "Relationship and mood for companion characters.",
  session_info: "Message count, tokens, and current scene.",
};

export function createWidgetNode(type: WidgetType): WidgetNode {
  const id = uuidv4();
  switch (type) {
    case "divider":
      return { id, type: "divider", style: "line" };
    case "box":
      return {
        id,
        type: "box",
        variant: "default",
        title: "Untitled",
        children: [],
      };
    case "character_info":
      return { id, type: "character_info" };
    case "persona_info":
      return { id, type: "persona_info" };
    case "scratch_pad":
      return { id, type: "scratch_pad", title: "Notes", content: "" };
    case "image":
      return { id, type: "image", source: { kind: "character_avatar" } };
    case "selector":
      return { id, type: "selector", kind: "persona", title: "Persona" };
    case "button":
      return {
        id,
        type: "button",
        action: "regenerate",
        title: "Regenerate last reply",
      };
    case "stat_tracker":
      return {
        id,
        type: "stat_tracker",
        title: "Stats",
        stats: [{ id: uuidv4(), label: "Affection", value: 0 }],
      };
    case "quick_snippets":
      return {
        id,
        type: "quick_snippets",
        title: "Quick snippets",
        snippets: [{ id: uuidv4(), label: "Continue", text: "Continue the scene." }],
      };
    case "dice":
      return { id, type: "dice", title: "Dice", notation: "1d20" };
    case "memory":
      return { id, type: "memory", title: "Memories", limit: 10 };
    case "companion_state":
      return { id, type: "companion_state", title: "Companion" };
    case "session_info":
      return { id, type: "session_info", title: "Session" };
  }
}

export function setLibraryImageOnNode(
  nodes: WidgetNode[],
  id: string,
  imageId: string,
): WidgetNode[] {
  return nodes.map((n) => {
    if (n.id === id && n.type === "image") {
      return { ...n, source: { kind: "library", path: imageId } };
    }
    if (n.type === "box") {
      return { ...n, children: setLibraryImageOnNode(n.children, id, imageId) };
    }
    return n;
  });
}

export function setScratchPadContentOnNode(
  nodes: WidgetNode[],
  id: string,
  content: string,
): WidgetNode[] {
  return nodes.map((n) => {
    if (n.id === id && n.type === "scratch_pad") {
      return { ...n, content };
    }
    if (n.type === "box") {
      return { ...n, children: setScratchPadContentOnNode(n.children, id, content) };
    }
    return n;
  });
}

export function patchWidgetNode(
  nodes: WidgetNode[],
  id: string,
  patch: Partial<WidgetNode>,
): WidgetNode[] {
  return nodes.map((n) => {
    if (n.id === id) {
      return { ...n, ...patch } as WidgetNode;
    }
    if (n.type === "box") {
      return { ...n, children: patchWidgetNode(n.children, id, patch) };
    }
    return n;
  });
}

export function widgetSummary(node: WidgetNode): string {
  switch (node.type) {
    case "divider":
      return node.style === "space" ? "Space" : "Line";
    case "box":
      return node.title || "Untitled box";
    case "character_info":
    case "persona_info":
      return WIDGET_TYPE_LABEL[node.type];
    case "scratch_pad":
      return node.title || "Scratch pad";
    case "image":
      return node.title || `Image: ${node.source.kind.replace("_", " ")}`;
    case "selector":
      return node.title || `Selector: ${node.kind.replace("_", " ")}`;
    case "button":
      return node.title || `Button: ${node.action.replace("_", " ")}`;
    case "stat_tracker":
      return node.title || `Stats (${node.stats.length})`;
    case "quick_snippets":
      return node.title || `Snippets (${node.snippets.length})`;
    case "dice":
      return node.title || `Dice ${node.notation ?? "1d20"}`;
    case "memory":
      return node.title || "Memories";
    case "companion_state":
      return node.title || "Companion state";
    case "session_info":
      return node.title || "Session info";
  }
}
