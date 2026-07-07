import { describe, expect, it } from 'vitest';
import type { ContinueRequest } from '../src/bindings/ContinueRequest.js';
import type { ExecuteRequest } from '../src/bindings/ExecuteRequest.js';
import type { TurnEvent } from '../src/bindings/TurnEvent.js';
import { ProviderCredentials } from '../src/credentials.js';
import { isSmistaError, SmistaError } from '../src/error.js';
import { SmistaClient } from '../src/smista-client.js';
import { defaults, MockRouter, sse } from './mock-router.js';

const SESSION_ID = '00000000-0000-0000-0000-000000000000';

/** A representative request body for the turn endpoints. */
function executeRequest(): ExecuteRequest {
  return {
    input: { text: 'hello' },
    workspace: { root: '/tmp', referenced_paths: [] },
    policy: {
      version: 1,
      source: 'merged',
      classification: { default_intent: 'chat', rules: [] },
      routing: { rules: [], default: null },
      tools: { permissions: {} },
      privacy: { restricted_paths: [], remote: { blocked_paths: [] }, local: {} },
    },
    local_preferences: { auto_apply: false, local_only: false, no_network: false },
    attachments: { files: [], instructions: [], invoked_skills: [], available_skills: [] },
  };
}

/** The data-less continuation that breaks an in-flight run. */
const BREAK: ContinueRequest = { type: 'break' };

/** Builds a client pointed at `router` with no credentials held. */
function clientFor(router: MockRouter): SmistaClient {
  return new SmistaClient({ baseUrl: 'http://router.test', fetch: router.fetch });
}

/** Builds a client already holding a session token, by signing in first. */
async function signedInClient(router: MockRouter): Promise<SmistaClient> {
  const client = clientFor(router);
  await client.signIn();
  return client;
}

