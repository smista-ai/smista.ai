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

/**
 * An async client for the `smista-router` HTTP JSON API.
 *
 * The interface has one method per router endpoint — health, authentication,
 * sessions, execution, streaming, route preview, traces, providers and models,
 * and usage. Concrete implementations live elsewhere; this interface is the
 * backend-agnostic contract they share, so consumers program against it rather
 * than hand-rolling HTTP.
 *
 * Credentials are held by the implementation, never passed per call.
 * `bootstrap` and `signIn` populate the held session token, `signOut` clears it,
 * and every authenticated method uses it. The provider keys the router needs to
 * reach a model are configured on the implementation too, so the methods that
 * can call a model take no credential argument either.
 *
 * Session identifiers are router UUIDs, passed as strings.
 */
export interface ISmistaClient {
  /** Calls the public `GET /status` health check, outside `/api/v1`. */
  status(): Promise<StatusResponse>;

  /**
   * Calls `POST /api/v1/auth/bootstrap` to create the first user and mint its
   * long-lived API key, returned exactly once.
   */
  bootstrap(): Promise<BootstrapResponse>;

  /**
   * Calls `POST /api/v1/auth/sign-in`, exchanging the held API key for a
   * session token the client then holds for authenticated calls.
   */
  signIn(): Promise<SignInResponse>;

  /** Calls `POST /api/v1/auth/sign-out`, revoking and clearing the held token. */
  signOut(): Promise<SignOutResponse>;

  /**
   * Calls `GET /api/v1/auth/me`, identifying the authenticated user from the
   * held session token.
   */
  me(): Promise<MeResponse>;

  /** Calls `POST /api/v1/sessions` to create a session. */
  createSession(req: CreateSessionRequest): Promise<CreateSessionResponse>;

  /**
   * Calls `GET /api/v1/sessions` to list the authenticated user's sessions,
   * optionally filtered by `scope` (exact match) and `title` (case-insensitive
   * substring). Each filter is omitted from the query when not given.
   */
  listSessions(scope?: string, title?: string): Promise<ListSessionsResponse>;

  /** Calls `GET /api/v1/sessions/{id}` to fetch a full session. */
  getSession(id: string): Promise<GetSessionResponse>;

  /**
   * Calls `PUT /api/v1/sessions/{id}` to update a session's title and/or
   * archive state, returning the updated summary.
   */
  updateSession(id: string, req: UpdateSessionRequest): Promise<UpdateSessionResponse>;

  /** Calls `DELETE /api/v1/sessions/{id}` to delete a session and its content. */
  deleteSession(id: string): Promise<DeleteSessionResponse>;

  /**
   * Calls `POST /api/v1/sessions/{id}/execute` and buffers the turn as a single
   * `TurnResponse`. For incremental delivery, use `streamExecute` instead.
   */
  execute(id: string, req: ExecuteRequest): Promise<TurnResponse>;

  /**
   * Calls `POST /api/v1/sessions/{id}/continue` to advance an in-flight run
   * (tool results, approval decisions, decrypted records, injected input or an
   * interrupt) and buffers the next turn as a `TurnResponse`.
   */
  continueRun(id: string, req: ContinueRequest): Promise<TurnResponse>;

  /**
   * Calls `POST /api/v1/sessions/{id}/execute` with `Accept: text/event-stream`
   * and yields the turn as a stream of `TurnEvent`s, terminated by a `turn_end`
   * carrying the final `TurnResponse`.
   *
   * The returned promise rejects if the stream cannot be opened; once open, the
   * async iterable yields each event in order.
   */
  streamExecute(id: string, req: ExecuteRequest): Promise<AsyncIterable<TurnEvent>>;

  /**
   * Calls `POST /api/v1/sessions/{id}/continue` with `Accept: text/event-stream`
   * and yields the advanced turn as a stream of `TurnEvent`s, terminated by a
   * `turn_end` carrying the final `TurnResponse`.
   *
   * The returned promise rejects if the stream cannot be opened; once open, the
   * async iterable yields each event in order.
   */
  streamContinue(id: string, req: ContinueRequest): Promise<AsyncIterable<TurnEvent>>;

  /**
   * Calls `POST /api/v1/sessions/{id}/preview` to predict how a task would be
   * routed without calling the model. Takes the same body and uses the same
   * provider credentials as `execute`.
   */
  preview(id: string, req: ExecuteRequest): Promise<PreviewResponse>;

  /**
   * Calls `GET /api/v1/sessions/{id}/traces`, a single paginated list windowed
   * by `limit` and `offset` (each defaulting on the router when omitted).
   */
  getSessionTraces(id: string, limit?: number, offset?: number): Promise<TraceResponse>;

  /** Calls `GET /api/v1/llm/providers` to list the available providers. */
  listProviders(): Promise<ListProvidersResponse>;

  /**
   * Calls `GET /api/v1/llm/models` to list the available models. A provider the
   * router cannot list is reported under `ListModelsResponse.unavailable` rather
   * than failing the call.
   */
  listModels(): Promise<ListModelsResponse>;

  /**
   * Calls `GET /api/v1/sessions/{id}/usage` for the session's token and cost
   * breakdown.
   */
  sessionUsage(id: string): Promise<SessionUsageResponse>;
}
