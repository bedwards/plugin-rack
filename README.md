# plugin-rack

A mixing-console plugin rack for Bitwig and other VST3 hosts. Written in Rust on [`nih_plug`](https://github.com/robbert-vdh/nih-plug).

## Why

- Host nested VST3/CLAP plugins inside one plugin instance.
- Show each nested plugin's GUI inside the rack, resizable and scalable.
- Surface nested plugin parameters to the DAW so host modulators (e.g. Bitwig's LFOs, envelopes, macros) can target them.
- Three fluid layout modes: single row, vertical stack, row-with-wrap.
- Respect the host's buffer size and sample rate; keep CPU overhead near zero.
- Link instances across tracks so two tracks feel like one mixing console.

## Status

Under active construction. See `SPEC.md` for the current technical spec, `DEV_WORKFLOW.md` for build/dev commands, `research/` for deep research that informed the design.

## Building

```
cargo xtask bundle rack-plugin --release
```

Output in `target/bundled/`.

## Verifying

`pluginrack verify` runs fmt, clippy, tests, bundle, then two validators: `pluginval` on the VST3 bundle and `clap-validator` (free-audio/clap-validator) on the CLAP bundle. pluginval is VST3-only; clap-validator fills the CLAP side.

## License

TBD.