describe('SmistaClient', () => {
  it('gets status without authentication', async () => {
    const router = new MockRouter();
    const status = await clientFor(router).status();
    expect(status).toEqual(defaults.status);
    expect(router.headerOf('/status', 'authorization')).toBeNull();
  });

  it('short-circuits authenticated calls without a token', async () => {
    const router = new MockRouter();
    const client = clientFor(router);

    await expect(client.listSessions()).rejects.toMatchObject({ kind: 'notAuthenticated' });
    // The guard returns before any request is sent.
    expect(router.requests).toHaveLength(0);
  });

  it('bootstraps and holds the minted api key for sign-in', async () => {
    const router = new MockRouter();
    const client = clientFor(router);

    const response = await client.bootstrap();
    expect(response).toEqual(defaults.bootstrap);

    // The held key is now presented on sign-in.
    await client.signIn();
    expect(router.headerOf('/auth/sign-in', 'x-smista-api-key')).toBe(defaults.bootstrap.api_key);
  });

  it('signs in and authenticates subsequent calls with a bearer token', async () => {
    const router = new MockRouter();
    const client = await signedInClient(router);

    await client.listSessions();
    const authorization = router.headerOf('/sessions', 'authorization');
    expect(authorization).toBe(`Bearer ${defaults.signIn.token}`);
  });

  it('signs out and clears the held token', async () => {
    const router = new MockRouter();
    const client = await signedInClient(router);

    const response = await client.signOut();
    expect(response.revoked).toBe(true);

    // With the token cleared, the next authenticated call short-circuits.
    await expect(client.listSessions()).rejects.toMatchObject({ kind: 'notAuthenticated' });
  });

  it('maps a non-success response to an api error paired with the status', async () => {
    const router = new MockRouter().respond('listSessions', {
      status: 401,
      json: { error: { code: 'invalid_token', message: 'bad token' } },
    });
    const client = await signedInClient(router);

    const error = await client.listSessions().catch((caught: unknown) => caught);
    expect(isSmistaError(error)).toBe(true);
    expect(error).toMatchObject({ kind: 'api', status: 401, code: 'invalid_token' });
  });

  it('maps a malformed body to a decode error', async () => {
    const router = new MockRouter().respond('listSessions', { text: 'not json' });
    const client = await signedInClient(router);

    await expect(client.listSessions()).rejects.toMatchObject({ kind: 'decode' });
  });

  it('forwards provider credentials on execute', async () => {
    const router = new MockRouter();
    const client = new SmistaClient({
      baseUrl: 'http://router.test',
      fetch: router.fetch,
      providerCredentials: ProviderCredentials.empty().withProvider('anthropic', 'sk-ant'),
    });
    await client.signIn();

    await client.execute(SESSION_ID, executeRequest());
    expect(router.headerOf('/execute', 'x-smista-provider-anthropic-api-key')).toBe('sk-ant');
  });

  it('forwards provider credentials on list models', async () => {
    const router = new MockRouter();
    const client = new SmistaClient({
      baseUrl: 'http://router.test',
      fetch: router.fetch,
      providerCredentials: ProviderCredentials.empty().withProvider('anthropic', 'sk-ant'),
    });
    await client.signIn();

    await client.listModels();
    expect(router.headerOf('/llm/models', 'x-smista-provider-anthropic-api-key')).toBe('sk-ant');
  });

  it('does not forward provider credentials on preview', async () => {
    const router = new MockRouter();
    const client = new SmistaClient({
      baseUrl: 'http://router.test',
      fetch: router.fetch,
      providerCredentials: ProviderCredentials.empty().withProvider('anthropic', 'sk-ant'),
    });
    await client.signIn();

    await client.preview(SESSION_ID, executeRequest());
    expect(router.headerOf('/preview', 'x-smista-provider-anthropic-api-key')).toBeNull();
  });

  it('sends pagination as query parameters for traces', async () => {
    const router = new MockRouter();
    const client = await signedInClient(router);

    await client.getSessionTraces(SESSION_ID, 10, 5);
    const request = router.requests.find((recorded) => recorded.url.pathname.endsWith('/traces'));
    expect(request?.url.searchParams.get('limit')).toBe('10');
    expect(request?.url.searchParams.get('offset')).toBe('5');
  });

  it('sends scope and title as query parameters for sessions', async () => {
    const router = new MockRouter();
    const client = await signedInClient(router);

    await client.listSessions('/work/api', 'refactor');
    const request = router.requests.find(
      (recorded) => recorded.url.pathname === '/api/v1/sessions',
    );
    expect(request?.url.searchParams.get('scope')).toBe('/work/api');
    expect(request?.url.searchParams.get('title')).toBe('refactor');
  });

  it('never places a credential in the query string', async () => {
    const router = new MockRouter();
    const client = new SmistaClient({
      baseUrl: 'http://router.test',
      fetch: router.fetch,
      providerCredentials: ProviderCredentials.empty().withProvider('anthropic', 'sk-ant'),
    });
    await client.signIn();
    await client.execute(SESSION_ID, executeRequest());

    for (const request of router.requests) {
      const query = request.url.search.toLowerCase();
      expect(query).not.toContain('sk-ant');
      expect(query).not.toContain('api');
    }
  });

  it('round-trips every authenticated endpoint', async () => {
    const router = new MockRouter();
    const client = await signedInClient(router);

    expect(await client.me()).toEqual(defaults.me);
    expect(await client.createSession({ title: 't' })).toEqual(defaults.createSession);
    expect(await client.listSessions()).toEqual(defaults.listSessions);
    expect(await client.getSession(SESSION_ID)).toEqual(defaults.getSession);
    expect(await client.updateSession(SESSION_ID, { title: 't' })).toEqual(defaults.updateSession);
    expect(await client.deleteSession(SESSION_ID)).toEqual(defaults.deleteSession);
    expect(await client.execute(SESSION_ID, executeRequest())).toEqual(defaults.turn);
    expect(await client.continueRun(SESSION_ID, BREAK)).toEqual(defaults.turn);
    expect(await client.preview(SESSION_ID, executeRequest())).toEqual(defaults.preview);
    expect(await client.getSessionTraces(SESSION_ID)).toEqual(defaults.traces);
    expect(await client.listProviders()).toEqual(defaults.listProviders);
    expect(await client.listModels()).toEqual(defaults.listModels);
    expect(await client.sessionUsage(SESSION_ID)).toEqual(defaults.sessionUsage);
  });

  it('streams a turn until the terminal event', async () => {
    const events: TurnEvent[] = [
      { type: 'text_delta', delta: 'Hello' },
      { type: 'turn_end', ...defaults.turn },
    ];
    const router = new MockRouter().respond('execute', { sse: sse(events) });
    const client = await signedInClient(router);

    const collected: TurnEvent[] = [];
    for await (const event of await client.streamExecute(SESSION_ID, executeRequest())) {
      collected.push(event);
    }

    expect(collected).toHaveLength(2);
    expect(collected[0]?.type).toBe('text_delta');
    expect(collected[1]?.type).toBe('turn_end');
  });

  it('streams a continued turn', async () => {
    const events: TurnEvent[] = [{ type: 'turn_end', ...defaults.turn }];
    const router = new MockRouter().respond('continueRun', { sse: sse(events) });
    const client = await signedInClient(router);

    const collected: TurnEvent[] = [];
    for await (const event of await client.streamContinue(SESSION_ID, BREAK)) {
      collected.push(event);
    }
    expect(collected).toHaveLength(1);
    expect(collected[0]?.type).toBe('turn_end');
  });

  it('surfaces a failed stream open as an api error before the stream opens', async () => {
    const router = new MockRouter().respond('execute', {
      status: 404,
      json: { error: { code: 'session_not_found', message: 'no such session' } },
    });
    const client = await signedInClient(router);

    const error = await client
      .streamExecute(SESSION_ID, executeRequest())
      .catch((caught: unknown) => caught);
    expect(error).toMatchObject({ kind: 'api', code: 'session_not_found' });
  });

  it('surfaces a malformed event as a decode error while iterating', async () => {
    const router = new MockRouter().respond('execute', { sse: 'data: not json\n\n' });
    const client = await signedInClient(router);

    const iterate = async (): Promise<void> => {
      for await (const _event of await client.streamExecute(SESSION_ID, executeRequest())) {
        // Draining the stream forces the malformed record to be parsed.
      }
    };
    await expect(iterate()).rejects.toBeInstanceOf(SmistaError);
    await expect(iterate()).rejects.toMatchObject({ kind: 'decode' });
  });
});
