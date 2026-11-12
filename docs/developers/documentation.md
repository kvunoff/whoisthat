---
title: "Documentation Guide"
description: "How WhoisThat documentation is organized, written, reviewed, and kept current."
audience: "developer"
type: "reference"
---

# Documentation Guide

This page is the contract for WhoisThat documentation. It explains what belongs
where, how a page is structured, who owns a fact, and how pages change over
time. Follow it for every new or rewritten page.

## Who this is for

- Contributors adding a feature that users can see.
- Maintainers reviewing documentation changes.
- Anyone rewriting a page that drifted from the code.

The public site is built from `docs/` with VitePress. It is published at
`https://docs.tuqise.com` from the `main` branch only. Older versions are not
published; git history and GitHub Releases are the archive.

## Content model

Use four content types. Pick the type from the reader's question, not from the
feature name.

| Type | Reader's question | Typical place | Required shape |
| :--- | :--- | :--- | :--- |
| tutorial | "Walk me through it" | Getting Started | goal → prerequisites → steps → check → next |
| how-to | "How do I do X?" | Usage, Integrations, Developers | prerequisites → steps → check → limits |
| reference | "What does Y mean?" | Reference | contract/table → examples → notes |
| explanation | "Why and how does it work?" | Networking, Architecture | context → diagram → walkthrough → failure modes → source map |

Sections are organized by the reader's journey and the product area. A page's
`type` is metadata about its intent and does not have to match its section:
`guide/platforms.md` is a how-to, `usage/settings.md` is a reference, and
`integrations/deeplinks.md` is a reference.

### When a page exists

1. One page = one audience + one intent + one set of canonical facts.
2. Split a page over roughly 1500 words, or when it mixes audiences.
3. Merge pages under roughly 400 words when they share the same intent.
4. Do not create a page just because a feature exists. A page serves a task or
   owns a contract.
5. Keep at most two levels of navigation below a section.
6. A fact has one home. Other pages link to it instead of repeating it.

Audiences: `user`, `operator`, `integrator`, `developer`. A page has exactly one
primary audience. Do not use `all` or `user/operator`.

A "hub" is a sidebar section, not a separate page. Entry points are the landing
page and the top navigation.

## Target map

The documentation is being restructured from feature-shaped pages into this
map. The source of truth for the current tree is `docs/.vitepress/config.mts`.

```text
docs/
├── index.md
├── guide/          installation, quick-start, platforms, maintenance
├── usage/          navigation, profiles, routing, split-tunnel, kill-switch,
│                   traffic, logs, settings, recipes
├── networking/     tun-mode, dns, routing-rules, happ-routing,
│                   json-subscriptions, protocols
├── integrations/   desktop-widgets, gnome-extension, deeplinks, hwid
├── reference/      cli, configuration, status-json, protocol, happ-format,
│                   routing-precedence, doctor, troubleshooting, glossary
├── architecture/   overview, rust-client, go-core, ipc-flow,
│                   data-and-encryption, networking, routing-engine,
│                   xray-runtime, security
└── developers/     index, documentation, build, repository, testing,
                    debugging, extending, releasing
```

## Canonical owners

Each fact is documented in one place. If a fact changes, edit its owner and
remove copies elsewhere.

| Topic | Owner |
| :--- | :--- |
| CLI flags, commands, exit codes, CLI JSON output | `reference/cli.md` |
| `config.toml`, `config.json`, keys, defaults, config/data paths | `reference/configuration.md` |
| `status --json` schema | `reference/status-json.md` |
| IPC commands, notifications, warning/error keys | `reference/protocol.md` |
| Happ URI and JSON format | `reference/happ-format.md` |
| Routing first-match order | `reference/routing-precedence.md` |
| Doctor categories and JSON report | `reference/doctor.md` |
| Keybindings and mouse actions | `usage/navigation.md` |
| Settings screen behavior | `usage/settings.md` |
| Log format, rotation, and log-file paths | `usage/logs.md` |
| Capabilities, setcap, ambient capabilities | `networking/tun-mode.md` |
| Upgrade, downgrade, uninstall, backup, restore, repair | `guide/maintenance.md` |
| Init systems, autostart, lifecycle, runtime directory | `guide/platforms.md` |
| Deep links (`wit://`) | `integrations/deeplinks.md` |
| HWID headers | `integrations/hwid.md` |
| Update checks and `WHOISTHAT_NO_UPDATE_CHECK` | `reference/configuration.md` |
| User-facing TUN, DNS, split, and kill-switch behavior | `networking/*` |
| Kernel networking internals | `architecture/networking.md` |
| Routing compilation, Happ/JSON, and reload | `architecture/routing-engine.md` |
| Security model and threat model | `architecture/security.md` |
| Rust client internals | `architecture/rust-client.md` |
| Go core internals | `architecture/go-core.md` |
| Build, test, debug, release | `developers/*` |

`reference/configuration.md` owns config and data paths; `usage/logs.md` owns log
paths only.

## Frontmatter

Every page starts with YAML frontmatter:

