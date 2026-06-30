import type { ProviderCredentials } from './credentials.js';

/**
 * A `fetch`-compatible function the client sends requests through.
 *
 * It matches the platform `fetch`, so the global is the default. A custom
 * implementation can be supplied to route requests elsewhere, most usefully a
 * mock router in tests.
 */
export type FetchLike = (input: string | URL, init?: RequestInit) => Promise<Response>;

/** Default router base URL: the loopback address and port the router listens on. */
export const DEFAULT_BASE_URL = 'http://localhost:7331';
/** Default per-request timeout, in milliseconds. */
export const DEFAULT_TIMEOUT_MS = 60_000;

/**
 * Connection settings and seeded state for a {@link SmistaClient}.
 *
 * The base URL is the scheme and host only (for example
 * `http://localhost:7331`); the client appends `/api/v1` for the versioned
 * endpoints and reaches `/status` at the root. Held credentials may be seeded
 * here to skip a `bootstrap` or `signIn`, for example to restore a still-valid
 * session token.
 */
export interface SmistaClientConfig {
  /** Router base URL, scheme and host only. Defaults to {@link DEFAULT_BASE_URL}. */
  readonly baseUrl?: string;
  /** Per-request timeout in milliseconds. Defaults to {@link DEFAULT_TIMEOUT_MS}. */
  readonly timeoutMs?: number;
  /** A held API key, as `bootstrap` would store; presented at sign-in. */
  readonly apiKey?: string;
  /** A held session token, as `signIn` would store; authenticates calls. */
  readonly sessionToken?: string;
  /** Provider keys forwarded on the model-calling methods. */
  readonly providerCredentials?: ProviderCredentials;
  /** The `fetch` implementation to use. Defaults to the platform global. */
  readonly fetch?: FetchLike;
}
