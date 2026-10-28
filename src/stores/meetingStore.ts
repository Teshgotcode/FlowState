import { create } from "zustand";
import { toast } from "sonner";
import i18n from "@/i18n";
import { commands, events, type MeetingStatus } from "@/bindings";

// The meeting session's stage, as the main window shows it. The backend owns
// the truth: this store only mirrors the status its commands return and the
// MeetingStateEvent it emits, so every window follows one session.
interface MeetingStore {
  status: MeetingStatus;
  initialized: boolean;
  initialize: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  cancel: () => Promise<void>;
}

const idle: MeetingStatus = { state: "idle" };

export const useMeetingStore = create<MeetingStore>()((set) => ({
  status: idle,
  initialized: false,

  initialize: async () => {
    if (useMeetingStore.getState().initialized) return;
    set({ initialized: true });

    // A meeting may already be recording when the window opens.
    try {
      set({ status: await commands.getMeetingStatus() });
    } catch (err) {
      console.warn("Failed to read the meeting status:", err);
    }

    // Start, stop and cancel all emit the event; following it keeps the
    // window correct even when the session changes somewhere else.
    await events.meetingStateEvent.listen(({ payload }) => {
      set({ status: payload.status });
    });
  },

  start: async () => {
    const result = await commands.startMeeting();
    if (result.status === "ok") {
      set({ status: result.data });
    } else {
      toast.error(i18n.t("settings.meeting.startFailed"));
    }
  },

  stop: async () => {
    const result = await commands.stopMeeting();
    if (result.status === "ok") {
      set({ status: idle });
      toast.success(i18n.t("settings.meeting.stopped"));
    } else {
      toast.error(i18n.t("settings.meeting.stopFailed"));
    }
  },

  cancel: async () => {
    const result = await commands.cancelMeeting();
    if (result.status === "ok") {
      set({ status: result.data });
    } else {
      toast.error(i18n.t("settings.meeting.cancelFailed"));
    }
  },
}));
