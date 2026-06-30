import type { ApiError } from './bindings/ApiError.js';
import type { BootstrapResponse } from './bindings/BootstrapResponse.js';
import type { ContinueRequest } from './bindings/ContinueRequest.js';
import type { CreateSessionRequest } from './bindings/CreateSessionRequest.js';
import type { CreateSessionResponse } from './bindings/CreateSessionResponse.js';
import type { DeleteSessionResponse } from './bindings/DeleteSessionResponse.js';
import type { ExecuteRequest } from './bindings/ExecuteRequest.js';
import type { GetSessionResponse } from './bindings/GetSessionResponse.js';
import type { ListModelsResponse } from './bindings/ListModelsResponse.js';
import type { ListProvidersResponse } from './bindings/ListProvidersResponse.js';
import type { ListSessionsResponse } from './bindings/ListSessionsResponse.js';
import type { MeResponse } from './bindings/MeResponse.js';
import type { PreviewResponse } from './bindings/PreviewResponse.js';
import type { SessionUsageResponse } from './bindings/SessionUsageResponse.js';
import type { SignInResponse } from './bindings/SignInResponse.js';
import type { SignOutResponse } from './bindings/SignOutResponse.js';
import type { StatusResponse } from './bindings/StatusResponse.js';
import type { TraceResponse } from './bindings/TraceResponse.js';
import type { TurnEvent } from './bindings/TurnEvent.js';
import type { TurnResponse } from './bindings/TurnResponse.js';
import type { UpdateSessionRequest } from './bindings/UpdateSessionRequest.js';
import type { UpdateSessionResponse } from './bindings/UpdateSessionResponse.js';
import type { ISmistaClient } from './client.js';
import {
  DEFAULT_BASE_URL,
  DEFAULT_TIMEOUT_MS,
  type FetchLike,
  type SmistaClientConfig,
} from './config.js';
import { ProviderCredentials } from './credentials.js';
import { SmistaError } from './error.js';

/** Header carrying the router API key, presented only at sign-in. */
const API_KEY_HEADER = 'X-Smista-Api-Key';

/** Narrows an unknown decoded body to the router's structured {@link ApiError}. */
function isApiError(value: unknown): value is ApiError {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const error = (value as { error?: unknown }).error;
  return (
    typeof error === 'object' &&
    error !== null &&
    typeof (error as { code?: unknown }).code === 'string' &&
    typeof (error as { message?: unknown }).message === 'string'
  );
}

/**
 * Parses one SSE record's `data:` payload into a {@link TurnEvent}.
 *
 * Per the SSE format, several `data:` lines in a record are joined with a
 * newline and other fields are ignored; the router sends exactly one. A record
 * carrying no data (a comment or keep-alive) yields `undefined`, and a malformed
 * payload throws a decode {@link SmistaError}.
 */
function parseSseRecord(record: string): TurnEvent | undefined {
  let data = '';
  for (const line of record.split('\n')) {
    if (line.startsWith('data:')) {
      const value = line.slice('data:'.length).replace(/^ /, '');
      data = data.length === 0 ? value : `${data}\n${value}`;
    }
  }
  if (data.length === 0) {
    return undefined;
  }
  try {
    return JSON.parse(data) as TurnEvent;
  } catch (cause) {
    throw SmistaError.decode(data, cause);
  }
}

/** Reads the next chunk, or `undefined` at end, mapping a read failure. */
async function readChunk(
  reader: ReadableStreamDefaultReader<Uint8Array>,
): Promise<Uint8Array | undefined> {
  try {
    const chunk = await reader.read();
    return chunk.done ? undefined : chunk.value;
  } catch (cause) {
    throw SmistaError.transport(cause);
  }
}

/**
 * Parses an open `text/event-stream` body into a sequence of {@link TurnEvent}s.
 *
 * Records are split on the blank-line (`\n\n`) separator the router emits. Each
 * event is yielded in order; the stream ends after the terminal `turn_end`. A
 * transport failure mid-stream is rethrown as a transport {@link SmistaError}
 * and a malformed event as a decode one.
 */
async function* sseEvents(stream: ReadableStream<Uint8Array>): AsyncGenerator<TurnEvent> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  try {
    for (;;) {
      const chunk = await readChunk(reader);
      if (chunk === undefined) {
        buffer += decoder.decode();
        const event = parseSseRecord(buffer);
        if (event !== undefined) {
          yield event;
        }
        return;
      }
      buffer += decoder.decode(chunk, { stream: true });
      let separator = buffer.indexOf('\n\n');
      while (separator !== -1) {
        const record = buffer.slice(0, separator);
        buffer = buffer.slice(separator + 2);
        const event = parseSseRecord(record);
        if (event !== undefined) {
          yield event;
          if (event.type === 'turn_end') {
            return;
          }
        }
        separator = buffer.indexOf('\n\n');
      }
    }
  } finally {
    reader.releaseLock();
  }
}