```yaml
---
title: "Installation"
description: "Install WhoisThat from the install script, packages, AUR, or source."
audience: "user"        # user | operator | integrator | developer
type: "tutorial"        # landing | tutorial | how-to | reference | explanation
status: "current"       # current | experimental | legacy | deprecated
since: "0.19.0"         # optional
changed_in: "0.21.0"    # optional
deprecated_in: "0.22.0" # required when status is deprecated
removed_in: "0.23.0"    # required when status is deprecated
replaced_by: "/usage/split-tunnel" # required when status is deprecated
review_by: "2027-03-01" # optional, for external formats such as Happ or Xray
---
```

Rules:

- `title` and `description` are required. The description is one sentence.
- `audience` and `type` are required and must use the values above.
- `status` is optional; a missing value means `current`.
- `deprecated` requires `deprecated_in`, `removed_in`, and `replaced_by`.
  Use `replaced_by: "none"` only when there is genuinely no replacement.
- `replaced_by` is an internal path that exists, or `none`.
- `since`, `changed_in`, `deprecated_in`, and `removed_in` use the client
  version from `Cargo.toml` (`0.19.0`), not the core or protocol version.
- `review_by` is a date. A monthly audit reports overdue pages; it does not
  block pull requests.
- Frontmatter is the first block of the file, before any HTML or heading.

`index.md` uses `type: "landing"` and may keep an HTML `<h1>` inside its landing
markup.

## Page templates

### Tutorial

```md
---
title: "..."
description: "..."
audience: "user"
type: "tutorial"
---

# ...

One paragraph: what the reader will have at the end.

## Before you start

## Steps

## Check it worked

## Next
```

### How-to

```md
# ...

One paragraph: the task and when to use it.

## Prerequisites

## Steps

## Verify

## Limits and caveats

## See also
```

### Reference

```md
# ...

One paragraph: what contract this page covers.

## Contract

## Examples

## Notes

## See also
```

### Explanation

```md
# ...

One paragraph: the system or behavior being explained.

## Context

## How it works

## Failure modes

## Source map

## See also
```

## Writing rules

- Address the reader as "you". Use present tense and active voice.
- Lead with the task or the contract; do not bury it under history.
- One idea per paragraph. Prefer short sentences.
- Do not sell the product inside reference or troubleshooting pages.
- Use one term for one concept. If a synonym is common, add it to the glossary.
- Do not repeat a canonical table. Link to its owner instead.
- Put commands in fenced blocks with a language. Use `text` for diagrams.
- Show the canonical command first. Put legacy aliases in a separate section.
- Use `whoisthat` and `whoisthat-core` exactly; do not invent short names.
- Redact tokens, UUIDs, and provider URLs in examples.

## Structure and formatting

- Exactly one H1 per page. The page title comes from the frontmatter or the H1,
  not both.
- Do not number headings manually (`## 1.`). Use descriptive headings and stable
  anchors.
- Keep heading depth to H3 unless a reference table genuinely needs H4.
- Put a 2–5 sentence introduction before the first H2.
- End every page with `## See also` and 2–5 links.
- Use VitePress containers intentionally:
  `::: info` for context, `::: tip` for shortcuts, `::: warning` for limits,
  `::: danger` for destructive actions.
- Prefer tables for contracts and lists for steps.
- Add alt text to screenshots and update them in the same change as the UI.
- Mermaid diagrams must have a text explanation nearby.

## Terminology

Canonical spellings:

| Term | Use |
| :--- | :--- |
| WhoisThat | product name, one word |
| core, `whoisthat-core` | the Go daemon |
| client, TUI | the Rust frontend |
| runner, `whoisthat-runner` | the per-user split-tunnel helper |
| profile | one server entry |
| group | a subscription/group of profiles |
| subscription | the provider source for a group |
| routing profile | a Happ or JSON routing configuration |
| TUN mode | system-wide routing through `whoisthattun` |
| proxy mode | local SOCKS5/HTTP proxy |
| split tunneling | per-application routing; two words |
| kill-switch | hyphenated |
| Happ | provider routing format and profiles |
| FakeDNS | Xray fake-IP DNS mode |
| doctor | `whoisthat doctor` diagnostics |
| IPC | the client-core protocol |

Avoid "Kill Switch", "Split Tunneling", "fake DNS", and "the daemon" when you
mean the core. The glossary in `reference/glossary.md` is the shared vocabulary.

## Versions and sources of truth

- Client version: `Cargo.toml`.
- Core version: `core/core/lib/version/version.go`.
- Protocol version: `core/core/lib/version/protocol.txt`.
- Pinned Xray version: `core/core/lib/xraymgr/version.go`.
- CLI surface: `src/cli/args.rs`.
- IPC commands and notifications: `core/core/lib/TCPServer/dispatch.go` and
  `src/core_client/dispatch.rs`.
- Config keys: `core/core/lib/AppConfig/config.go`.

Do not hard-code versions in prose unless a script checks them. When you mention
a version, say whether it is the client, core, protocol, or Xray.

## Links and anchors

