export type ImageSession = { id: string; title: string; platform: "codex" | "gemini"; archived: boolean; created_at: string; updated_at: string };
export type ImageMessage = { id: string; session_id: string; role: "user" | "assistant"; prompt: string; request_json: string; status: string; model?: string | null; error_message?: string | null; image_count: number; created_at: string; updated_at: string };
export type ImageAsset = { id: string; session_id: string; message_id: string; relative_path: string; mime_type: string; sha256: string; width?: number | null; height?: number | null; byte_size: number; created_at: string };
export type ImageConversation = { session: ImageSession; messages: ImageMessage[]; assets: ImageAsset[] };
export type ImageModelOption = { id: string; upstream_model: string; capabilities: string[] };
export type GenerateImageInput = { session_id: string; model: string; prompt: string; count: number; size: string; quality: string };
export type GenerateImageOutcome = { message: ImageMessage; assets: ImageAsset[] };
export type ImageAssetContent = { asset: ImageAsset; data_base64: string };
