import { create } from "zustand";
import { toast } from "sonner";
import i18n from "@/i18n";
import {
  commands,
  events,
  type MeetingSegment,
  type MeetingStatus,
} from "@/bindings";

// The most segments a live transcript keeps, so an hours-long meeting cannot
// grow the list without bound. Enough for a whole day of continuous speech.
const MAX_SEGMENTS = 2000;

// The meeting session's stage, as the main window shows it. The backend owns
// the truth: this store only mirrors the status its commands return and the
// events it emits, so every window follows one session. `segments` is the
// live transcript: every piece the backend's already-running decode finished,
// in the order it was spoken. It survives the stop (it is the transcript the
// finished entry ends up with) and is cleared when the next meeting starts.
interface MeetingStore {
  status: MeetingStatus;
  segments: MeetingSegment[];
  initialized: boolean;
  initialize: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  cancel: () => Promise<void>;
}

const idle: MeetingStatus = { state: "idle" };

export const useMeetingStore = create<MeetingStore>()((set) => ({
  status: idle,
  segments: [],
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

    // The live transcript: each finished piece of speech arrives as it is
    // decoded, not after the stop. Kept in arrival order and capped, so an
    // hours-long meeting cannot grow the list without bound.
    await events.meetingSegment.listen(({ payload }) => {
      set((state) => {
        const segments = [...state.segments, payload];
        return {
          segments:
            segments.length > MAX_SEGMENTS
              ? segments.slice(segments.length - MAX_SEGMENTS)
              : segments,
        };
      });
    });
  },

  start: async () => {
    const result = await commands.startMeeting();
    // A new session starts a new transcript: the previous meeting's segments
    // belong to its own entry in History.
    if (result.status === "ok") set({ segments: [] });
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
    // A cancelled meeting saves nothing; its live transcript goes with it.
    if (result.status === "ok") set({ segments: [] });
    if (result.status === "ok") {
      set({ status: result.data });
    } else {
      toast.error(i18n.t("settings.meeting.cancelFailed"));
    }
  },
}));
