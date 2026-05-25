import { describe, expect, it } from 'vitest';
import { SmistaClient } from '../src/index.js';

describe('SmistaClient', () => {
  it('exposes the configured router URL', () => {
    const client = new SmistaClient({ routerUrl: 'http://127.0.0.1:7331' });
    expect(client.routerUrl).toBe('http://127.0.0.1:7331');
  });

  it('is not authenticated without a token', () => {
    const client = new SmistaClient({ routerUrl: 'http://127.0.0.1:7331' });
    expect(client.authenticated).toBe(false);
  });

  it('is authenticated when a token is provided', () => {
    const client = new SmistaClient({ routerUrl: 'http://127.0.0.1:7331', token: 'st_test' });
    expect(client.authenticated).toBe(true);
  });
});
