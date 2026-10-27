# Lastdown anthology

Supporting plates for the Lastdown crate: the field, the reserve, the continuation, scouting, the InGauge account quota, the measured ledger, and the operator card.

The pages are static HTML, CSS, and JavaScript. The server is a separate Rust crate with no dependencies and no link to the library workspace.

## Run

From this checkout:

```bash
cargo run --offline --manifest-path website/server/Cargo.toml
```

Open `http://127.0.0.1:8741/`.

`ANTHOLOGY_PORT` changes the port. `ANTHOLOGY_BIND` changes the address. The default bind is `127.0.0.1`. `ANTHOLOGY_ROOT` points at another copy of `website/public`.

## Check

```bash
cargo test --offline --manifest-path website/server/Cargo.toml
```

The library benches stay on the Lastdown crate. This server does not run them.
