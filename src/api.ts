import { invoke } from "@tauri-apps/api/core";

export type ClipKind = "text" | "link" | "code" | "image";
export type OcrStatus = "none" | "pending" | "done" | "empty" | "failed";

export interface Clip {
  id: number;
  content: string;
  createdAt: number;
  kind: ClipKind;
  language: string | null;
  width: number | null;
  height: number | null;
  ocrStatus: OcrStatus;
  ocrText: string | null;
  ocrLang: string | null;
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

export function pasteClip(id: number, asText = false): Promise<void> {
  return invoke("paste_clip", { id, asText });
}

export function copyClip(id: number, asText = false): Promise<void> {
  return invoke("copy_clip", { id, asText });
}

export function clipImage(id: number): Promise<string> {
  return invoke("clip_image", { id });
}

export function clipThumb(id: number): Promise<string> {
  return invoke("clip_thumb", { id });
}

export function retryOcr(id: number): Promise<void> {
  return invoke("retry_ocr", { id });
}

export function hidePanel(): Promise<void> {
  return invoke("hide_panel");
}

export function armWindowDrag(): Promise<void> {
  return invoke("arm_window_drag");
}

export function getEffectState(): Promise<EffectState> {
  return invoke("get_effect_state");
}
