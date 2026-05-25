/**
 * Client SDK for the smista-router HTTP API.
 *
 * `@smista-ai/sdk` is a thin, typed client over the router's REST API. It must
 * not reimplement routing, policy evaluation, provider selection or tool
 * mediation; that logic stays owned by smista-router.
 *
 * The concrete resource methods (auth, sessions, execute, stream, preview,
 * trace, usage, providers/models) and the API types generated from
 * `smista-core` are added in milestone M7.
 */

/** Options used to construct a {@link SmistaClient}. */
export interface SmistaClientOptions {
  /** Base URL of the smista-router API, e.g. `http://127.0.0.1:7331`. */
  readonly routerUrl: string;
  /** Short-lived router session token sent as a bearer credential. */
  readonly token?: string;
}

/**
 * Typed client for smista-router.
 *
 * Resource accessors are introduced in milestone M7.
 */
export class SmistaClient {
  readonly #routerUrl: string;
  readonly #token?: string;

  constructor(options: SmistaClientOptions) {
    this.#routerUrl = options.routerUrl;
    this.#token = options.token;
  }

  /** The configured router base URL. */
  get routerUrl(): string {
    return this.#routerUrl;
  }

  /** Whether a session token is currently configured. */
  get authenticated(): boolean {
    return this.#token !== undefined;
  }
}