/**
 * A {@link ISmistaClient} implemented over the platform `fetch`.
 *
 * # Held state
 *
 * The client owns its credentials, so the methods take no per-call credential.
 * {@link bootstrap} stores the API key it mints, {@link signIn} exchanges that
 * key for a session token it then holds, and {@link signOut} clears the token.
 * An authenticated call made before a token is held fails with a
 * `notAuthenticated` {@link SmistaError} without a network round trip.
 *
 * The provider keys the router needs to reach a model are held too and travel as
 * request headers on the model-calling methods.
 *
 * # Wire behaviour
 *
 * Versioned endpoints are prefixed with `/api/v1`; the health check is `/status`
 * at the root. A non-success response is decoded as the router's structured
 * {@link ApiError} body and thrown paired with the HTTP status; transport and
 * decode failures throw the corresponding {@link SmistaError}. The streaming
 * methods negotiate `text/event-stream` and parse the events into
 * {@link TurnEvent}s, ending after the terminal `turn_end`. Credentials travel
 * only in headers, never in a query parameter, log or error.
 */
export class SmistaClient implements ISmistaClient {
  private readonly baseUrl: string;
  private readonly timeoutMs: number;
  private readonly fetchFn: FetchLike;
  private readonly providerCredentials: ProviderCredentials;
  private apiKey: string | undefined;
  private sessionToken: string | undefined;

  constructor(config: SmistaClientConfig = {}) {
    this.baseUrl = config.baseUrl ?? DEFAULT_BASE_URL;
    this.timeoutMs = config.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.fetchFn = config.fetch ?? globalThis.fetch;
    this.providerCredentials = config.providerCredentials ?? ProviderCredentials.empty();
    this.apiKey = config.apiKey;
    this.sessionToken = config.sessionToken;
  }

  async status(): Promise<StatusResponse> {
    return this.send('GET', this.url('/status'), new Headers());
  }

  async bootstrap(): Promise<BootstrapResponse> {
    const response = await this.send<BootstrapResponse>(
      'POST',
      this.url('/api/v1/auth/bootstrap'),
      new Headers(),
    );
    // Bootstrap mints the long-lived API key exactly once: hold it so a later
    // sign-in can present it.
    this.apiKey = response.api_key;
    return response;
  }

  async signIn(): Promise<SignInResponse> {
    const headers = new Headers();
    // The API key is presented only at sign-in, only when one is held; absent
    // it, the router answers with a missing-credentials API error.
    if (this.apiKey !== undefined) {
      headers.set(API_KEY_HEADER, this.apiKey);
    }
    const response = await this.send<SignInResponse>(
      'POST',
      this.url('/api/v1/auth/sign-in'),
      headers,
    );
    // Hold the minted session token for subsequent authenticated calls.
    this.sessionToken = response.token;
    return response;
  }

  async signOut(): Promise<SignOutResponse> {
    const response = await this.send<SignOutResponse>(
      'POST',
      this.url('/api/v1/auth/sign-out'),
      this.authedHeaders(),
    );
    // The token is revoked server-side; drop the held copy too.
    this.sessionToken = undefined;
    return response;
  }

  async me(): Promise<MeResponse> {
    return this.send('GET', this.url('/api/v1/auth/me'), this.authedHeaders());
  }

  async createSession(req: CreateSessionRequest): Promise<CreateSessionResponse> {
    return this.send(
      'POST',
      this.url('/api/v1/sessions'),
      this.authedHeaders({ json: true }),
      JSON.stringify(req),
    );
  }

  async listSessions(scope?: string, title?: string): Promise<ListSessionsResponse> {
    // Omitted filters are left off the query, so the router lists every session.
    const url = this.url('/api/v1/sessions');
    if (scope !== undefined) {
      url.searchParams.set('scope', scope);
    }
    if (title !== undefined) {
      url.searchParams.set('title', title);
    }
    return this.send('GET', url, this.authedHeaders());
  }

  async getSession(id: string): Promise<GetSessionResponse> {
    return this.send('GET', this.url(`/api/v1/sessions/${id}`), this.authedHeaders());
  }

  async updateSession(id: string, req: UpdateSessionRequest): Promise<UpdateSessionResponse> {
    return this.send(
      'PUT',
      this.url(`/api/v1/sessions/${id}`),
      this.authedHeaders({ json: true }),
      JSON.stringify(req),
    );
  }

  async deleteSession(id: string): Promise<DeleteSessionResponse> {
    return this.send('DELETE', this.url(`/api/v1/sessions/${id}`), this.authedHeaders());
  }

  async execute(id: string, req: ExecuteRequest): Promise<TurnResponse> {
    return this.send(
      'POST',
      this.url(`/api/v1/sessions/${id}/execute`),
      this.authedHeaders({ json: true, provider: true }),
      JSON.stringify(req),
    );
  }

  async continueRun(id: string, req: ContinueRequest): Promise<TurnResponse> {
    return this.send(
      'POST',
      this.url(`/api/v1/sessions/${id}/continue`),
      this.authedHeaders({ json: true, provider: true }),
      JSON.stringify(req),
    );
  }

