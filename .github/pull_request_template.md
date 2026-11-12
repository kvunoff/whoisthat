## Summary

<!-- What changed and why. Link the issue or plan if there is one. -->

## Documentation

<!-- Required for any user-visible change. See docs/developers/documentation.md. -->

- [ ] User-visible behavior changed; canonical page updated: `...`
- [ ] New or changed CLI command / config key / IPC command / doctor category
- [ ] `since` / `status` / `changed_in` updated when applicable
- [ ] Breaking change: migration section or deprecation banner
- [ ] Screenshots updated if the TUI changed
- [ ] Old URLs redirected if a page moved or was removed

## Checks

- [ ] `./scripts/check-docs.sh`
- [ ] `./scripts/check-docs.test.sh`
- [ ] `./scripts/check-versions.sh`
- [ ] `npm run docs:build` (docs changes)
- [ ] Relevant `make` targets (code changes)
