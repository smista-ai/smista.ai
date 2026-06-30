import type { TurnEvent } from '../src/bindings/TurnEvent.js';
import type { TurnResponse } from '../src/bindings/TurnResponse.js';
import type { FetchLike } from '../src/config.js';

/** A request the mock recorded, with its headers and body. */
export interface RecordedRequest {
  readonly method: string;
  readonly url: URL;
  readonly headers: Headers;
  readonly body: string;
}

/** A canned response a mock endpoint returns. */
export interface MockResponse {
  /** HTTP status; defaults to `200`. */
  readonly status?: number;
  /** A JSON body, serialized and sent with a JSON content type. */
  readonly json?: unknown;
  /** A raw text body, sent verbatim. */
  readonly text?: string;
  /** A raw `text/event-stream` body, sent verbatim. */
  readonly sse?: string;
}

/** The logical endpoints the router exposes, used to key overrides. */
export type Endpoint =
  | 'status'
  | 'bootstrap'
  | 'signIn'
  | 'signOut'
  | 'me'
  | 'createSession'
  | 'listSessions'
  | 'getSession'
  | 'updateSession'
  | 'deleteSession'
  | 'execute'
  | 'continueRun'
  | 'preview'
  | 'traces'
  | 'listProviders'
  | 'listModels'
  | 'sessionUsage';

/** A minimal but valid `idle` turn outcome, used by the turn endpoints. */
const turn: TurnResponse = { status: 'idle', data: { trace_id: 'trace-1' } };

/** Default success bodies the mock returns for each endpoint. */
export const defaults = {
  status: { status: 'ok', version: '0.0.0' },
  bootstrap: { user_id: 'user:abc', api_key: 'sk-smista-test-key' },
  signIn: { token: 'session-token-xyz', expires_at: '2099-01-01T00:00:00Z' },
  signOut: { revoked: true },
  me: { user_id: 'user:abc' },
  createSession: { id: '00000000-0000-0000-0000-000000000000' },
  listSessions: { sessions: [] },
  getSession: { id: '00000000-0000-0000-0000-000000000000' },
  updateSession: { id: '00000000-0000-0000-0000-000000000000' },
  deleteSession: { deleted: true },
  turn,
  preview: { decision: 'preview' },
  traces: { traces: [], total: 0 },
  listProviders: { providers: [] },
  listModels: { models: [], unavailable: [] },
  sessionUsage: { input_tokens: 0, output_tokens: 0 },
};

/** Serializes turn events as the SSE record stream the router emits. */
export function sse(events: readonly TurnEvent[]): string {
  return events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join('');
}

/** Resolves a method and path to its logical {@link Endpoint}, if any. */
function routeOf(method: string, pathname: string): Endpoint | undefined {
  if (pathname === '/status') {
    return 'status';
  }
  if (pathname === '/api/v1/auth/bootstrap') {
    return 'bootstrap';
  }
  if (pathname === '/api/v1/auth/sign-in') {
    return 'signIn';
  }
  if (pathname === '/api/v1/auth/sign-out') {
    return 'signOut';
  }
  if (pathname === '/api/v1/auth/me') {
    return 'me';
  }
  if (pathname === '/api/v1/llm/providers') {
    return 'listProviders';
  }
  if (pathname === '/api/v1/llm/models') {
    return 'listModels';
  }
  if (pathname === '/api/v1/sessions') {
    return method === 'POST' ? 'createSession' : 'listSessions';
  }
  if (pathname.endsWith('/execute')) {
    return 'execute';
  }
  if (pathname.endsWith('/continue')) {
    return 'continueRun';
  }
  if (pathname.endsWith('/preview')) {
    return 'preview';
  }
  if (pathname.endsWith('/traces')) {
    return 'traces';
  }
  if (pathname.endsWith('/usage')) {
    return 'sessionUsage';
  }
  if (pathname.startsWith('/api/v1/sessions/')) {
    if (method === 'PUT') {
      return 'updateSession';
    }
    if (method === 'DELETE') {
      return 'deleteSession';
    }
    return 'getSession';
  }
  return undefined;
}

/** Maps an endpoint to its default {@link MockResponse}. */
function defaultResponse(endpoint: Endpoint): MockResponse {
  switch (endpoint) {
    case 'execute':
    case 'continueRun':
      return { json: defaults.turn };
    default:
      return { json: defaults[endpoint] };
  }
}

/** Builds a `Response` from a {@link MockResponse}. */
function toResponse(mock: MockResponse): Response {
  const status = mock.status ?? 200;
  if (mock.sse !== undefined) {
    return new Response(mock.sse, {
      status,
      headers: { 'content-type': 'text/event-stream' },
    });
  }
  if (mock.text !== undefined) {
    return new Response(mock.text, { status, headers: { 'content-type': 'text/plain' } });
  }
  return new Response(JSON.stringify(mock.json ?? {}), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/**
 * An in-memory smista-router that satisfies every endpoint over a `FetchLike`.
 *
 * It records each request for assertions and answers with default bodies, which
 * a test may override per endpoint to exercise error and streaming paths.
 */
export class MockRouter {
  readonly requests: RecordedRequest[] = [];
  private readonly overrides = new Map<Endpoint, MockResponse>();

  /** Overrides the response for `endpoint`, returning the router for chaining. */
  respond(endpoint: Endpoint, response: MockResponse): this {
    this.overrides.set(endpoint, response);
    return this;
  }

  /** The `FetchLike` to hand to a `FetchClient`. */
  get fetch(): FetchLike {
    return (input, init) => this.handle(input, init);
  }

  /** Reads a header off the first recorded request whose path ends with `suffix`. */
  headerOf(suffix: string, name: string): string | null {
    const request = this.requests.find((recorded) => recorded.url.pathname.endsWith(suffix));
    return request?.headers.get(name) ?? null;
  }

  private async handle(input: string | URL, init?: RequestInit): Promise<Response> {
    const url = new URL(input.toString());
    const method = (init?.method ?? 'GET').toUpperCase();
    const headers = new Headers(init?.headers);
    const body = typeof init?.body === 'string' ? init.body : '';
    this.requests.push({ method, url, headers, body });

    const endpoint = routeOf(method, url.pathname);
    if (endpoint === undefined) {
      return new Response('not found', { status: 404 });
    }
    return toResponse(this.overrides.get(endpoint) ?? defaultResponse(endpoint));
  }
}
