---
name: google-workspace
description: Use for any Google Workspace or Google account task through the profile-aware `gw` CLI - Gmail (read, search, triage, label, draft, send, reply, forward), Google Calendar (agenda, events, availability, invites), Google Tasks, Google Drive, Docs, Sheets, Slides, Forms, Google Chat, Google Keep, Contacts, and Workspace Admin. Also use when the request involves multiple Google accounts, choosing between personal and work mail, switching Google identities, listing configured accounts, or authenticating a Google account. Triggers include "my email", "my inbox", "unread mail", "check my calendar", "what is on my schedule", "book a meeting", "send an email", "draft a reply", "my tasks", "my Drive files", "share a doc", "which Google accounts do I have".
---

# Google Workspace via `gw`

`gw` is a thin profile facade around `gws` (the Google Workspace CLI).
A profile is one named Google account with its own fully isolated credential, token, and configuration state.

`gw` selects the identity. `gws` does the Google work.

## Invocation

```bash
gw <profile> <gws arguments...>
```

Everything after the profile name is passed to `gws` unchanged.

Never invoke `gws` directly. Raw `gws` has no profile isolation, so it can silently act as the wrong Google account.

## Rules

1. Always invoke `gw`. Never invoke `gws`.
2. Every Google Workspace operation names an explicit profile. There is no default profile, no environment fallback, and no "current account".
3. Discover the available profiles with `gw profile list --json` instead of assuming names.
4. Never silently choose a profile for a write. If the correct account is ambiguous, ask.
5. Every write targets exactly one profile. Do not fan a write out across profiles.
6. Reads may query several profiles independently when the user asked for that.
7. Preserve provenance. When results from more than one profile are combined, label every item with the profile and account it came from.
8. Prefer creating a Gmail draft when the user asks to draft, prepare, or review mail. Send only when the user asked to send.
9. Inspect `--help` or `gws schema` rather than guessing command syntax.
10. Use `--dry-run` first where the command supports it, especially for deletes, bulk label changes, calendar changes, and sharing changes.
11. Never print credentials, OAuth tokens, client IDs, client secrets, encryption keys, or any file under a profile's `gws` config directory. Never read that directory to answer a question. Never run `gw <profile> auth export`; it prints decrypted credentials.

## Discovering profiles

```bash
gw profile list
gw profile list --json
gw profile show <profile> --json
```

The JSON is profile metadata only:

```json
{"profiles":[{"name":"<profile>","account":"<email>","gws_config_dir":"<path>"}]}
```

Use `name` for invocation and `account` for provenance. `gws_config_dir` is opaque; do not open it.

Which profile represents which role for a given user is local configuration, not part of this skill. If no mapping is provided, ask.

## Discovering commands

`gws` builds its command surface from the Google Discovery Service, so introspect instead of guessing:

```bash
gw <profile> --help
gw <profile> gmail --help
gw <profile> calendar events list --help
gw <profile> schema gmail.users.messages.list
```

## Command shape

`gws` exposes two kinds of command.

Helper commands are prefixed with `+` and take ordinary flags:

```bash
gw <profile> gmail +triage --max 5 --query 'from:someone@example.com'
gw <profile> gmail +read --id <ID>
gw <profile> gmail +send --to someone@example.com --subject "..." --body "..."
gw <profile> calendar +agenda
```

Generated commands mirror the Discovery resource path and take JSON:

```bash
gw <profile> gmail users drafts create --json '{"message":{"raw":"..."}}'
gw <profile> drive files list --params '{"pageSize":10}'
gw <profile> tasks tasklists list
gw <profile> calendar events list --params '{"calendarId":"primary"}'
```

Prefer a helper when one exists. Confirm any command with `--help` or `gws schema <service.resource.method>` before running it.

Useful global flags: `--dry-run`, `--format json|table|yaml|csv`, `--page-all`.

## Multiple accounts

For a cross-account read, run one command per profile and keep the results attributed:

```bash
gw <profile-a> calendar +agenda
gw <profile-b> calendar +agenda
```

Then report them as separate, labelled sets. Do not merge them into one undifferentiated list, and do not imply a single unified calendar or inbox exists.

## Authentication

```bash
gw <profile> auth login
gw <profile> auth status
```

Authentication is per profile and uses normal `gws` behaviour. After logging in, confirm the identity with an ordinary API call (for example a Gmail profile or user-info read) rather than by inspecting credential files.

If a command fails with an authentication or permission error, report it and suggest `gw <profile> auth login`. Do not try another profile to make the command succeed.

## Service availability

- Gmail, Calendar, Tasks, Drive, Docs, Sheets, Slides: generally available on both consumer and Workspace accounts.
- Google Chat: treat as a Workspace capability. Do not assume it works on a consumer Google account.
- Google Keep: the API is Workspace and admin gated. Do not assume it works on a consumer Google account.
- Admin, Reports, and directory commands require Workspace admin rights.

If a service is unavailable for the selected account, say so. Do not substitute a different account or invent the data.

## Failure handling

- `error: profile required` means no profile was given. Re-run with an explicit profile.
- `error: unknown profile: "..."` means the profile does not exist. Run `gw profile list --json`.
- `error: invalid profile name: "..."` means the name was rejected as unsafe. Use a name from `gw profile list`.

Never work around these by falling back to `gws`.
