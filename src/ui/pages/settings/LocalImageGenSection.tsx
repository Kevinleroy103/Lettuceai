import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Cpu, HardDrive, Search, Trash2 } from "lucide-react";

import {
  sdDeleteModel,
  sdGetStatus,
  sdListModels,
  sdRemoveModelRow,
  type SdModelEntry,
  type SdStatus,
} from "../../../core/local-diffusion";
import { useI18n } from "../../../core/i18n/context";
import { Routes } from "../../navigation";
import { toast } from "../../components/toast";

function formatBytes(bytes: number): string {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** exponent).toFixed(exponent > 1 ? 1 : 0)} ${units[exponent]}`;
}

export function LocalImageGenSection() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [status, setStatus] = useState<SdStatus | null>(null);
  const [models, setModels] = useState<SdModelEntry[]>([]);

  const refresh = useCallback(async () => {
    try {
      const [nextStatus, nextModels] = await Promise.all([sdGetStatus(), sdListModels()]);
      setStatus(nextStatus);
      setModels(nextModels);
    } catch (err) {
      console.error("Failed to load local diffusion state:", err);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const deleteEntry = async (entry: SdModelEntry) => {
    try {
      await sdDeleteModel(entry.id, true);
      await sdRemoveModelRow(entry.id);
      void refresh();
    } catch (err) {
      toast.error(
        t("imageGeneration.local.deleteFailed"),
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  if (!status) {
    return null;
  }

  return (
    <div className="space-y-4">
      {!status.binary ? (
        <button
          type="button"
          onClick={() => navigate(Routes.settingsModelsRuntimeDefaults)}
          className="flex w-full items-center gap-3 rounded-[12px] border border-dashed border-fg/15 bg-fg/2 px-4 py-3 text-left transition hover:border-fg/25 hover:bg-fg/5"
        >
          <div className="rounded-[9px] border border-info/30 bg-info/10 p-1.5 text-info/80">
            <Cpu className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-fg/70">
              {t("imageGeneration.local.engineMissingTitle")}
            </p>
            <p className="text-xs text-fg/40">
              {t("imageGeneration.local.engineMissingDescription")}
            </p>
          </div>
        </button>
      ) : null}

      <section className="rounded-[12px] border border-fg/10 bg-fg/5">
        <div className="flex items-start justify-between gap-3 border-b border-fg/8 px-4 py-4">
          <div className="flex items-start gap-3">
            <div className="rounded-[9px] border border-success/30 bg-success/10 p-1.5 text-success/80">
              <HardDrive className="h-4 w-4" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-fg">
                {t("imageGeneration.local.modelsTitle")}
              </h3>
              <p className="mt-1 text-sm leading-6 text-fg/48">
                {t("imageGeneration.local.modelsDescription")}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={() => navigate(`${Routes.settingsModelsBrowse}?mode=sd`)}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-[9px] border border-fg/12 px-3 py-1.5 text-xs font-medium text-fg/70 transition-colors hover:bg-fg/8"
          >
            <Search className="h-3.5 w-3.5" />
            {t("imageGeneration.local.browseHf")}
          </button>
        </div>

        <div className="space-y-3 px-4 py-4">
          {models.length === 0 ? (
            <p className="rounded-[10px] border border-dashed border-fg/12 bg-surface/30 px-3.5 py-3 text-sm text-fg/45">
              {t("imageGeneration.local.noModels")}
            </p>
          ) : (
            models.map((entry) => (
              <div
                key={entry.id}
                className="flex items-center justify-between gap-3 rounded-[10px] border border-fg/10 bg-surface/40 px-3.5 py-3"
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-fg">{entry.name}</div>
                  <div className="mt-0.5 text-xs uppercase text-fg/45">
                    {entry.family}
                    {entry.totalBytes > 0 ? ` · ${formatBytes(entry.totalBytes)}` : ""}
                    {!entry.complete ? (
                      <span className="ml-2 normal-case text-warning/80">
                        {t("imageGeneration.local.incomplete")}
                      </span>
                    ) : null}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => void deleteEntry(entry)}
                  className="shrink-0 rounded-[8px] p-1.5 text-fg/40 transition-colors hover:bg-danger/10 hover:text-danger/80"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            ))
          )}
        </div>
      </section>
    </div>
  );
}
