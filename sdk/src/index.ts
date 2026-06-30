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

export type { ISmistaClient } from './client.js';
