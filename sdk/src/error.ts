import type { ApiError } from './bindings/ApiError.js';
import type { JsonValue } from './bindings/serde_json/JsonValue.js';

/**
 * Discriminates the failure modes a {@link SmistaError} can represent.
 *
 * - `api`: the router answered with a structured {@link ApiError} body.
 * - `decode`: a response body could not be parsed as the expected shape.
 * - `transport`: the request never produced a response (network, abort, …).
 * - `notAuthenticated`: an authenticated call was made before signing in.
 * - `invalidApiKey`: the held API key was rejected when exchanging it.
 */
export type SmistaErrorKind = 'api' | 'decode' | 'transport' | 'notAuthenticated' | 'invalidApiKey';

/** Extracts a human-readable message from an arbitrary thrown value. */
function causeMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * The single error type every {@link ISmistaClient} method rejects with.
 *
 * It is a real `Error`, so it carries a stack and works with `instanceof`, and
 * its `kind` discriminates why the call failed. Construct instances through the
 * static factories rather than the constructor, which keeps each variant's
 * fields consistent.
 */
export class SmistaError extends Error {
  /** Why the call failed. */
  readonly kind: SmistaErrorKind;
  /** HTTP status code; present only for `api` errors. */
  readonly status?: number;
  /** Stable machine-readable router error code; present only for `api` errors. */
  readonly code?: string;
  /** Structured router context; present only for `api` errors that carry it. */
  readonly details?: JsonValue;

  private constructor(
    kind: SmistaErrorKind,
    message: string,
    options?: {
      readonly status?: number;
      readonly code?: string;
      readonly details?: JsonValue;
      readonly cause?: unknown;
    },
  ) {
    super(message, options?.cause === undefined ? undefined : { cause: options.cause });
    this.name = 'SmistaError';
    this.kind = kind;
    this.status = options?.status;
    this.code = options?.code;
    this.details = options?.details;
    Object.setPrototypeOf(this, SmistaError.prototype);
  }

  /** Builds an `api` error from the status and decoded {@link ApiError} body. */
  static api(status: number, body: ApiError): SmistaError {
    const { code, message, details } = body.error;
    return new SmistaError('api', `API error (status ${status}, code ${code}): ${message}`, {
      status,
      code,
      details,
    });
  }

  /** Builds a `decode` error from the offending body text and its cause. */
  static decode(body: string, cause: unknown): SmistaError {
    return new SmistaError('decode', `failed to decode response body: ${body}`, { cause });
  }

  /** Builds a `transport` error from the underlying network failure. */
  static transport(cause: unknown): SmistaError {
    return new SmistaError('transport', `transport error: ${causeMessage(cause)}`, { cause });
  }

  /** Builds the error raised when an authenticated call runs before sign-in. */
  static notAuthenticated(): SmistaError {
    return new SmistaError('notAuthenticated', 'client is not authenticated; sign in first');
  }

  /** Builds the error raised when the held API key is rejected at sign-in. */
  static invalidApiKey(): SmistaError {
    return new SmistaError('invalidApiKey', 'the configured API key is invalid');
  }
}

/** Type guard narrowing an unknown thrown value to a {@link SmistaError}. */
export function isSmistaError(value: unknown): value is SmistaError {
  return value instanceof SmistaError;
}
