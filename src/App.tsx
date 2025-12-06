import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpCircle, Coffee, Languages, Moon, Plus, RefreshCw, Settings, SunMedium, Trash2, Undo2 } from "lucide-react";
import { toast } from "sonner";
import { LANGUAGE_LABELS, languageOptions, useI18n, useLanguage } from "@/components/language-provider";
import { type SupportedLanguage, type TranslationValues } from "@/lib/i18n";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Toaster } from "@/components/ui/sonner";
import packageInfo from "../package.json";
import "./App.css";

type ImeStatus = "korean" | "english" | "unknown";

type ProcessInfo = {
  pid: number;
  name: string;
  title: string;
};

type FocusSnapshot = {
  process?: ProcessInfo;
  imeStatus: ImeStatus;
  manualOverride: boolean;
  updatedAt?: string | null;
  lastUpdated?: string;
};

type StatusMessage = {
  key: string;
  values?: TranslationValues;
};

type AppConfig = {
  selectedProcesses: string[];
  useAutoToEn: boolean;
  useMouseMoveEvent: boolean;
  detectIntervalSecs: number;
  mouseSensitivity: number;
  startWithWindows: boolean;
  language: SupportedLanguage;
};

type AppViewModel = {
  savedConfig: AppConfig;
  draftConfig: AppConfig;
  availableProcesses: ProcessInfo[];
  focus?: FocusSnapshot | null;
  hasUnsavedChanges: boolean;
  statusMessage?: StatusMessage | null;
};

const LANGUAGE_ICONS: Record<SupportedLanguage, string> = {
  en: "🇺🇸",
  ko: "🇰🇷",
  ja: "🇯🇵",
  zh: "🇨🇳",
};

const DEFAULT_DETECT_INTERVAL = 0.5;
const DEFAULT_MOUSE_SENSITIVITY = 100;

