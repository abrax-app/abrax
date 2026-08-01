export interface ModelStateEvent {
  event_type: string;
  model_id?: string;
  model_name?: string;
  error?: string;
}

/** Payload del evento `spectrum` (regla R8: solo se emite con suscriptor). */
export interface SpectrumPayload {
  bands: number[];
  rms: number;
  bass: number;
  dominant: number;
}
