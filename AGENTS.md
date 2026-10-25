# flix

## Move notice

This project moved into the monorepo
`/Users/babalolaopeyemidaniel/Desktop/Workspace/opeyemi/babalola`
(`BabalolaBrainiac/babalola`, issue monorepo #108):

- Rust core (CLI, TUI, desktop web-UI host): `cli/flix-cli`.
- Gateway worker (live at gateway.babalola.dev): `apps/flix-gateway`.
- Svelte web UI: `apps/flix-web`.
- iOS bridge project: `apps/flix-ios`.

The pre-migration working state (13 dirty files, `ios/`, `src/ios.rs`,
`src/player/embedded.rs`) is preserved on this repo's branch
`backup/pre-migration-2026-10-25`. All other branches were verified
content-merged into `main` before the import. This folder is kept only until
the owner archives it. Do not build, test, or deploy from here; the release
pipeline (release.yml, tag-based) still runs from this repo until the owner
moves it.

## Verification (monorepo)

- Rust: from `cli/flix-cli`, `cargo fmt --check`,
  `cargo test --offline --all-features` (build apps/flix-web first and sync
  its `dist/` into `cli/flix-cli/web/dist/` for the embed test).
- Gateway: from `apps/flix-gateway`, `pnpm type-check && pnpm test`.
- Web: from `apps/flix-web`, `pnpm check && pnpm test && pnpm build`.