function App() {
  const { t } = useI18n();
  const { language, setLanguage } = useLanguage();
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const [savedConfig, setSavedConfig] = useState<AppConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<AppConfig | null>(null);
  const [availableProcesses, setAvailableProcesses] = useState<ProcessInfo[]>([]);
  const [processQuery, setProcessQuery] = useState("");
  const [focusSnapshot, setFocusSnapshot] = useState<FocusSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [prioritizeSelected, setPrioritizeSelected] = useState(true);
  const autosaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [appVersion, setAppVersion] = useState<string>(packageInfo.version ?? "0.0.0");
  const [latestVersion, setLatestVersion] = useState<string | null>(null);
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const updateToastShownRef = useRef(false);
  const [checkingLatest, setCheckingLatest] = useState(false);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [versionCheckFailed, setVersionCheckFailed] = useState(false);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);

  const syncFromView = useCallback(
    (view: AppViewModel) => {
      if (view.savedConfig?.language && view.savedConfig.language !== language) {
        setLanguage(view.savedConfig.language as SupportedLanguage);
      }
      setSavedConfig(view.savedConfig);
      setDraftConfig(view.draftConfig);
      setAvailableProcesses(view.availableProcesses);
      if (view.statusMessage) {
        toast.info(t(view.statusMessage.key, view.statusMessage.values));
      }
      if (view.focus) {
        setFocusSnapshot({
          ...view.focus,
          lastUpdated: view.focus.updatedAt ?? new Date().toLocaleTimeString(),
        });
      } else {
        setFocusSnapshot(null);
      }
    },
    [language, setLanguage, t],
  );

  const handleError = useCallback((err: unknown) => {
    const message = err instanceof Error ? err.message : String(err);
    toast.error(message);
  }, []);

  useEffect(() => {
    invoke<string>("get_app_version")
      .then((version) => setAppVersion(version))
      .catch((err) => handleError(err));
  }, [handleError]);

  const isOutdated = useCallback((current: string, latest: string) => {
    const parse = (value: string) => value.split(".").map((part) => Number.parseInt(part, 10) || 0);
    const currentParts = parse(current);
    const latestParts = parse(latest);
    const maxLength = Math.max(currentParts.length, latestParts.length);
    for (let i = 0; i < maxLength; i += 1) {
      const currentPart = currentParts[i] ?? 0;
      const latestPart = latestParts[i] ?? 0;
      if (currentPart < latestPart) return true;
      if (currentPart > latestPart) return false;
    }
    return false;
  }, []);

  useEffect(() => {
    if (!appVersion) return;

    let cancelled = false;

    const fetchLatestVersion = async () => {
      setCheckingLatest(true);
      setVersionCheckFailed(false);
      let attempt = 0;
      const maxAttempts = 4;

      const tryFetch = async () => {
        attempt += 1;
        try {
          const version = await invoke<string>("get_latest_version");
          if (cancelled || typeof version !== "string") return;
          setLatestVersion(version);
          const needsUpdate = isOutdated(appVersion, version);
          setUpdateAvailable(needsUpdate);
          setVersionCheckFailed(false);
          if (needsUpdate && !updateToastShownRef.current) {
            toast.info(t("toast.update.available", { version }));
            updateToastShownRef.current = true;
          }
          setCheckingLatest(false);
          if (retryTimerRef.current) {
            clearTimeout(retryTimerRef.current);
            retryTimerRef.current = null;
          }
        } catch (err) {
          console.error(err);
          if (cancelled || attempt >= maxAttempts) {
            setCheckingLatest(false);
            setVersionCheckFailed(true);
            setUpdateAvailable(false);
            return;
          }
          const delay = Math.min(1000 * 2 ** (attempt - 1), 8000);
          retryTimerRef.current = setTimeout(() => void tryFetch(), delay);
        }
      };

      void tryFetch();
    };

    fetchLatestVersion();

    return () => {
      cancelled = true;
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [appVersion, isOutdated, t]);

  const invokeAndSync = useCallback(
    async (command: string, args?: Record<string, unknown>) => {
      const view = await invoke<AppViewModel>(command, args);
      syncFromView(view);
    },
    [syncFromView],
  );

  const scheduleAutoSave = useCallback(async () => {
    if (autosaveTimerRef.current) {
      clearTimeout(autosaveTimerRef.current);
    }
    autosaveTimerRef.current = setTimeout(async () => {
      try {
        await invokeAndSync("save_changes");
      } catch (err) {
        handleError(err);
      }
    }, 100);
  }, [handleError, invokeAndSync]);

  const resetDetectInterval = useCallback(async () => {
    if (!draftConfig) return;
    try {
      setDraftConfig((prev) =>
        prev ? { ...prev, detectIntervalSecs: DEFAULT_DETECT_INTERVAL } : prev,
      );
      await invokeAndSync("set_detect_interval", { seconds: DEFAULT_DETECT_INTERVAL });
      await scheduleAutoSave();
    } catch (err) {
      handleError(err);
    }
  }, [draftConfig, handleError, invokeAndSync, scheduleAutoSave]);

  const resetMouseSensitivity = useCallback(async () => {
    if (!draftConfig) return;
    try {
      setDraftConfig((prev) => (prev ? { ...prev, mouseSensitivity: DEFAULT_MOUSE_SENSITIVITY } : prev));
      await invokeAndSync("set_mouse_sensitivity", { distance: DEFAULT_MOUSE_SENSITIVITY });
      await scheduleAutoSave();
    } catch (err) {
      handleError(err);
    }
  }, [draftConfig, handleError, invokeAndSync, scheduleAutoSave]);

  const loadState = useCallback(async () => {
    try {
      const view = await invoke<AppViewModel>("load_state");
      syncFromView(view);
    } catch (err) {
      handleError(err);
    } finally {
      setLoading(false);
    }
  }, [handleError, syncFromView]);

  useEffect(() => {
    loadState();
  }, [loadState]);

  useEffect(() => {
    return () => {
      if (autosaveTimerRef.current) {
        clearTimeout(autosaveTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    let unsubscribes: Array<() => void> = [];

    const listenAll = async () => {
      const focusUnlisten = await listen<FocusSnapshot | null>("focus-changed", (event) => {
        const payload = event.payload;
        if (payload) {
          setFocusSnapshot({
            ...payload,
            lastUpdated: payload.updatedAt ?? new Date().toLocaleTimeString(),
          });
        } else {
          setFocusSnapshot(null);
        }
      });

      const statusUnlisten = await listen<StatusMessage>("status-message", (event) => {
        if (event.payload) {
          toast.info(t(event.payload.key, event.payload.values));
        }
      });

      const processUnlisten = await listen<ProcessInfo[]>("processes-updated", (event) => {
        if (Array.isArray(event.payload)) {
          setAvailableProcesses(event.payload);
        }
      });

      unsubscribes = [focusUnlisten, statusUnlisten, processUnlisten];
    };

    listenAll();

    return () => {
      unsubscribes.forEach((unsub) => unsub());
    };
  }, []);

  const filteredProcesses = useMemo(() => {
    const selectedSet = new Set(draftConfig?.selectedProcesses ?? []);
    if (!processQuery.trim()) {
      if (!prioritizeSelected) {
        return availableProcesses;
      }
      return [...availableProcesses].sort(
        (a, b) => Number(selectedSet.has(b.name)) - Number(selectedSet.has(a.name)),
      );
    }
    const filtered = availableProcesses.filter((proc) =>
      `${proc.name} ${proc.title}`.toLowerCase().includes(processQuery.trim().toLowerCase()),
    );
    if (!prioritizeSelected) {
      return filtered;
    }
    return [...filtered].sort((a, b) => Number(selectedSet.has(b.name)) - Number(selectedSet.has(a.name)));
  }, [availableProcesses, processQuery, prioritizeSelected, draftConfig?.selectedProcesses]);

  const handleAddProcess = async (name: string) => {
    try {
      await invokeAndSync("add_selected_process", { name });
      toast.success(t("toast.process.added", { name }));
      await scheduleAutoSave();
    } catch (err) {
      handleError(err);
    }
  };

  const handleRemoveProcess = async (name: string) => {
    try {
      await invokeAndSync("remove_selected_process", { name });
      toast.success(t("toast.process.removed", { name }));
      await scheduleAutoSave();
    } catch (err) {
      handleError(err);
    }
  };

  const handleLanguageChange = useCallback(
    async (next: SupportedLanguage) => {
      setLanguage(next);
      try {
        await invokeAndSync("set_language", { language: next });
        await scheduleAutoSave();
      } catch (err) {
        handleError(err);
      }
    },
    [handleError, invokeAndSync, scheduleAutoSave, setLanguage],
  );

  const handleRefreshProcesses = async () => {
    try {
      await invokeAndSync("refresh_processes");
      toast.info(t("toast.process.refreshed"));
    } catch (err) {
      handleError(err);
    }
  };

  const toggleManualOverride = async (enabled: boolean) => {
    if (!focusSnapshot?.process) return;
    try {
      await invokeAndSync("set_manual_override", {
        processName: focusSnapshot.process.name,
        enabled,
      });
      toast.info(enabled ? t("toast.manual.enabled") : t("toast.manual.disabled"));
    } catch (err) {
      handleError(err);
    }
  };

  const themeIcon = theme === "dark" ? <SunMedium className="size-4" /> : <Moon className="size-4" />;

  const imeLabel = useMemo(() => {
    if (!focusSnapshot) {
      return {
        label: t("focus.status.waiting"),
        tone: "border-muted-foreground/30 bg-muted/40 text-muted-foreground",
      };
    }
    switch (focusSnapshot.imeStatus) {
      case "korean":
        return { label: t("focus.status.korean"), tone: "bg-emerald-500/10 text-emerald-500 border-emerald-500/30" };
      case "english":
        return { label: t("focus.status.english"), tone: "bg-blue-500/10 text-blue-500 border-blue-500/30" };
      default:
        return { label: t("focus.status.unknown"), tone: "bg-muted/40 text-muted-foreground border-muted-foreground/40" };
    }
  }, [focusSnapshot, t]);

  if (loading || !savedConfig || !draftConfig) {
    return (
      <div className="min-h-screen bg-gradient-to-b from-background via-background to-muted/40 px-6 py-10 text-foreground">
        <div className="mx-auto flex max-w-4xl flex-col gap-4 rounded-xl border bg-card px-6 py-8 shadow-sm">
          <p className="text-lg font-semibold">{t("common.loadingTitle")}</p>
          <p className="text-sm text-muted-foreground">{t("common.loadingSubtitle")}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_20%_20%,rgba(59,130,246,0.08),transparent_25%),radial-gradient(circle_at_80%_0%,rgba(16,185,129,0.08),transparent_20%)] bg-gradient-to-b from-background via-background to-muted/40 text-foreground transition-colors">
      <div className="mx-auto box-border flex h-screen max-w-6xl flex-col gap-6 px-6 py-8">
        <div className="grid min-h-0 flex-1 grid-rows-[auto,1fr] gap-6">
          <Card className="shadow-sm">
            <CardHeader className="flex flex-row items-center justify-between space-y-0">
              <div className="flex items-center gap-2">
                <CardTitle className="text-lg font-semibold">{t("common.appName")}</CardTitle>
                <Badge variant="secondary" className="text-xs font-semibold">
                  v{appVersion}
                </Badge>
                {versionCheckFailed ? (
                  <Badge variant="outline" className="text-xs font-semibold border-amber-400 text-amber-700">
                    {t("common.updateCheckFailedBadge")}
                  </Badge>
                ) : null}
                {checkingLatest && !versionCheckFailed ? (
                  <Badge variant="outline" className="text-xs font-semibold border-blue-300 text-blue-700">
                    {t("common.updateCheckingBadge")}
                  </Badge>
                ) : null}
                {!updateAvailable && latestVersion && !versionCheckFailed && !checkingLatest ? (
                  <Badge variant="outline" className="text-xs font-semibold border-emerald-300 text-emerald-700">
                    {t("common.latestVersionBadge")}
                  </Badge>
                ) : null}
                {updateAvailable && latestVersion && !versionCheckFailed ? (
                  <Badge variant="default" className="text-xs font-semibold bg-amber-500 text-amber-950">
                    {t("common.updateAvailableBadge", { version: latestVersion })}
                  </Badge>
                ) : null}
              </div>
              <div className="flex items-center gap-2">
                {updateAvailable ? (
                  <Button
                    variant="outline"
                    size="icon"
                    className="size-9 border-amber-300 bg-amber-50 text-amber-700 shadow-xs transition hover:bg-amber-100 hover:text-amber-800 animate-[pulse_1.6s_ease-in-out_infinite]"
                    onClick={async () => {
                      try {
                        await openUrl("https://github.com/0sami6/langcon");
                      } catch (err) {
                        handleError(err);
                      }
                    }}
                    aria-label={t("common.updateAvailableBadge", { version: latestVersion ?? "" })}
                    title={t("common.updateAvailableBadge", { version: latestVersion ?? "" })}
                  >
                    <ArrowUpCircle className="size-5" />
                  </Button>
                ) : null}
                <Popover>
                  <PopoverTrigger asChild>
                    <Button variant="outline" size="icon" className="size-9">
                      <Settings className="size-4" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-[360px] space-y-4" align="end" sideOffset={8}>
                    <div>
                      <p className="text-sm font-semibold">{t("settings.title")}</p>
                    </div>
                    <div className="space-y-4">
                      <div className="flex items-start justify-between gap-4">
                        <div className="space-y-1">
                          <p className="text-sm font-medium">{t("settings.startWithWindows.title")}</p>
                          <p className="text-xs text-muted-foreground">{t("settings.startWithWindows.description")}</p>
                        </div>
                        <Switch
                          checked={draftConfig.startWithWindows}
                          onCheckedChange={async (checked) => {
                            try {
                              await invokeAndSync("set_start_with_windows", { enabled: Boolean(checked) });
                              toast.info(checked ? t("toast.startup.enabled") : t("toast.startup.disabled"));
                              await scheduleAutoSave();
                            } catch (err) {
                              handleError(err);
                            }
                          }}
                        />
                      </div>
                      <div className="flex items-start justify-between gap-4">
                        <div className="space-y-1">
                          <p className="text-sm font-medium">{t("settings.autoToEn.title")}</p>
                          <p className="text-xs text-muted-foreground">{t("settings.autoToEn.description")}</p>
                        </div>
                        <Switch
                          checked={draftConfig.useAutoToEn}
                          onCheckedChange={async (checked) => {
                            try {
                              await invokeAndSync("set_use_auto_to_en", { enabled: Boolean(checked) });
                              toast.info(checked ? t("toast.autoToEn.enabled") : t("toast.autoToEn.disabled"));
                              await scheduleAutoSave();
                            } catch (err) {
                              handleError(err);
                            }
                          }}
                        />
                      </div>
                      <div className="flex items-start justify-between gap-4">
                        <div className="space-y-1">
                          <p className="text-sm font-medium">{t("settings.mouseMove.title")}</p>
                          <p className="text-xs text-muted-foreground">{t("settings.mouseMove.description")}</p>
                        </div>
                        <Switch
                          checked={draftConfig.useMouseMoveEvent}
                          onCheckedChange={async (checked) => {
                            try {
                              await invokeAndSync("set_use_mouse_move_event", { enabled: Boolean(checked) });
                              toast.info(checked ? t("toast.mouseMove.enabled") : t("toast.mouseMove.disabled"));
                              await scheduleAutoSave();
                            } catch (err) {
                              handleError(err);
                            }
                          }}
                        />
                      </div>
                      <Separator />
                      <div className="space-y-3">
                        <div className="flex items-center justify-between text-sm">
                          <div>
                            <p className="font-medium flex items-center gap-2">
                              {t("settings.detectInterval.title")}
                              <button
                                type="button"
                                onClick={resetDetectInterval}
                                className="text-muted-foreground transition hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                                aria-label={t("settings.detectInterval.reset", {
                                  seconds: DEFAULT_DETECT_INTERVAL.toFixed(1),
                                })}
                                disabled={draftConfig.detectIntervalSecs === DEFAULT_DETECT_INTERVAL}
                              >
                                <Undo2 className="size-4" />
                              </button>
                            </p>
                            <p className="text-xs text-muted-foreground">
                              {t("settings.detectInterval.description", {
                                seconds: draftConfig.detectIntervalSecs.toFixed(1),
                              })}
                            </p>
                          </div>
                          <Badge variant="outline" className="text-muted-foreground">
                            {t("settings.detectInterval.badge", {
                              seconds: draftConfig.detectIntervalSecs.toFixed(1),
                            })}
                          </Badge>
                        </div>
                        <Slider
                          value={[draftConfig.detectIntervalSecs]}
                          min={0.1}
                          max={2}
                          step={0.1}
                          onValueChange={([value]) => {
                            setDraftConfig((prev) => (prev ? { ...prev, detectIntervalSecs: value } : prev));
                          }}
                          onValueCommit={async ([value]) => {
                            try {
                              await invokeAndSync("set_detect_interval", { seconds: value });
                              await scheduleAutoSave();
                            } catch (err) {
                              handleError(err);
                            }
                          }}
                        />
                        <div className="flex items-center justify-between text-sm">
                          <div>
                            <p className="font-medium flex items-center gap-2">
                              {t("settings.mouseSensitivity.title")}
                              <button
                                type="button"
                                onClick={resetMouseSensitivity}
                                className="text-muted-foreground transition hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                                aria-label={t("settings.mouseSensitivity.reset", {
                                  pixels: DEFAULT_MOUSE_SENSITIVITY,
                                })}
                                disabled={draftConfig.mouseSensitivity === DEFAULT_MOUSE_SENSITIVITY}
                              >
                                <Undo2 className="size-4" />
                              </button>
                            </p>
                            <p className="text-xs text-muted-foreground">
                              {t("settings.mouseSensitivity.description", {
                                pixels: draftConfig.mouseSensitivity,
                              })}
                            </p>
                          </div>
                          <Badge variant="outline" className="text-muted-foreground">
                            {t("settings.mouseSensitivity.badge", { pixels: draftConfig.mouseSensitivity })}
                          </Badge>
                        </div>
                        <Slider
                          value={[draftConfig.mouseSensitivity]}
                          min={10}
                          max={500}
                          step={10}
                          onValueChange={([value]) => {
                            setDraftConfig((prev) => (prev ? { ...prev, mouseSensitivity: value } : prev));
                          }}
                          onValueCommit={async ([value]) => {
                            try {
                              await invokeAndSync("set_mouse_sensitivity", { distance: value });
                              await scheduleAutoSave();
                            } catch (err) {
                              handleError(err);
                            }
                          }}
                        />
                      </div>
                      <Separator />
                    </div>
                  </PopoverContent>
                </Popover>
                <Popover>
                  <PopoverTrigger asChild>
                    <Button variant="outline" size="icon" className="size-9">
                      <Languages className="size-4" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-[200px] space-y-2" align="end" sideOffset={8}>
                    <p className="text-sm font-semibold">{t("settings.language.title")}</p>
                    <Select value={language} onValueChange={(value) => handleLanguageChange(value as SupportedLanguage)}>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {languageOptions.map((option) => (
                          <SelectItem key={option} value={option} className="flex items-center gap-2">
                            <span className="mr-2 text-base leading-none">{LANGUAGE_ICONS[option]}</span>
                            {LANGUAGE_LABELS[option]}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </PopoverContent>
                </Popover>
                <Button
                  variant="outline"
                  size="icon"
                  className="size-9"
                  onClick={async () => {
                    try {
                      await openUrl("https://samylabs.gumroad.com/coffee");
                    } catch (err) {
                      handleError(err);
                    }
                  }}
                >
                  <Coffee className="size-4" />
                </Button>
                <Button
                  variant="outline"
                  size="icon"
                  className="size-9"
                  onClick={() => setTheme((prev) => (prev === "light" ? "dark" : "light"))}
                >
                  {themeIcon}
                </Button>
              </div>
            </CardHeader>
            <CardContent className="space-y-4">
              {focusSnapshot?.process ? (
                <div className="rounded-xl border bg-muted/40 p-4">
                  <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                    <div className="space-y-1 sm:min-w-0 sm:flex-1">
                      <p className="text-sm font-medium text-muted-foreground">{t("focus.sectionTitle")}</p>
                      <div className="flex flex-wrap items-center gap-2 sm:gap-3 sm:min-w-0">
                        <p className="text-lg font-semibold sm:min-w-0 sm:max-w-[40%] truncate">
                          {focusSnapshot.process.name}
                        </p>
                        <p className="text-sm text-muted-foreground sm:flex-1 sm:min-w-0 truncate">
                          {focusSnapshot.process.title}
                        </p>
                        <Badge variant="outline" className="text-muted-foreground shrink-0">
                          {t("common.pidLabel", { pid: focusSnapshot.process.pid })}
                        </Badge>
                      </div>
                    </div>
                  </div>
                  <Separator className="my-3" />
                  <div className="flex flex-wrap items-center justify-between gap-4">
                    <div className="flex items-center gap-2">
                      <Switch
                        id="manual-override"
                        checked={focusSnapshot.manualOverride}
                        onCheckedChange={(checked) => toggleManualOverride(Boolean(checked))}
                      />
                      <Label htmlFor="manual-override" className="text-sm font-medium">
                        {t("focus.manualToggle")}
                      </Label>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="outline" className={`border ${imeLabel.tone}`}>
                        {imeLabel.label}
                      </Badge>
                      {focusSnapshot.manualOverride ? (
                        <Badge variant="outline" className="border-amber-300 bg-amber-500/10 text-amber-600">
                          {t("focus.manualBadge")}
                        </Badge>
                      ) : (
                        <Badge variant="outline" className="border-emerald-300 bg-emerald-500/10 text-emerald-600">
                          {t("focus.autoBadge")}
                        </Badge>
                      )}
                    </div>
                  </div>
                </div>
              ) : (
                <div className="rounded-lg border border-dashed bg-muted/40 p-6 text-center text-muted-foreground">
                  {t("common.noActiveWindow")}
                </div>
              )}
            </CardContent>
          </Card>
          <Card className="shadow-sm flex h-full flex-col overflow-hidden">
            <CardHeader className="flex flex-row items-center justify-between space-y-0">
              <div>
                <CardTitle className="text-lg font-semibold">{t("process.title")}</CardTitle>
              </div>
              <div className="flex items-center gap-2">
                <Popover>
                  <PopoverTrigger asChild>
                    <Button variant="outline" size="icon" className="size-9">
                      <Settings className="size-4" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-64 space-y-3" align="end" sideOffset={8}>
                    <div className="flex items-center justify-between gap-3">
                      <div className="space-y-1">
                        <p className="text-sm font-medium">{t("process.settings.prioritizeSelected")}</p>
                        <p className="text-xs text-muted-foreground">{t("process.settings.prioritizeDescription")}</p>
                      </div>
                      <Switch
                        checked={prioritizeSelected}
                        onCheckedChange={(checked) => setPrioritizeSelected(Boolean(checked))}
                      />
                    </div>
                  </PopoverContent>
                </Popover>
                <Button variant="outline" size="icon" className="size-9" onClick={handleRefreshProcesses}>
                  <RefreshCw className="size-4" />
                </Button>
              </div>
            </CardHeader>
            <CardContent className="space-y-6 flex min-h-0 flex-1 flex-col">
              <div className="flex items-center gap-3">
                <Input
                  value={processQuery}
                  onChange={(e) => setProcessQuery(e.currentTarget.value)}
                  placeholder={t("process.searchPlaceholder")}
                  className="h-10"
                />
              </div>
              <ScrollArea className="flex-1 min-h-0 rounded-lg border bg-muted/30 p-3">
                <div className="space-y-3">
                  {filteredProcesses.map((process) => (
                    <div
                      key={`${process.pid}-${process.name}`}
                      className="flex items-center justify-between gap-3 rounded-lg border bg-background/60 p-3 shadow-xs transition hover:border-primary/40"
                    >
                      <div className="min-w-0 flex-1">
                        <p className="text-sm font-semibold">{process.name}</p>
                        <p className="text-xs text-muted-foreground line-clamp-1">{process.title}</p>
                      </div>
                      <Badge variant="outline" className="text-muted-foreground shrink-0">
                        {t("common.pidLabel", { pid: process.pid })}
                      </Badge>
                      {draftConfig.selectedProcesses.includes(process.name) ? (
                        <Button
                          variant="outline"
                          size="sm"
                          className="shrink-0"
                          onClick={() => handleRemoveProcess(process.name)}
                        >
                          <Trash2 className="size-4" />
                          {t("process.action.remove")}
                        </Button>
                      ) : (
                        <Button
                          variant="outline"
                          size="sm"
                          className="shrink-0"
                          onClick={() => handleAddProcess(process.name)}
                        >
                          <Plus className="size-4" />
                          {t("process.action.add")}
                        </Button>
                      )}
                    </div>
                  ))}
                  {filteredProcesses.length === 0 ? (
                    <div className="rounded-lg border border-dashed bg-background/80 p-6 text-center text-muted-foreground">
                      {t("process.empty")}
                    </div>
                  ) : null}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </div>
      </div>
      <Toaster position="top-center" richColors />
    </div>
  );
}

export default App;
