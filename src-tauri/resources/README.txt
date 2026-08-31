The `consortium` CLI is staged into this directory at build time by
scripts/bundle-cli.mjs, so the packaged app can install it for the user.

The binary itself is not committed — it is produced by `npm run bundle:cli`,
which tauri.conf.json runs via beforeBuildCommand. This file exists so the
resources glob resolves on a clean checkout, before the CLI has been built.
