# Regenerate the OpenAPI schema at docs/api/openapi.json from the Rust types
[group('openapi')]
gen_openapi:
    cargo test -p smista-router --features openapi gen_openapi_schema

# Fail if the committed OpenAPI schema is out of date with the Rust types
[group('openapi')]
check_openapi:
    cargo test -p smista-router --features openapi gen_openapi_schema
    git diff --exit-code -- docs/api/openapi.json
