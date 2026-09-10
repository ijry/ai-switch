import { getTransport } from "../lib/transport";
import type { GenerateImageInput, GenerateImageOutcome, ImageAssetContent, ImageConversation, ImageModelOption, ImageSession } from "./types";

const call = <T,>(command: string, args?: Record<string, unknown>) => getTransport().call<T>(command, args);
export const imagegenApi = {
  listSessions: () => call<ImageSession[]>("imagegen_list_sessions", { includeArchived: false }),
  createSession: (input: { title: string; platform: "codex" | "gemini" }) => call<ImageSession>("imagegen_create_session", { input }),
  updateSession: (input: { id: string; title?: string; archived?: boolean }) => call<ImageSession>("imagegen_update_session", { input }),
  deleteSession: (id: string) => call<void>("imagegen_delete_session", { id }),
  conversation: (id: string) => call<ImageConversation>("imagegen_get_conversation", { id }),
  models: (platform: string) => call<ImageModelOption[]>("imagegen_list_models", { platform }),
  generate: (input: GenerateImageInput) => call<GenerateImageOutcome>("imagegen_generate", { input }),
  asset: (id: string) => call<ImageAssetContent>("imagegen_read_asset", { id }),
};
