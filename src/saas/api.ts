import { getTransport } from "../lib/transport";
import type { AdminOperation, SaasErrorEnvelope, SaasPublicConfig, SaasSession, UserOperation } from "./types";

export class SaasApiError extends Error {
  constructor(public code: string, message: string, public status = 0, public details?: unknown) {
    super(message);
    this.name = "SaasApiError";
  }
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) return String(error.message);
  return String(error);
}

async function readResponse<Result>(response: Response): Promise<Result> {
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const failure = body as Partial<SaasErrorEnvelope> | null;
    throw new SaasApiError(failure?.code || `http_${response.status}`, failure?.message || `HTTP ${response.status}`, response.status, failure?.details);
  }
  if (body === null) throw new SaasApiError("invalid_response", "The server returned an invalid JSON response.");
  return body as Result;
}

export function createUserClient(fetcher: typeof fetch = (...args) => fetch(...args), onExpired?: () => void) {
  let csrfToken: string | null = null;
  async function request<Result>(path: string, payload?: unknown): Promise<Result> {
    try {
      const requestHeaders: Record<string, string> = payload === undefined
        ? { Accept: "application/json" }
        : { "Content-Type": "application/json" };
      if (csrfToken) requestHeaders["x-saas-csrf"] = csrfToken;
      return await readResponse<Result>(await fetcher(`/api/saas/${path}`, {
        method: payload === undefined ? "GET" : "POST",
        credentials: "same-origin",
        cache: "no-store",
        headers: requestHeaders,
        ...(payload === undefined ? {} : { body: JSON.stringify(payload) }),
      }));
    } catch (error) {
      if (error instanceof SaasApiError && (error.status === 401 || error.code === "saas_disabled" || error.code === "user_banned")) {
        csrfToken = null;
        if (path.startsWith("user/")) onExpired?.();
      }
      throw error;
    }
  }
  return {
    publicConfig: () => request<SaasPublicConfig>("public/config"),
    async session(): Promise<SaasSession> {
      try {
        const result = await request<SaasSession>("auth/session");
        csrfToken = result.user ? result.csrfToken : null;
        return result;
      } catch (error) {
        if (error instanceof SaasApiError && error.status === 401) return { user: null, csrfToken: null };
        throw error;
      }
    },
    async passwordLogin(email: string, password: string): Promise<SaasSession> {
      const result = await request<SaasSession>("auth/password", { email, password });
      csrfToken = result.csrfToken;
      return result;
    },
    call<Result>(operation: UserOperation, payload: unknown = {}): Promise<Result> {
      if (!csrfToken) return Promise.reject(new SaasApiError("unauthenticated", "Please sign in again.", 401));
      return request<Result>(`user/${operation}`, payload);
    },
    async logout(): Promise<void> {
      await request("auth/logout", {});
      csrfToken = null;
    },
  };
}

export type SaasUserClient = ReturnType<typeof createUserClient>;

export function adminCall<Result>(operation: AdminOperation, payload: unknown = {}): Promise<Result> {
  return getTransport().call<Result>("saas_admin", { operation, payload });
}
