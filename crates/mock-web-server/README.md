# smista-mock-web-server

Unpublished test helper for running a local mock of the `smista-router` HTTP
API.

The crate is used by workspace tests that need a deterministic router stand-in
without starting the real router service. It serves schema-correct default
responses, lets tests override endpoint responses, records received requests,
and supports cancellation through `CancellationToken`.

## License

Licensed under the MIT License. See [LICENSE-MIT](../../LICENSE-MIT) for
details.
