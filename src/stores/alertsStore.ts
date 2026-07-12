import { create } from "zustand";
import { commands, events } from "@/bindings";
import type { UserAlertEvent } from "@/bindings";

/**
 * Centro de errores (F1): espejo frontend del registro de alertas del backend.
 * Se alimenta del evento único `UserAlertEvent` en vivo y de
 * `get_recent_alerts` al montar — así los fallos ocurridos con la ventana
 * oculta quedan visibles al abrirla.
 */
interface AlertsState {
  alerts: UserAlertEvent[];
  initialized: boolean;
  initialize: () => Promise<void>;
  dismissAll: () => Promise<void>;
}

const MAX_ALERTS = 20;

export const useAlertsStore = create<AlertsState>((set, get) => ({
  alerts: [],
  initialized: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });

    try {
      const recent = await commands.getRecentAlerts();
      set({ alerts: recent });
    } catch (e) {
      console.error("Failed to load recent alerts:", e);
    }

    await events.userAlertEvent.listen((event) => {
      set((state) => ({
        alerts: [event.payload, ...state.alerts].slice(0, MAX_ALERTS),
      }));
    });
  },

  dismissAll: async () => {
    set({ alerts: [] });
    try {
      await commands.clearRecentAlerts();
    } catch (e) {
      console.error("Failed to clear recent alerts:", e);
    }
  },
}));
