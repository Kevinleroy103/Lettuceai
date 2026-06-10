import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Cpu, Download, FolderOpen, Image as ImageIcon } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  sdFinalizeBinaryInstall,
  sdGetStatus,
  sdListEngineVariants,
  sdQueueBinaryInstall,
  sdRemoveBinary,
  sdSetCustomBinary,
  type SdEngineVariant,
  type SdStatus,
} from "../../../core/local-diffusion";
import { readSettings, saveAdvancedSettings } from "../../../core/storage/repo";
import { useDownloadQueue } from "../../../core/downloads/DownloadQueueContext";
import { useI18n } from "../../../core/i18n/context";
import { toast } from "../../components/toast";
import { NumberInput } from "../../components/NumberInput";

type RuntimeDefaults = {
  llamaDefaultContextLength: number | null;
  llamaDefaultKvCacheType: "auto" | "f16" | "q8_0" | "q4_0";
  sdDefaultOffloadMode: "auto" | "gpu" | "mixed";
  sdDefaultSize: string;
};

function formatBytes(bytes: number): string {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** exponent).toFixed(exponent > 1 ? 1 : 0)} ${units[exponent]}`;
}

const selectClassName =
  "rounded-[9px] border border-fg/12 bg-surface/60 px-3 py-2 text-sm text-fg focus:border-fg/25 focus:outline-none";

function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-[10px] border border-fg/10 bg-surface/40 px-3.5 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-fg">{title}</p>
        <p className="mt-0.5 text-xs leading-5 text-fg/45">{description}</p>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function LocalRuntimeDefaultsPage() {
  const { t } = useI18n();
  const { queue } = useDownloadQueue();
  const [status, setStatus] = useState<SdStatus | null>(null);
  const [variants, setVariants] = useState<SdEngineVariant[] | null>(null);
  const [variantsError, setVariantsError] = useState<string | null>(null);
  const [selectedVariant, setSelectedVariant] = useState<string>("");
  const [installing, setInstalling] = useState(false);
  const [defaults, setDefaults] = useState<RuntimeDefaults | null>(null);
  const finalizedRef = useRef(false);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await sdGetStatus());
    } catch (err) {
      console.error("Failed to load sd.cpp status:", err);
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
    readSettings()
      .then((settings) => {
        const advanced = settings.advancedSettings ?? {};
        setDefaults({
          llamaDefaultContextLength: advanced.llamaDefaultContextLength ?? null,
          llamaDefaultKvCacheType: advanced.llamaDefaultKvCacheType ?? "auto",
          sdDefaultOffloadMode: advanced.sdDefaultOffloadMode ?? "auto",
          sdDefaultSize: advanced.sdDefaultSize ?? "",
        });
      })
      .catch(() => {});
  }, [refreshStatus]);

  useEffect(() => {
    if (!status || status.binary || variants) return;
    sdListEngineVariants()
      .then((list) => {
        setVariants(list);
        setVariantsError(null);
        const recommended = list.find((variant) => variant.recommended);
        setSelectedVariant((current) => current || recommended?.id || list[0]?.id || "");
      })
      .catch((err) => {
        setVariantsError(err instanceof Error ? err.message : String(err));
      });
  }, [status, variants]);

  const persistDefaults = useCallback(async (next: RuntimeDefaults) => {
    setDefaults(next);
    try {
      const settings = await readSettings();
      await saveAdvancedSettings({
        ...(settings.advancedSettings ?? {}),
        llamaDefaultContextLength: next.llamaDefaultContextLength ?? undefined,
        llamaDefaultKvCacheType:
          next.llamaDefaultKvCacheType === "auto" ? undefined : next.llamaDefaultKvCacheType,
        sdDefaultOffloadMode:
          next.sdDefaultOffloadMode === "auto" ? undefined : next.sdDefaultOffloadMode,
        sdDefaultSize: next.sdDefaultSize.trim() || undefined,
      });
    } catch (err) {
      toast.error(
        t("runtimeDefaults.saveFailed"),
        err instanceof Error ? err.message : String(err),
      );
    }
  }, [t]);

  const engineItems = useMemo(
    () => queue.filter((item) => item.queueKind === "sdcpp"),
    [queue],
  );
  const engineActive = engineItems.some(
    (item) => item.status === "queued" || item.status === "downloading",
  );
  const engineFailed = engineItems.find((item) => item.status === "error");
  const engineProgress = useMemo(() => {
    const total = engineItems.reduce((sum, item) => sum + item.total, 0);
    const downloaded = engineItems.reduce((sum, item) => sum + item.downloaded, 0);
    return total > 0 ? Math.round((downloaded / total) * 100) : 0;
  }, [engineItems]);

  useEffect(() => {
    if (!installing || engineItems.length === 0) return;
    if (engineActive || finalizedRef.current) return;
    if (engineItems.every((item) => item.status === "complete")) {
      finalizedRef.current = true;
      sdFinalizeBinaryInstall()
        .then(() => {
          toast.success(t("imageGeneration.local.engineInstalled"));
          setInstalling(false);
          void refreshStatus();
        })
        .catch((err) => {
          toast.error(
            t("imageGeneration.local.engineInstallFailed"),
            err instanceof Error ? err.message : String(err),
          );
          setInstalling(false);
        });
    } else if (engineFailed) {
      toast.error(t("imageGeneration.local.engineInstallFailed"), engineFailed.error ?? "");
      setInstalling(false);
    }
  }, [installing, engineActive, engineFailed, engineItems, refreshStatus, t]);

  const startEngineInstall = async () => {
    try {
      finalizedRef.current = false;
      setInstalling(true);
      await sdQueueBinaryInstall(selectedVariant || null);
    } catch (err) {
      setInstalling(false);
      toast.error(
        t("imageGeneration.local.engineInstallFailed"),
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const pickCustomBinary = async () => {
    const selection = await open({ multiple: false });
    if (typeof selection !== "string") return;
    try {
      await sdSetCustomBinary(selection);
      toast.success(t("runtimeDefaults.customBinarySet"));
      void refreshStatus();
    } catch (err) {
      toast.error(
        t("runtimeDefaults.customBinaryFailed"),
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const removeEngine = async () => {
    try {
      await sdRemoveBinary();
      setVariants(null);
      void refreshStatus();
    } catch (err) {
      toast.error(
        t("imageGeneration.local.engineRemoveFailed"),
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  if (!status || !defaults) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-fg/10 border-t-fg/60" />
      </div>
    );
  }

  return (
    <div className="flex min-h-screen flex-col">
      <main className="flex-1 px-4 pb-24 pt-5">
        <div className="mx-auto w-full max-w-3xl space-y-6">
          <section className="rounded-[12px] border border-fg/10 bg-fg/5">
            <div className="flex items-start gap-3 border-b border-fg/8 px-4 py-4">
              <div className="rounded-[9px] border border-info/30 bg-info/10 p-1.5 text-info/80">
                <ImageIcon className="h-4 w-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-fg">
                  {t("runtimeDefaults.engineTitle")}
                </h3>
                <p className="mt-1 text-sm leading-6 text-fg/48">
                  {status.binary
                    ? t("imageGeneration.local.engineInstalledDescription")
                    : t("imageGeneration.local.engineDescription")}
                </p>
              </div>
            </div>
            <div className="space-y-3 px-4 py-4">
              {status.binary ? (
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0 text-sm text-fg/70">
                    <span className="font-medium text-fg">{status.binary.variant}</span>
                    <span className="ml-2 text-xs text-fg/45">{status.binary.releaseTag}</span>
                    <p
                      className="mt-0.5 truncate font-mono text-xs text-fg/40"
                      title={status.binary.path}
                    >
                      {status.binary.path}
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={() => void removeEngine()}
                    className="rounded-[9px] border border-danger/25 px-3 py-1.5 text-xs font-medium text-danger/80 transition-colors hover:bg-danger/10"
                  >
                    {t("common.buttons.remove")}
                  </button>
                </div>
              ) : engineActive || installing ? (
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-xs text-fg/55">
                    <span>{t("imageGeneration.local.engineDownloading")}</span>
                    <span>{engineProgress}%</span>
                  </div>
                  <div className="h-1.5 overflow-hidden rounded-full bg-fg/10">
                    <div
                      className="h-full rounded-full bg-accent transition-all"
                      style={{ width: `${engineProgress}%` }}
                    />
                  </div>
                </div>
              ) : variantsError ? (
                <p className="text-xs leading-5 text-danger/80">{variantsError}</p>
              ) : !variants ? (
                <p className="text-xs text-fg/45">
                  {t("imageGeneration.local.engineLoadingVariants")}
                </p>
              ) : (
                <div className="flex flex-wrap items-center gap-3">
                  <select
                    value={selectedVariant}
                    onChange={(event) => setSelectedVariant(event.target.value)}
                    className={selectClassName}
                  >
                    {variants.map((variant) => (
                      <option key={variant.id} value={variant.id}>
                        {variant.id}
                        {variant.recommended
                          ? ` (${t("imageGeneration.local.recommended")})`
                          : ""}{" "}
                        · {formatBytes(variant.size)}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    onClick={() => void startEngineInstall()}
                    className="inline-flex items-center gap-2 rounded-[9px] border border-accent/35 bg-accent/12 px-3.5 py-2 text-sm font-medium text-accent transition-colors hover:bg-accent/20"
                  >
                    <Download className="h-4 w-4" />
                    {t("imageGeneration.local.installEngine")}
                  </button>
                </div>
              )}
              {!engineActive && !installing ? (
                <button
                  type="button"
                  onClick={() => void pickCustomBinary()}
                  className="inline-flex items-center gap-2 rounded-[9px] border border-fg/12 px-3.5 py-2 text-sm font-medium text-fg/70 transition-colors hover:bg-fg/8"
                >
                  <FolderOpen className="h-4 w-4" />
                  {t("runtimeDefaults.useExistingBinary")}
                </button>
              ) : null}
              <p className="text-xs leading-5 text-fg/40">
                {t("runtimeDefaults.customBinaryHint")}
              </p>
            </div>
          </section>

          <section className="rounded-[12px] border border-fg/10 bg-fg/5">
            <div className="flex items-start gap-3 border-b border-fg/8 px-4 py-4">
              <div className="rounded-[9px] border border-warning/30 bg-warning/10 p-1.5 text-warning/80">
                <Cpu className="h-4 w-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-fg">
                  {t("runtimeDefaults.llamaTitle")}
                </h3>
                <p className="mt-1 text-sm leading-6 text-fg/48">
                  {t("runtimeDefaults.llamaDescription")}
                </p>
              </div>
            </div>
            <div className="space-y-3 px-4 py-4">
              <SettingRow
                title={t("runtimeDefaults.llamaContextTitle")}
                description={t("runtimeDefaults.llamaContextDescription")}
              >
                <div className="w-32">
                  <NumberInput
                    min={512}
                    max={1048576}
                    step={1024}
                    value={defaults.llamaDefaultContextLength}
                    onChange={(next) =>
                      void persistDefaults({
                        ...defaults,
                        llamaDefaultContextLength: next === null ? null : Math.trunc(next),
                      })
                    }
                    placeholder="8192"
                    className="w-full rounded-[9px] border border-fg/12 bg-surface/60 px-3 py-2 text-center text-sm text-fg focus:border-fg/25 focus:outline-none"
                  />
                </div>
              </SettingRow>
              <SettingRow
                title={t("runtimeDefaults.llamaKvTitle")}
                description={t("runtimeDefaults.llamaKvDescription")}
              >
                <select
                  value={defaults.llamaDefaultKvCacheType}
                  onChange={(event) =>
                    void persistDefaults({
                      ...defaults,
                      llamaDefaultKvCacheType: event.target
                        .value as RuntimeDefaults["llamaDefaultKvCacheType"],
                    })
                  }
                  className={selectClassName}
                >
                  <option value="auto">{t("common.labels.auto")}</option>
                  <option value="f16">F16</option>
                  <option value="q8_0">Q8_0</option>
                  <option value="q4_0">Q4_0</option>
                </select>
              </SettingRow>
            </div>
          </section>

          <section className="rounded-[12px] border border-fg/10 bg-fg/5">
            <div className="flex items-start gap-3 border-b border-fg/8 px-4 py-4">
              <div className="rounded-[9px] border border-success/30 bg-success/10 p-1.5 text-success/80">
                <ImageIcon className="h-4 w-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-fg">{t("runtimeDefaults.sdTitle")}</h3>
                <p className="mt-1 text-sm leading-6 text-fg/48">
                  {t("runtimeDefaults.sdDescription")}
                </p>
              </div>
            </div>
            <div className="space-y-3 px-4 py-4">
              <SettingRow
                title={t("runtimeDefaults.sdOffloadTitle")}
                description={t("runtimeDefaults.sdOffloadDescription")}
              >
                <select
                  value={defaults.sdDefaultOffloadMode}
                  onChange={(event) =>
                    void persistDefaults({
                      ...defaults,
                      sdDefaultOffloadMode: event.target
                        .value as RuntimeDefaults["sdDefaultOffloadMode"],
                    })
                  }
                  className={selectClassName}
                >
                  <option value="auto">{t("editModel.sdOffload.auto")}</option>
                  <option value="gpu">{t("editModel.sdOffload.gpu")}</option>
                  <option value="mixed">{t("editModel.sdOffload.mixed")}</option>
                </select>
              </SettingRow>
              <SettingRow
                title={t("runtimeDefaults.sdSizeTitle")}
                description={t("runtimeDefaults.sdSizeDescription")}
              >
                <input
                  type="text"
                  value={defaults.sdDefaultSize}
                  onChange={(event) =>
                    setDefaults({ ...defaults, sdDefaultSize: event.target.value })
                  }
                  onBlur={(event) =>
                    void persistDefaults({ ...defaults, sdDefaultSize: event.target.value })
                  }
                  placeholder="1024x1024"
                  className="w-32 rounded-[9px] border border-fg/12 bg-surface/60 px-3 py-2 text-center text-sm text-fg placeholder:text-fg/35 focus:border-fg/25 focus:outline-none"
                />
              </SettingRow>
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}
