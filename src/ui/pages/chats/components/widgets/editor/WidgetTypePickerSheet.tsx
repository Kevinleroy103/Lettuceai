import {
  ArrowLeftRight,
  Box as BoxIcon,
  Brain,
  Dices,
  Gauge,
  Image as ImageIcon,
  Info,
  ListChecks,
  Minus,
  NotebookPen,
  Sparkles,
  User,
  UserCircle,
  Zap,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { BottomMenu, MenuButton } from "../../../../../components";
import { useWidgetContext } from "../WidgetContext";
import {
  WIDGET_TYPE_DESC,
  WIDGET_TYPE_LABEL,
  type WidgetType,
} from "./widgetFactories";

const TYPE_ORDER: WidgetType[] = [
  "character_info",
  "persona_info",
  "companion_state",
  "memory",
  "stat_tracker",
  "scratch_pad",
  "quick_snippets",
  "dice",
  "session_info",
  "selector",
  "button",
  "image",
  "box",
  "divider",
];

const TYPE_ICON: Record<WidgetType, LucideIcon> = {
  divider: Minus,
  box: BoxIcon,
  character_info: User,
  persona_info: UserCircle,
  scratch_pad: NotebookPen,
  image: ImageIcon,
  selector: ListChecks,
  button: ArrowLeftRight,
  stat_tracker: Gauge,
  quick_snippets: Zap,
  dice: Dices,
  memory: Brain,
  companion_state: Sparkles,
  session_info: Info,
};

interface WidgetTypePickerSheetProps {
  open: boolean;
  onClose: () => void;
  onPick: (type: WidgetType) => void;
}

export function WidgetTypePickerSheet({
  open,
  onClose,
  onPick,
}: WidgetTypePickerSheetProps) {
  const { character } = useWidgetContext();
  const isCompanion = character?.mode === "companion";
  const types = TYPE_ORDER.filter((type) =>
    type === "companion_state" ? isCompanion : true,
  );
  return (
    <BottomMenu isOpen={open} onClose={onClose} title="Add widget">
      <div className="flex flex-col gap-2">
        {types.map((type) => (
          <MenuButton
            key={type}
            icon={TYPE_ICON[type]}
            title={WIDGET_TYPE_LABEL[type]}
            description={WIDGET_TYPE_DESC[type]}
            onClick={() => {
              onPick(type);
              onClose();
            }}
          />
        ))}
      </div>
    </BottomMenu>
  );
}
