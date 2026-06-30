/**
 * Client SDK for the smista-router HTTP API.
 *
 * `@smista-ai/sdk` is a thin, typed client over the router's REST API. It must
 * not reimplement routing, policy evaluation, provider selection or tool
 * mediation; that logic stays owned by smista-router.
 *
 * {@link SmistaClient} implements the {@link ISmistaClient} contract over the
 * platform `fetch`, covering every endpoint (auth, sessions, execute, stream,
 * preview, traces, usage, providers/models and status). Its request and
 * response types are the bindings generated from `smista-core`.
 */

export type { ISmistaClient } from './client.js';
export {
  DEFAULT_BASE_URL,
  DEFAULT_TIMEOUT_MS,
  type FetchLike,
  type SmistaClientConfig,
} from './config.js';
export { ProviderCredentials } from './credentials.js';
export { isSmistaError, SmistaError, type SmistaErrorKind } from './error.js';
export { SmistaClient } from './smista-client.js';
