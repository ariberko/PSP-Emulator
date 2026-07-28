# Base44 CLI

How to connect this checkout to a [Base44](https://docs.base44.com) app.

Verified against `base44` v0.1.6 (npm package name is `base44`, not `@base44/cli`).

## Requirements

- Node.js `>=20.19.0` (the CLI package declares this in `engines`).

## Install

```bash
npm install -g base44
```

Or invoke it without installing:

```bash
npx base44 <command>
```

## 1. Authenticate

```bash
base44 login
```

This uses the OAuth **device-code** flow: the CLI prints a verification code and
a URL, and you confirm the code in any browser. The machine running the CLI does
not need its own browser, so this works over SSH and in containers.

Credentials are written to `~/.base44/` (outside the repo).

Check who you are:

```bash
base44 whoami
```

## 2. Connect this repo to an app

Pick the case that matches your situation. All three end with the same two
files: a committed `base44/config.jsonc` and a gitignored `base44/.app.jsonc`.

### A. The Base44 app already exists

```bash
base44 scaffold --app-id <app_id>       # wire the current directory to that app
```

To download the app's existing code instead of just scaffolding config:

```bash
base44 eject --app-id <app_id> --path .
```

### B. No app exists yet

```bash
base44 create PSP-Emulator --path .     # -t backend-only for a config-only scaffold
```

This creates the app on Base44 *and* writes both `base44/config.jsonc` and
`base44/.app.jsonc`, so no separate `link` step is needed. Note that `--path`
requires the name argument, and that `create` rejects `--app-id` /
`BASE44_APP_ID` — it always creates a new app.

### C. Fresh clone where `base44/config.jsonc` is already committed

`.app.jsonc` is gitignored, so a new clone has project config but no app link.
Restore it with:

```bash
base44 link                             # select an existing app interactively
base44 link --create --name PSP-Emulator  # or create a new app for this config
```

`link` requires `base44/config.jsonc` to exist; conversely `create` and
`scaffold` refuse to run in a directory that already has one (`A Base44 project
already exists at ...`). So let the CLI generate that file — don't hand-write it.

### Which file is which

- `base44/config.jsonc` — app name, `visibility`, `site` build settings,
  and the `entities` / `functions` / `agents` / `connectors` directories.
  **Commit this.**
- `base44/.app.jsonc` — just `{"id": "<app_id>"}`. Per-checkout state,
  **gitignored on purpose**.

## Targeting an app without a link file

Every command accepts an explicit app ID, which overrides both the link file and
the environment:

```bash
base44 <command> --app-id <app_id>
export BASE44_APP_ID=<app_id>         # equivalent, for a whole shell session
```

## Headless / CI authentication

`base44 login` is interactive. For automation, authenticate with environment
variables instead:

| Variable | Purpose |
| --- | --- |
| `BASE44_API_KEY` | Workspace API key (prefixed `b44k_`). Preferred for CI. |
| `BASE44_ACCESS_TOKEN` + `BASE44_REFRESH_TOKEN` | Seeds the credential file from an existing token pair. Both are required. |
| `BASE44_APP_ID` | Target app, instead of `--app-id`. |
| `BASE44_DISABLE_TELEMETRY` | Set to disable the CLI's PostHog telemetry. |

Add `--json` to any command for scripting: stdout becomes a single JSON
document, prompts and status output are suppressed, diagnostics go to stderr,
and failures print `{"error": ..., "code": ..., "hints": [...]}` with a non-zero
exit code.

```bash
base44 sandbox ls src --app-id <app_id> --json | jq '.entries'
```

## Common commands

| Command | Description |
| --- | --- |
| `base44 deploy` | Deploy all resources (entities, functions, agents, connectors, site) |
| `base44 dev` | Start the development server (requires a linked project) |
| `base44 entities push` | Push local entity schemas |
| `base44 functions deploy` | Deploy backend functions |
| `base44 secrets set` / `list` / `delete` | Manage project secrets |
| `base44 logs` | Fetch function logs |
| `base44 sandbox <ls\|read\|write\|run>` | Operate on the app's remote sandbox |
| `base44 types generate` | Generate TypeScript types from project resources |
| `base44 visibility <public\|private\|workspace>` | Set app visibility |

Full list: `base44 --help`, or `base44 <command> --help`.

## Network requirements

The CLI needs outbound HTTPS to:

- `app.base44.com` — API and OAuth endpoints (override with `BASE44_API_URL`)
- `npm.jsr.io` — the CLI's `@deno/loader` dependency is pinned to a tarball on
  this host, so `npm install base44` fails without it

In a sandboxed environment with an egress allowlist (for example a Claude Code
web session), both hosts must be allowlisted or install and login fail with
`Host not in allowlist` / `{"error":"fetch failed"}`.