- Link to the canonical owner, not to a copy.
- Link to a page, not to a numbered heading. Headings change; anchors break.
- Use root-relative links (`/reference/cli`) inside the docs.
- Do not link to pages that do not exist yet. Use code spans for planned paths.
- Every moved page keeps a redirect from its old URL.
- Never reuse an old URL for a different topic.

## Lifecycle

Pages move through these states:

| Status | Meaning | Action |
| :--- | :--- | :--- |
| `current` | current behavior | normal page |
| `experimental` | may change | banner, link to the tracking issue |
| `legacy` | still supported, not recommended | banner, link to the replacement |
| `deprecated` | will be removed | banner with `removed_in` and `replaced_by` |
| removed | no page | redirect to replacement/hub and a release note |

Rules:

- A deprecated page requires `deprecated_in`, `removed_in`, and `replaced_by`.
- Deprecation window: at least one minor release for CLI, config, and behavior;
  one protocol version for IPC. Code should warn no later than the docs banner.
- After `removed_in`, delete the page, add a redirect, and mention the removal in
  the release notes.
- `legacy` is for behavior that is still supported but not recommended, such as
  old CLI aliases. It has no removal date.
- A rewritten but still supported behavior (for example split tunneling) stays
  `current`. Add `## Changed in vX` and, for breaking changes,
  `## Migration from vX`.

Deprecation banner:

```md
::: warning Deprecated in v0.23
Use `whoisthat run` instead. This mode is removed in v0.24.
:::
```

Old versions are not published on the site. Git history and release tags are the
archive.

## Ownership and content map

Keep a living table of areas, canonical pages, and owners in this guide:

| Area | Canonical pages | Owner |
| :--- | :--- | :--- |
| split tunnel | `usage/split-tunnel.md`, `architecture/networking.md` | networking |
| routing | `usage/routing.md`, `networking/routing-rules.md`, `reference/routing-precedence.md`, `architecture/routing-engine.md` | routing |
| CLI and config | `reference/cli.md`, `reference/configuration.md` | core |

Update the table in the same change that adds, removes, or moves a page. When
there is more than one maintainer, add `CODEOWNERS` for these areas. Until then,
the table is enough.

## Adding, moving, and deleting pages

1. Decide whether the content needs a new page or a section in an existing one.
2. Choose the type, audience, and canonical owner.
3. Create the page with the full frontmatter.
4. Add it to `docs/.vitepress/config.mts` and link it from a hub.
5. If it replaces a page, add a redirect from the old URL.
6. If it changes a fact, remove the old copy.
7. Run the checks below.

When moving a page, use `git mv` so `lastUpdated` and blame survive. Add a
redirect stub in `docs/public/` for the old URL.

When deleting a page, redirect to the replacement or nearest hub. Do not leave a
404 and do not leave the old URL empty.

## Review checklist

Before requesting review:

- [ ] The page has one audience, one intent, and one owner.
- [ ] Frontmatter is valid and `status` is correct.
- [ ] Commands, paths, defaults, and versions match the code.
- [ ] The canonical fact is not duplicated elsewhere.
- [ ] All links and anchors resolve.
- [ ] One H1, no manual heading numbers.
- [ ] Code fences have a language.
- [ ] Screenshots are current and have alt text.
- [ ] `## See also` has 2–5 useful links.
- [ ] Moved or deleted pages have redirects.
- [ ] `status: deprecated` has `removed_in` and `replaced_by`.
- [ ] The content map is updated if the page was added or removed.

## Release checklist

Before a release:

- [ ] `./scripts/check-docs.sh`
- [ ] `./scripts/check-docs.test.sh`
- [ ] `./scripts/check-versions.sh`
- [ ] `npm run docs:build`
- [ ] Review `status: deprecated` pages and remove those past `removed_in`.
- [ ] Verify redirects for pages removed in this release.
- [ ] Add docs links to the release notes.
- [ ] Smoke-test old URLs after deployment.

## Automation

- `./scripts/check-docs.sh` checks IPC names, config keys, CLI nouns, doctor
  categories, links, versions, frontmatter, sidebar coverage, and lifecycle
  metadata.
- `./scripts/check-docs.test.sh` tests the checker itself.
- `./scripts/check-versions.sh` keeps version strings in sync.
- `npm run docs:build` builds the site.
- The `docs` job in CI runs the checks and the build on every pull request.
- A monthly `docs-audit` workflow will report orphans, overdue deprecations,
  stale `review_by` dates, and broken external links as an issue, not as a
  failed pull request.

## What not to do

- Do not create a page for every feature when a section is enough.
- Do not duplicate canonical tables "for convenience".
- Do not publish a museum of old versions.
- Do not leave `deprecated` without a removal version and a replacement.
- Do not add VitePress plugins or npm dependencies just for redirects.
- Do not commit `docs/.vitepress/dist` or `docs/.vitepress/cache`.
- Do not generate documentation from code yet; a generated CLI reference is a
  future option only when the surface grows too large to maintain by hand.

## See also

- [For Developers](/developers/)
- [Glossary](/reference/glossary)
- [Build & Run](/developers/build)
- [System Architecture Overview](/architecture/overview)
