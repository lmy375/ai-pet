# Release

`.github/workflows/release.yml` builds and publishes everything.

**Cut a release:** push a tag — `git tag v1.0.3 && git push origin v1.0.3`.
**Dry run:** Actions → Release → *Run workflow*. Leave `tag` empty to only build
artifacts (nothing is published); fill it in to publish that tag from the chosen
branch.

Assets: `pet-<version>-macos-{arm64,x64}.dmg`,
`pet-cli-<version>-{macos-arm64,macos-x64,linux-x64,linux-x64-musl}.tar.gz`,
`SHA256SUMS.txt`.

## Secrets (all optional)

- `LIVE2D_ASSETS_URL` — zip that unpacks into `public/` (i.e. contains
  `lib/live2dcubismcore.min.js` and `models/…`). The Cubism SDK and models are
  copyrighted and gitignored, so **without this the released app has no pet** —
  it builds and runs, but Live2D fails to load.
- `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` — sign + notarize. Unset means
  the `.dmg` ships unsigned and users need
  `xattr -dr com.apple.quarantine /Applications/pet.app` (the release notes say so).
