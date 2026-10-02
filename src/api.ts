import { invoke } from "@tauri-apps/api/core";

export interface Clip {
  id: number;
  content: string;
  createdAt: number;
}

export interface EffectState {
  effect: string;
  transparencyEnabled: boolean;
  highContrast: boolean;
  windowsBuild: number;
  detail: string;
}

export function listClips(): Promise<Clip[]> {
  return invoke("list_clips");
}

export function pasteClip(id: number): Promise<void> {
  return invoke("paste_clip", { id });
}

export function hidePanel(): Promise<void> {
  return invoke("hide_panel");
}

export function getEffectState(): Promise<EffectState> {
  return invoke("get_effect_state");
}
