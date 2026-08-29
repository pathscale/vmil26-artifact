# VMIL 2026 artifact

This repository contains the benchmark harness and recorded timing evidence for
“The Event Loop That Wasn’t: What Happens to a Reactive System When You Remove
JavaScript” (VMIL 2026).

The paper source and review history are intentionally not part of the artifact.
The implementation repositories and every evaluated dependency are identified
by full commit hash in [REVISIONS.md](REVISIONS.md).

## Reproduce

Install a current Rust toolchain and the WebAssembly target:

```console
rustup target add wasm32-unknown-unknown
cd bench
cargo test
cargo run --release -- results.json
cargo bench --bench boundary
```

`cargo test` checks the deterministic crossing and byte counts. The generated
`results.json` contains the paper tables and sweep summary. Timing results depend
on the machine and should only be compared within one run. The recorded VMIL
Criterion run is preserved in `evidence/criterion-run.log`; a validation report
generated from this extracted repository is in `evidence/results.json`.

## Related implementations

- [SolidRS](https://github.com/pathscale/SolidRS)
- [ps-blitz](https://github.com/pathscale/ps-blitz)

The artifact is licensed under Apache-2.0; see [LICENSE](LICENSE).
