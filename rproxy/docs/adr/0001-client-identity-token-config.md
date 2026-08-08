# Client identity token configuration

rproxy will model authentication around long-lived client identities, each with a client token. The first multi-token server configuration uses `[[clients]]` entries with `id` and plaintext `token`, loaded only at server startup; `--token` remains as a legacy single-token mode and is mutually exclusive with `--config`. This keeps the first step deployable while preserving a stable place to add token hashing, rotation, and per-client permissions later.
