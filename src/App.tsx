import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useState,
  useRef,
  type ReactNode,
} from "react";
import { toast, Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { ModelStateEvent, RecordingErrorEvent } from "./lib/types/events";
import "./App.css";
import SecureInputWarning from "./components/SecureInputWarning";
import { OnboardingFlow } from "./components/onboarding/OnboardingFlow";
import ModelMigration from "./components/onboarding/ModelMigration";
import {
  ReplaceModel,
  ReturningPermissions,
} from "./components/onboarding/StandaloneScreens";
import {
  clearOnboardingProgress,
  type OnboardingStep as OnboardingPreviewStep,
} from "./components/onboarding/onboardingStore";
import { DebugSettings, ModelsSettings } from "./components/settings";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { FileDropOverlay } from "./components/FileDropOverlay";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { DictationHome } from "./components/home/DictationHome";
import { NotesPage } from "./components/notes/NotesPage";
import { NotetakerPage } from "./components/notetaker/NotetakerPage";
import { DictionaryPage } from "./components/dictionary/DictionaryPage";
import { SnippetsPage } from "./components/snippets/SnippetsPage";
import { TransformsPage } from "./components/transforms/TransformsPage";
import { TopBar } from "./components/TopBar";
import { CommandPalette } from "./components/command/CommandPalette";
import { MeetingDetectedBanner } from "./components/home/MeetingDetectedBanner";
import { StylePage } from "./components/style/StylePage";
import { HelpPage } from "./components/help/HelpPage";
import { SettingsHub } from "./components/settings/SettingsHub";
import {
  GeneralSettings,
  AdvancedSettings,
  AboutSettings,
  HistorySettings,
  IntegrationsSettings,
  PostProcessingSettings,
  SyncSettings,
} from "./components/settings";
import { InsightsPage } from "./components/insights/InsightsPage";
import { WhatsNewGate } from "./components/whats-new";
import { useSettings } from "./hooks/useSettings";
import { useSettingsStore } from "./stores/settingsStore";
import { commands } from "@/bindings";
import { deleteRetiredModel } from "@/lib/models/retired";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";

// flow is first-run onboarding; the rest are single screens for returning users.
type OnboardingStep =
  | "flow"
  | "permissions"
  | "migration"
  | "replaceModel"
  | "done";

/** The working screens render full-bleed; everything else is settings. */
const renderDailyContent = (
  section: SidebarSection,
  onSectionChange: (section: SidebarSection) => void,
) => {
  if (section === "scratchpad") return <NotesPage />;
  if (section === "insights") return <InsightsPage />;
  if (section === "notetaker") return <NotetakerPage />;
  if (section === "dictionary") return <DictionaryPage />;
  if (section === "snippets") return <SnippetsPage />;
  if (section === "transforms") return <TransformsPage />;
  if (section === "style") return <StylePage />;
  if (section === "help") return <HelpPage />;
  return <DictationHome onOpenShortcuts={() => onSectionChange("general")} />;
};

const DAILY_SECTIONS: SidebarSection[] = [
  "home",
  "scratchpad",
  "insights",
  "notetaker",
  "dictionary",
  "snippets",
  "style",
  "transforms",
  "help",
];

const renderSettingsContent = (
  section: SidebarSection,
  onPreviewOnboarding: (step: OnboardingPreviewStep) => void,
  onRunOnboarding: () => void,
  onRunModelMigration: () => void,
  onSectionChange: (section: SidebarSection) => void,
) => {
  if (section === "models") {
    return (
      <ModelsSettings onChangeLanguages={() => onSectionChange("general")} />
    );
  }
  if (section === "debug") {
    return (
      <DebugSettings
        onPreviewOnboarding={onPreviewOnboarding}
        onRunOnboarding={onRunOnboarding}
        onRunModelMigration={onRunModelMigration}
      />
    );
  }
  if (section === "postprocessing") return <PostProcessingSettings />;
  if (section === "integrations") return <IntegrationsSettings />;
  if (section === "sync") return <SyncSettings />;
  if (section === "advanced") return <AdvancedSettings />;
  if (section === "about") return <AboutSettings />;
  if (section === "history") return <HistorySettings />;
  return <GeneralSettings />;
};

function App() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  const [onboardingPreview, setOnboardingPreview] =
    useState<OnboardingPreviewStep | null>(null);
  // Set when a returning user's selected model is a retired (ONNX) model that
  // is still on disk: they must update it before the app can transcribe.
  const [migrationModelId, setMigrationModelId] = useState<string | null>(null);
  const [currentSection, setCurrentSection] = useState<SidebarSection>("home");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const showHistory = useCallback(() => setCurrentSection("history"), []);
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const hasCompletedPostOnboardingInit = useRef(false);
  const settingsScrollRef = useRef<HTMLDivElement>(null);
  // Reset the scroll position whenever the active section changes.
  useLayoutEffect(() => {
    settingsScrollRef.current?.scrollTo({ top: 0 });
  }, [currentSection]);

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // Cmd+K (Ctrl+K) opens the palette; Escape closes it.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "\\") {
        event.preventDefault();
        setSidebarOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([
        commands.initializeEnigo(),
        commands.initializeShortcuts(),
      ]).catch((e) => {
        console.warn("Failed to initialize:", e);
      });
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  // Listen for recording errors from the backend and show a toast
  useEffect(() => {
    const unlisten = listen<RecordingErrorEvent>("recording-error", (event) => {
      const { error_type, detail } = event.payload;

      if (error_type === "microphone_permission_denied") {
        const currentPlatform = platform();
        const platformKey = `errors.micPermissionDenied.${currentPlatform}`;
        const description = t(platformKey, {
          defaultValue: t("errors.micPermissionDenied.generic"),
        });
        toast.error(t("errors.micPermissionDeniedTitle"), { description });
      } else if (error_type === "no_input_device") {
        toast.error(t("errors.noInputDeviceTitle"), {
          description: t("errors.noInputDevice"),
        });
      } else {
        toast.error(
          t("errors.recordingFailed", { error: detail ?? "Unknown error" }),
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for paste failures and show a toast.
  // The technical error detail is logged to flowstate.log on the Rust side
  // (see actions.rs `error!("Failed to paste transcription: ...")`),
  // so we show a localized, user-friendly message here instead of the raw error.
  // The bead's menu can ask for a section (it opens the app on Notes).
  useEffect(() => {
    const unlisten = listen<string>("navigate-section", (event) => {
      setCurrentSection(event.payload as SidebarSection);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen("paste-error", () => {
      toast.error(t("errors.pasteFailedTitle"), {
        description: t("errors.pasteFailed"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for transcription failures and show a toast.
  // The payload is the backend error message (also logged to flowstate.log).
  useEffect(() => {
    const unlisten = listen<string>("transcription-error", (event) => {
      toast.error(t("errors.transcriptionFailedTitle"), {
        description: event.payload,
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for model loading failures and show a toast
  useEffect(() => {
    const unlisten = listen<ModelStateEvent>("model-state-changed", (event) => {
      if (event.payload.event_type === "loading_failed") {
        toast.error(
          t("errors.modelLoadFailed", {
            model:
              event.payload.model_name || t("errors.modelLoadFailedUnknown"),
          }),
          {
            description: event.payload.error,
          },
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  const revealMainWindowForPermissions = async () => {
    try {
      await commands.showMainWindowCommand();
    } catch (e) {
      console.warn("Failed to show main window for permission onboarding:", e);
    }
  };

  const getMigrationModelId = async (
    selectedModel: string,
  ): Promise<string | null> => {
    const retired = await commands.getRetiredModels();
    return retired.some((m) => m.id === selectedModel) ? selectedModel : null;
  };

  // Returning users land in the app unless their model needs updating. The
  // window may start hidden, so bring it forward for the update screen.
  const finishReturningUser = async (retiredModelId: string | null) => {
    if (retiredModelId) {
      await revealMainWindowForPermissions();
      setOnboardingStep("migration");
    } else {
      setOnboardingStep("done");
    }
  };

  const checkOnboardingStatus = async () => {
    try {
      const settingsResult = await commands.getAppSettings();
      const hasCompletedOnboarding =
        settingsResult.status === "ok" &&
        settingsResult.data.onboarding_completed === true;
      const currentPlatform = platform();

      if (hasCompletedOnboarding) {
        // Returning user - check if they need to grant permissions first
        const retiredModelId = await getMigrationModelId(
          settingsResult.data.selected_model ?? "",
        );
        setMigrationModelId(retiredModelId);

        if (currentPlatform === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              await revealMainWindowForPermissions();
              setOnboardingStep("permissions");
              return;
            }
          } catch (e) {
            console.warn("Failed to check macOS permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        if (currentPlatform === "windows") {
          try {
            const microphoneStatus =
              await commands.getWindowsMicrophonePermissionStatus();
            if (
              microphoneStatus.supported &&
              microphoneStatus.overall_access === "denied"
            ) {
              await revealMainWindowForPermissions();
              setOnboardingStep("permissions");
              return;
            }
          } catch (e) {
            console.warn("Failed to check Windows microphone permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        await finishReturningUser(retiredModelId);
      } else {
        // New user - start full onboarding
        setOnboardingStep("flow");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("flow");
    }
  };

  // Debug: the real onboarding from Welcome, for testing it end to end.
  const runOnboarding = () => {
    clearOnboardingProgress();
    setOnboardingStep("flow");
  };

  // Debug: the real model migration screen. It leads with the selected model
  // if that's retired, otherwise with the first retired model on disk.
  const runModelMigration = async () => {
    const retired = await commands.getRetiredModels();
    if (retired.length === 0) {
      toast.info(t("settings.debug.modelMigration.noRetired"));
      return;
    }
    const selected = settings?.selected_model;
    setMigrationModelId(
      retired.find((m) => m.id === selected)?.id ?? retired[0].id,
    );
    setOnboardingStep("migration");
  };

  const handleModelSelected = async () => {
    // Reached when the user picks a different model instead of updating.
    for (const model of await commands.getRetiredModels()) {
      await deleteRetiredModel(model.id);
    }
    setOnboardingStep("done");
  };

  // Rendered once around every step below (including onboarding) so
  // toast.error() calls surface to the user. sonner renders via a portal, so
  // its position in the tree doesn't affect layout. Without this, errors during
  // onboarding (e.g. a model download failing because blob.handy.computer is
  // unreachable) are silently swallowed and the wizard just appears to "blink".
  const toaster = (
    <Toaster
      theme="system"
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            "bg-background border border-line rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm",
          title: "font-medium",
          description: "text-ink-3",
          actionButton:
            "px-2 py-1 text-xs font-medium rounded-lg border bg-wash border-line hover:bg-logo-primary/20 hover:border-logo-primary cursor-pointer whitespace-nowrap",
        },
      }}
    />
  );

  // Still checking onboarding status
  if (onboardingStep === null) {
    return null;
  }

  // Select the content for the current step. The Toaster is rendered once, in a
  // stable wrapper around this node, so crossing between onboarding steps and
  // the main app never remounts it (which would drop any in-flight toast).
  let content: ReactNode;
  if (onboardingPreview) {
    // Previews render in the same top-level slot as real onboarding, inert:
    // no prompts, downloads or setting writes.
    content = (
      <>
        <OnboardingFlow
          preview
          previewStep={onboardingPreview}
          onComplete={() => setOnboardingPreview(null)}
        />
        <button
          type="button"
          onClick={() => setOnboardingPreview(null)}
          className="fixed top-4 end-4 z-50 rounded-lg border border-line bg-background px-4 py-2 text-sm font-medium text-text shadow-lg hover:bg-logo-primary/20 cursor-pointer"
        >
          {t("settings.debug.onboardingPreview.exitButton")}
        </button>
      </>
    );
  } else if (onboardingStep === "flow") {
    content = <OnboardingFlow onComplete={() => setOnboardingStep("done")} />;
  } else if (onboardingStep === "permissions") {
    content = (
      <ReturningPermissions
        onComplete={() => finishReturningUser(migrationModelId)}
      />
    );
  } else if (onboardingStep === "migration" && migrationModelId) {
    content = (
      <ModelMigration
        inUseModelId={migrationModelId}
        onComplete={() => setOnboardingStep("done")}
        onChooseAnother={() => setOnboardingStep("replaceModel")}
      />
    );
  } else if (onboardingStep === "replaceModel") {
    content = (
      <ReplaceModel
        onBack={() => setOnboardingStep("migration")}
        onComplete={handleModelSelected}
      />
    );
  } else {
    content = (
      <div
        dir={direction}
        data-tauri-drag-region
        className="h-screen flex flex-col select-none cursor-default bg-background text-text"
      >
        <ErrorBoundary context="What's New">
          <WhatsNewGate />
        </ErrorBoundary>
        <TopBar
          onOpenSettings={() => setCurrentSection("general")}
          onToggleSidebar={() => setSidebarOpen((open) => !open)}
        />
        <div className="flex-1 flex overflow-hidden">
          {sidebarOpen && (
            <Sidebar
              activeSection={currentSection}
              onSectionChange={setCurrentSection}
            />
          )}
          <main className="flex-1 flex flex-col overflow-hidden">
            <div className="flex-1 overflow-y-auto mb-[8px] me-[8px] rounded-t-[24px] rounded-b-[16px] bg-surface px-[40px] pt-[40px]">
              <MeetingDetectedBanner />
              {DAILY_SECTIONS.includes(currentSection) ? (
                renderDailyContent(currentSection, setCurrentSection)
              ) : (
                <div
                  ref={settingsScrollRef}
                  className={`mx-auto flex w-full flex-col items-center gap-4 pb-10 ${
                    currentSection === "models" ? "h-full" : "max-w-[860px]"
                  }`}
                >
                  <SecureInputWarning />
                  <SettingsHub
                    active={currentSection}
                    onSelect={(id) => setCurrentSection(id as SidebarSection)}
                    render={(id) =>
                      renderSettingsContent(
                        id as SidebarSection,
                        setOnboardingPreview,
                        runOnboarding,
                        runModelMigration,
                        setCurrentSection,
                      )
                    }
                  />
                </div>
              )}
            </div>
          </main>
        </div>
        <CommandPalette
          open={paletteOpen}
          onClose={() => setPaletteOpen(false)}
          onNavigate={setCurrentSection}
        />
        <FileDropOverlay onDropped={showHistory} />
      </div>
    );
  }

  return (
    <>
      {toaster}
      {content}
    </>
  );
}

export default App;