  async streamExecute(id: string, req: ExecuteRequest): Promise<AsyncIterable<TurnEvent>> {
    return this.openStream(this.url(`/api/v1/sessions/${id}/execute`), JSON.stringify(req));
  }

  async streamContinue(id: string, req: ContinueRequest): Promise<AsyncIterable<TurnEvent>> {
    return this.openStream(this.url(`/api/v1/sessions/${id}/continue`), JSON.stringify(req));
  }

  async preview(id: string, req: ExecuteRequest): Promise<PreviewResponse> {
    // Preview never calls a model, so it carries no provider credentials.
    return this.send(
      'POST',
      this.url(`/api/v1/sessions/${id}/preview`),
      this.authedHeaders({ json: true }),
      JSON.stringify(req),
    );
  }

  async getSessionTraces(id: string, limit?: number, offset?: number): Promise<TraceResponse> {
    // Omitted bounds fall back to the router's defaults.
    const url = this.url(`/api/v1/sessions/${id}/traces`);
    if (limit !== undefined) {
      url.searchParams.set('limit', String(limit));
    }
    if (offset !== undefined) {
      url.searchParams.set('offset', String(offset));
    }
    return this.send('GET', url, this.authedHeaders());
  }

  async listProviders(): Promise<ListProvidersResponse> {
    return this.send('GET', this.url('/api/v1/llm/providers'), this.authedHeaders());
  }

  async listModels(): Promise<ListModelsResponse> {
    // Provider credentials let the router enumerate credentialed remotes.
    return this.send('GET', this.url('/api/v1/llm/models'), this.authedHeaders({ provider: true }));
  }

  async sessionUsage(id: string): Promise<SessionUsageResponse> {
    return this.send('GET', this.url(`/api/v1/sessions/${id}/usage`), this.authedHeaders());
  }

  /** Joins `path` onto the configured base URL. */
  private url(path: string): URL {
    return new URL(path, this.baseUrl);
  }

  /** Returns the held session token, or throws `notAuthenticated` if none. */
  private requireToken(): string {
    if (this.sessionToken === undefined) {
      throw SmistaError.notAuthenticated();
    }
    return this.sessionToken;
  }

  /**
   * Builds the headers for an authenticated request.
   *
   * Adds the bearer token, throwing `notAuthenticated` before one is held so
   * authenticated calls short-circuit without a request. Optionally marks the
   * body as JSON and attaches the held provider credentials.
   */
  private authedHeaders(options?: {
    readonly json?: boolean;
    readonly provider?: boolean;
  }): Headers {
    const headers = new Headers();
    headers.set('Authorization', `Bearer ${this.requireToken()}`);
    if (options?.json === true) {
      headers.set('Content-Type', 'application/json');
    }
    if (options?.provider === true) {
      for (const [name, value] of Object.entries(this.providerCredentials.headersMap())) {
        headers.set(name, value);
      }
    }
    return headers;
  }

  /** Sends a request and decodes a success body or throws an error response. */
  private async send<T>(method: string, url: URL, headers: Headers, body?: string): Promise<T> {
    let response: Response;
    try {
      response = await this.fetchFn(url, {
        method,
        headers,
        body,
        signal: AbortSignal.timeout(this.timeoutMs),
      });
    } catch (cause) {
      throw SmistaError.transport(cause);
    }
    const text = await SmistaClient.readBody(response);
    if (!response.ok) {
      throw SmistaClient.apiError(response.status, text);
    }
    try {
      return JSON.parse(text) as T;
    } catch (cause) {
      throw SmistaError.decode(text, cause);
    }
  }

  /**
   * Opens an SSE stream of {@link TurnEvent}s for a model-calling POST.
   *
   * Negotiates `text/event-stream` and attaches the held provider credentials.
   * A non-success status is decoded as an {@link ApiError} and thrown before the
   * stream opens; once open, each event is parsed lazily.
   */
  private async openStream(url: URL, body: string): Promise<AsyncIterable<TurnEvent>> {
    const headers = this.authedHeaders({ json: true, provider: true });
    headers.set('Accept', 'text/event-stream');
    let response: Response;
    try {
      response = await this.fetchFn(url, { method: 'POST', headers, body });
    } catch (cause) {
      throw SmistaError.transport(cause);
    }
    if (!response.ok) {
      throw SmistaClient.apiError(response.status, await SmistaClient.readBody(response));
    }
    if (response.body === null) {
      throw SmistaError.decode('', new Error('the streaming response has no body'));
    }
    return sseEvents(response.body);
  }

  /** Reads a response body as text, mapping a read failure to a transport error. */
  private static async readBody(response: Response): Promise<string> {
    try {
      return await response.text();
    } catch (cause) {
      throw SmistaError.transport(cause);
    }
  }

  /** Decodes a non-success body into an `api` error, or a `decode` error. */
  private static apiError(status: number, text: string): SmistaError {
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch (cause) {
      return SmistaError.decode(text, cause);
    }
    if (!isApiError(parsed)) {
      return SmistaError.decode(text, new Error('response body is not a structured API error'));
    }
    return SmistaError.api(status, parsed);
  }
}
