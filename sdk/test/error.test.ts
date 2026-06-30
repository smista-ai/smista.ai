import { describe, expect, it } from 'vitest';
import type { ApiError } from '../src/bindings/ApiError.js';
import { isSmistaError, SmistaError } from '../src/error.js';

describe('SmistaError', () => {
  it('builds an api error from a decoded body', () => {
    const body: ApiError = {
      error: { code: 'invalid_token', message: 'The session token is unknown.' },
    };
    const error = SmistaError.api(401, body);
    expect(error.kind).toBe('api');
    expect(error.status).toBe(401);
    expect(error.code).toBe('invalid_token');
    expect(error.message).toBe(
      'API error (status 401, code invalid_token): The session token is unknown.',
    );
  });

  it('carries api error details', () => {
    const body: ApiError = {
      error: { code: 'x', message: 'y', details: { provider: 'anthropic' } },
    };
    expect(SmistaError.api(400, body).details).toEqual({ provider: 'anthropic' });
  });

  it('builds a decode error with a cause', () => {
    const cause = new Error('boom');
    const error = SmistaError.decode('not json', cause);
    expect(error.kind).toBe('decode');
    expect(error.message).toBe('failed to decode response body: not json');
    expect(error.cause).toBe(cause);
  });

  it('builds a transport error from a cause', () => {
    const error = SmistaError.transport(new Error('refused'));
    expect(error.kind).toBe('transport');
    expect(error.message).toBe('transport error: refused');
  });

  it('builds the not-authenticated error', () => {
    const error = SmistaError.notAuthenticated();
    expect(error.kind).toBe('notAuthenticated');
    expect(error.message).toBe('client is not authenticated; sign in first');
  });

  it('is recognized by the type guard and is an Error', () => {
    const error = SmistaError.invalidApiKey();
    expect(error.kind).toBe('invalidApiKey');
    expect(error).toBeInstanceOf(Error);
    expect(isSmistaError(error)).toBe(true);
    expect(isSmistaError(new Error('plain'))).toBe(false);
    expect(isSmistaError(null)).toBe(false);
  });
});
