import type { Provider } from './bindings/Provider.js';

/** Literal prefix of a provider credential header name. */
const HEADER_PREFIX = 'X-Smista-Provider-';
/** Literal suffix that closes a provider credential header name. */
const HEADER_SUFFIX = '-Api-Key';
/** Scheme tag an OpenAI-compatible provider renders with; dropped in headers. */
const OPENAI_COMPAT_PREFIX = 'openai-compat:';

/**
 * API keys for upstream model providers, forwarded to the router per request.
 *
 * Provider credentials are distinct from router authentication (the API key and
 * session token): they are the keys the router needs to call a remote model on
 * the caller's behalf. The client holds them and sends them on the methods that
 * can reach a model (`execute`, `continueRun`, `streamExecute`,
 * `streamContinue` and `listModels`), as `X-Smista-Provider-<provider>-Api-Key`
 * request headers; the router never persists them.
 *
 * Instances are immutable: {@link withProvider} returns a new set rather than
 * mutating in place.
 */
export class ProviderCredentials {
  private readonly keys: ReadonlyMap<Provider, string>;

  private constructor(keys: ReadonlyMap<Provider, string>) {
    this.keys = keys;
  }

  /** Creates an empty set of provider credentials. */
  static empty(): ProviderCredentials {
    return new ProviderCredentials(new Map());
  }

  /** Creates a set from provider-to-key entries. */
  static fromEntries(entries: Iterable<readonly [Provider, string]>): ProviderCredentials {
    return new ProviderCredentials(new Map(entries));
  }

  /** The number of providers with a held key. */
  get size(): number {
    return this.keys.size;
  }

  /** Returns a new set with `key` added or replaced for `provider`. */
  withProvider(provider: Provider, key: string): ProviderCredentials {
    const next = new Map(this.keys);
    next.set(provider, key);
    return new ProviderCredentials(next);
  }

  /**
   * Renders the credentials as a map of request header name to key.
   *
   * The header name is `X-Smista-Provider-<segment>-Api-Key`, where `segment`
   * is the provider's bare name. For an OpenAI-compatible instance rendered as
   * `openai-compat:<name>`, the `openai-compat:` tag is dropped, since a `:`
   * cannot appear in a header name and the router matches the bare instance
   * name directly.
   */
  headersMap(): Record<string, string> {
    const headers: Record<string, string> = {};
    for (const [provider, key] of this.keys) {
      const segment = provider.startsWith(OPENAI_COMPAT_PREFIX)
        ? provider.slice(OPENAI_COMPAT_PREFIX.length)
        : provider;
      headers[`${HEADER_PREFIX}${segment}${HEADER_SUFFIX}`] = key;
    }
    return headers;
  }
}
